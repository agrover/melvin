// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Reading and writing LVM on-disk labels and metadata.

//
// label is at start of sectors 0-3, usually 1
// label includes offset of pvheader (also within 1st 4 sectors)
// pvheader includes ptrs to data (1), metadata(0-2), and boot(0-1) areas
// metadata area (MDA), located anywhere, starts with 512b mda header, then
//   large text area
// mda header has 40b of stuff, then rlocns[].
// rlocns point into mda text area. rlocn 0 used for text metadata, rlocn 1
//   points to precommitted data (not currently supported by Melvin)
// text metadata written aligned to sector-size; text area treated as circular
//   and text may wrap across end to beginning
// text metadata contains vg metadata in lvm config text format. Each write
//   increments seqno.
//

use std::io;
use std::io::{Read, Write, Result, Error, Seek, SeekFrom};
use std::io::ErrorKind::Other;
use std::path::{Path, PathBuf};
use std::fs::{File, read_dir, OpenOptions};
use std::cmp::min;
use std::slice::bytes::copy_memory;
use std::os::unix::io::AsRawFd;
use std::str::FromStr;

use byteorder::{LittleEndian, ByteOrder};
use nix::sys::{stat, ioctl};

use parser::{LvmTextMap, textmap_to_buf, buf_to_textmap, Entry};
use util::{align_to, crc32_calc, make_uuid, hyphenate_uuid};
use pv::Device;
use lvmetad;

const LABEL_SCAN_SECTORS: usize = 4;
const ID_LEN: usize = 32;
const MDA_MAGIC: &'static [u8] = b"\x20\x4c\x56\x4d\x32\x20\x78\x5b\x35\x41\x25\x72\x30\x4e\x2a\x3e";
const LABEL_SIZE: usize = 32;
const LABEL_SECTOR: usize = 1;
pub const SECTOR_SIZE: usize = 512;
const MDA_HEADER_SIZE: usize = 512;
const DEFAULT_MDA_SIZE: u64 = (1024 * 1024);
const EXTENSION_VERSION: u32 = 1;

#[derive(Debug)]
struct LabelHeader {
    id: String,
    sector: u64,
    crc: u32,
    offset: u32,
    label: String,
}

impl LabelHeader {
    fn from_buf(buf: &[u8]) -> Result<LabelHeader> {
        for x in 0..LABEL_SCAN_SECTORS {
            let sec_buf = &buf[x*SECTOR_SIZE..x*SECTOR_SIZE+SECTOR_SIZE];
            if &sec_buf[..8] == b"LABELONE" {
                let crc = LittleEndian::read_u32(&sec_buf[16..20]);
                if crc != crc32_calc(&sec_buf[20..SECTOR_SIZE]) {
                    return Err(Error::new(Other, "Label CRC error"));
                }

                let sector = LittleEndian::read_u64(&sec_buf[8..16]);
                if sector != x as u64 {
                    return Err(Error::new(Other, "Sector field should equal sector count"));
                }

                return Ok(LabelHeader{
                    id: String::from_utf8_lossy(&sec_buf[..8]).into_owned(),
                    sector: sector,
                    crc: crc,
                    // switch from "offset from label" to "offset from start", more convenient.
                    offset: LittleEndian::read_u32(&sec_buf[20..24]) + (x*SECTOR_SIZE as usize) as u32,
                    label: String::from_utf8_lossy(&sec_buf[24..32]).into_owned(),
                })
            }
        }

        Err(Error::new(Other, "Label not found"))
    }

    /// Initialize a device with a label header.
    fn initialize(sec_buf: &mut [u8; SECTOR_SIZE]) -> () {
        copy_memory(b"LABELONE", &mut sec_buf[..8]);
        LittleEndian::write_u64(&mut sec_buf[8..16], LABEL_SECTOR as u64);
        LittleEndian::write_u32(&mut sec_buf[20..24], LABEL_SIZE as u32);
        copy_memory(b"LVM2 001", &mut sec_buf[24..32]);
        let crc_val = crc32_calc(&sec_buf[20..]);
        LittleEndian::write_u32(&mut sec_buf[16..20], crc_val);
    }
}

/// Describes an area within a PV
#[derive(Debug, PartialEq, Clone, Copy)]
pub struct PvArea {
    /// The offset from the start of the device in bytes
    pub offset: u64,
    /// The size in bytes
    pub size: u64,
}

#[derive(Debug)]
struct PvAreaIter<'a> {
    area: &'a[u8],
}

fn iter_pv_area<'a>(buf: &'a[u8]) -> PvAreaIter<'a> {
    PvAreaIter { area: buf }
}

impl<'a> Iterator for PvAreaIter<'a> {
    type Item = PvArea;

    fn next (&mut self) -> Option<PvArea> {
        let off = LittleEndian::read_u64(&self.area[..8]);
        let size = LittleEndian::read_u64(&self.area[8..16]);

        if off == 0 {
            None
        }
        else {
            self.area = &self.area[16..];
            Some(PvArea {
                offset: off,
                size: size,
            })
        }
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
struct RawLocn {
    offset: u64,
    size: u64,
    checksum: u32,
    ignored: bool,
}

#[derive(Debug)]
struct RawLocnIter<'a> {
    area: &'a[u8],
}

fn iter_raw_locn<'a>(buf: &'a[u8]) -> RawLocnIter<'a> {
    RawLocnIter { area: buf }
}

impl<'a> Iterator for RawLocnIter<'a> {
    type Item = RawLocn;

    fn next (&mut self) -> Option<RawLocn> {
        let off = LittleEndian::read_u64(&self.area[..8]);
        let size = LittleEndian::read_u64(&self.area[8..16]);
        let checksum = LittleEndian::read_u32(&self.area[16..20]);
        let flags = LittleEndian::read_u32(&self.area[20..24]);

        if off == 0 {
            None
        }
        else {
            self.area = &self.area[24..];
            Some(RawLocn {
                offset: off,
                size: size,
                checksum: checksum,
                ignored: (flags & 1) > 0,
            })
        }
    }
}

/// A block device that has been initialized to be a LVM Physical
/// Volume, but that may not be part of a VG yet.
#[derive(Debug, PartialEq, Clone)]
pub struct PvHeader {
    /// The unique identifier.
    pub uuid: String,
    /// Size in bytes of the entire PV.
    pub size: u64,
    /// Extension version. If 1, we look for an extension header that may contain a reference
    /// to a bootloader area.
    ext_version: u32,
    /// Extension flags, of which there are none.
    ext_flags: u32,
    /// A list of the data areas.
    pub data_areas: Vec<PvArea>,
    /// A list of the metadata areas.
    pub metadata_areas: Vec<PvArea>,
    /// A list of the bootloader areas.
    pub bootloader_areas: Vec<PvArea>,
    /// The path to the device this pvheader is within.
    pub dev_path: PathBuf,
}

impl PvHeader {
    //
    // PV HEADER LAYOUT:
    // - static header (uuid and size)
    // - 0+ data areas (actually max 1, usually 1; size 0 == "rest of blkdev")
    //   Remember to subtract mda1 size if present.
    // - blank entry
    // - 0+ metadata areas (max 2, usually 1)
    // - blank entry
    // - 8 bytes of pvextension header
    // - if version > 0
    //   - 0+ bootloader areas (usually 0)
    //
    // Parse a buf containing the on-disk pvheader and create a struct
    // representing it.
    fn from_buf(buf: &[u8], path: &Path) -> Result<PvHeader> {

        let mut da_buf = &buf[ID_LEN+8..];

        let da_vec: Vec<_> = iter_pv_area(da_buf).collect();

        // move slice past any actual entries plus blank
        // terminating entry
        da_buf = &da_buf[(da_vec.len()+1)*16..];

        let md_vec: Vec<_> = iter_pv_area(da_buf).collect();

        da_buf = &da_buf[(md_vec.len()+1)*16..];

        let ext_version = LittleEndian::read_u32(&da_buf[..4]);
        let mut ext_flags = 0;
        let mut ba_vec = Vec::new();

        if ext_version != 0 {
            ext_flags = LittleEndian::read_u32(&da_buf[4..8]);

            da_buf = &da_buf[8..];

            ba_vec = iter_pv_area(da_buf).collect();
        }

        Ok(PvHeader{
            uuid: hyphenate_uuid(&buf[..ID_LEN]),
            size: LittleEndian::read_u64(&buf[ID_LEN..ID_LEN+8]),
            ext_version: ext_version,
            ext_flags: ext_flags,
            data_areas: da_vec,
            metadata_areas: md_vec,
            bootloader_areas: ba_vec,
            dev_path: path.to_owned(),
        })
    }

    /// Find the PvHeader struct in a given device.
    pub fn find_in_dev(path: &Path) -> Result<PvHeader> {

        let mut f = try!(File::open(path));

        let mut buf = [0u8; LABEL_SCAN_SECTORS * SECTOR_SIZE];

        try!(f.read(&mut buf));

        let label_header = try!(LabelHeader::from_buf(&buf));
        let pvheader = try!(Self::from_buf(&buf[label_header.offset as usize..], path));

        return Ok(pvheader);
    }

    fn blkdev_size(file: &File) -> Result<u64> {
        // BLKGETSIZE64
        let op = ioctl::op_read(0x12, 114, 8);
        let mut val: u64 = 0;

        match unsafe { ioctl::read_into(file.as_raw_fd(), op, &mut val) } {
            Err(_) => return Err((io::Error::last_os_error())),
            Ok(_) => Ok(val),
        }
    }

    /// Initialize a device as a PV with reasonable defaults: two metadata
    /// areas, no bootsector area, and size based on the device's size.
    pub fn initialize(path: &Path) -> Result<PvHeader> {

        let mut f = try!(OpenOptions::new().write(true).open(path));

        // mda0 starts at 9th sector
        let mda0_offset = (8 * SECTOR_SIZE) as u64;
        // mda0's length is reduced a little by the header length,
        // maybe to keep the data area aligned to 1MB?
        let mda0_length = DEFAULT_MDA_SIZE - mda0_offset;
        let dev_size = try!(Self::blkdev_size(&f));

        if dev_size < ((DEFAULT_MDA_SIZE * 2) + mda0_offset) {
            return Err(Error::new(Other, "Device too small"));
        }

        let pvh = PvHeader{
            uuid: make_uuid(),
            size: dev_size,
            ext_version: EXTENSION_VERSION,
            ext_flags: 0,
            data_areas: vec![
                // da0 length is not used
                PvArea { offset: mda0_offset + mda0_length, size: 0 },
                ],
            metadata_areas: vec![
                PvArea { offset: mda0_offset, size: mda0_length },
                PvArea { offset: dev_size - DEFAULT_MDA_SIZE, size: DEFAULT_MDA_SIZE },
                ],
            bootloader_areas: Vec::new(),
            dev_path: path.to_owned(),
        };

        let mut sec_buf = [0u8; SECTOR_SIZE];

        // Translate to on-disk format
        {
            let mut slc = &mut sec_buf[LABEL_SIZE..];

            let uuid = pvh.uuid.replace("-", "");
            copy_memory(uuid.as_bytes(), &mut slc[..ID_LEN]);
            let mut slc = &mut slc[ID_LEN..];

            LittleEndian::write_u64(slc, dev_size);
            let mut slc = &mut slc[8..];

            // da0 defined first, but "in the middle"
            LittleEndian::write_u64(slc, pvh.data_areas[0].offset);
            let mut slc = &mut slc[8..];
            LittleEndian::write_u64(slc, pvh.data_areas[0].size);
            let mut slc = &mut slc[8..];

            // skip 16 bytes to indicate end of da list
            let mut slc = &mut slc[16..];

            // mda0 at start of PV
            LittleEndian::write_u64(slc, pvh.metadata_areas[0].offset);
            let mut slc = &mut slc[8..];
            LittleEndian::write_u64(slc, pvh.metadata_areas[0].size);
            let mut slc = &mut slc[8..];

            // mda1 at end of PV
            LittleEndian::write_u64(slc, pvh.metadata_areas[1].offset);
            let mut slc = &mut slc[8..];
            LittleEndian::write_u64(slc, pvh.metadata_areas[1].size);
            let mut slc = &mut slc[8..];

            // skip 16 bytes to indicate end of mda list
            let mut slc = &mut slc[16..];

            // Extension header
            LittleEndian::write_u32(slc, pvh.ext_version);

            // everything else is 0 (no bas) so we're finished
        }

        // Must do label last since it calcs crc over everything
        LabelHeader::initialize(&mut sec_buf);

        try!(f.seek(SeekFrom::Start(LABEL_SECTOR as u64 * SECTOR_SIZE as u64)));
        try!(f.write_all(&mut sec_buf));

        for area in &pvh.metadata_areas {
            let new_rl = RawLocn {
                offset: 0,
                size: 0,
                checksum: 0,
                ignored: false,
            };
            try!(Self::write_mda_header(area, &mut f, &new_rl));
        }

        let map = pvh.clone().into();
        try!(lvmetad::pv_found(&map));

        Ok(pvh)
    }

    // For the moment, the only important thing in the MDA header is rlocn0,
    // so we don't need separate functions that return anything in it except
    // rlocn0.
    fn read_mda_header(area: &PvArea, file: &mut File) -> io::Result<Option<RawLocn>> {
        assert!(area.size as usize > MDA_HEADER_SIZE);
        try!(file.seek(SeekFrom::Start(area.offset)));
        let mut hdr = [0u8; MDA_HEADER_SIZE];
        try!(file.read(&mut hdr));

        if LittleEndian::read_u32(&hdr[..4]) != crc32_calc(&hdr[4..MDA_HEADER_SIZE]) {
            return Err(Error::new(Other, "MDA header checksum failure"));
        }

        if &hdr[4..20] != MDA_MAGIC {
            return Err(Error::new(
                Other, format!("'{}' doesn't match MDA_MAGIC",
                               String::from_utf8_lossy(&hdr[4..20]))));
        }

        let ver = LittleEndian::read_u32(&hdr[20..24]);
        if ver != 1 {
            return Err(Error::new(Other, "Bad version, expected 1"));
        }

        let start = LittleEndian::read_u64(&hdr[24..32]);
        if start != area.offset {
            return Err(Error::new(Other, format!("mdah start {} does not equal pvarea start {}",
                                                 start, area.offset)));
        }

        let size = LittleEndian::read_u64(&hdr[32..40]);
        if size != area.size {
            return Err(Error::new(Other, format!("mdah size {} does not equal pvarea size {}",
                                                 size, area.size)));
        }

        Ok(iter_raw_locn(&hdr[40..]).next())
    }

    fn write_mda_header(area: &PvArea, file: &mut File, rl: &RawLocn) -> io::Result<()> {
        let mut hdr = [0u8; MDA_HEADER_SIZE];

        copy_memory(MDA_MAGIC, &mut hdr[4..20]);
        LittleEndian::write_u32(&mut hdr[20..24], 1);
        LittleEndian::write_u64(&mut hdr[24..32], area.offset);
        LittleEndian::write_u64(&mut hdr[32..40], area.size);

        {
            let mut raw_locn = &mut hdr[40..];

            LittleEndian::write_u64(&mut raw_locn[..8], rl.offset);
            LittleEndian::write_u64(&mut raw_locn[8..16], rl.size);
            LittleEndian::write_u32(&mut raw_locn[16..20], rl.checksum);

            let flags = rl.ignored as u32;
            LittleEndian::write_u32(&mut raw_locn[20..24], flags);
        }

        let csum = crc32_calc(&hdr[4..]);
        LittleEndian::write_u32(&mut hdr[..4], csum);

        try!(file.seek(SeekFrom::Start(area.offset)));
        file.write_all(&hdr)
    }

    /// Read the metadata contained in the metadata area.
    /// In the case of multiple metadata areas, return the information
    /// from the first valid one.
    pub fn read_metadata(&self) -> io::Result<LvmTextMap> {
        let mut f = try!(OpenOptions::new().read(true).open(&self.dev_path));

        for pvarea in &self.metadata_areas {
            let rl = match try!(Self::read_mda_header(&pvarea, &mut f)) {
                None => continue,
                Some(x) => x,
            };

            if rl.ignored {
                continue
            }

            let mut text = vec![0; rl.size as usize];
            let first_read = min(pvarea.size - rl.offset, rl.size) as usize;

            try!(f.seek(SeekFrom::Start(pvarea.offset + rl.offset)));
            try!(f.read(&mut text[..first_read]));

            if first_read != rl.size as usize {
                try!(f.seek(SeekFrom::Start(
                    pvarea.offset + MDA_HEADER_SIZE as u64)));
                try!(f.read(&mut text[rl.size as usize - first_read..]));
            }

            if rl.checksum != crc32_calc(&text) {
                return Err(Error::new(Other, "MDA text checksum failure"));
            }

            return buf_to_textmap(&text);
        }

        return Err(Error::new(Other, "No valid metadata found"));
    }

    /// Write the given metadata to all active metadata areas in the PV.
    pub fn write_metadata(&mut self, map: &LvmTextMap) -> io::Result<()> {

        let mut f = try!(OpenOptions::new().read(true).write(true)
                         .open(&self.dev_path));

        let mut text = textmap_to_buf(map);
        // Ends with one null
        text.push(b'\0');

        for pvarea in &self.metadata_areas {
            // If this is the first write, supply an initial RawLocn template
            let rl = match try!(Self::read_mda_header(&pvarea, &mut f)) {
                None => RawLocn {
                    offset: MDA_HEADER_SIZE as u64,
                    size: 0,
                    checksum: 0,
                    ignored: false,
                },
                Some(x) => x,
            };

            if rl.ignored {
                continue
            }

            // start at next sector in loop, but skip 0th sector
            let start_off = min(MDA_HEADER_SIZE as u64,
                                (align_to(
                                    (rl.offset + rl.size) as usize,
                                    SECTOR_SIZE)
                                 % pvarea.size as usize) as u64);
            let tail_space = pvarea.size as u64 - start_off;

            assert_eq!(start_off % SECTOR_SIZE as u64, 0);
            assert_eq!(tail_space % SECTOR_SIZE as u64, 0);

            let written = if tail_space != 0 {
                try!(f.seek(
                    SeekFrom::Start(pvarea.offset + start_off)));
                try!(f.write_all(&text[..min(tail_space as usize, text.len())]));
                min(tail_space as usize, text.len())
            } else {
                0
            };

            if written != text.len() {
                try!(f.seek(
                    SeekFrom::Start(pvarea.offset + MDA_HEADER_SIZE as u64)));
                try!(f.write_all(&text[written as usize..]));
            }

            let new_rl = RawLocn {
                offset: start_off,
                size: text.len() as u64,
                checksum: crc32_calc(&text),
                ignored: rl.ignored,
            };
            try!(Self::write_mda_header(&pvarea, &mut f, &new_rl));
        }

        Ok(())
    }
}

impl From<PvHeader> for LvmTextMap {
    fn from(pvh: PvHeader) -> Self {
        let dev = Device::from_str(&pvh.dev_path.to_string_lossy()).unwrap();
        let devno: u64 = dev.into();

        let mut pvmeta = LvmTextMap::new();
        pvmeta.insert("device".to_string(), Entry::Number(devno as i64));
        pvmeta.insert("dev_size".to_string(), Entry::Number(pvh.size as i64));
        pvmeta.insert("format".to_string(), Entry::String("lvm2".to_string()));
        pvmeta.insert("label_sector".to_string(), Entry::Number(LABEL_SECTOR as i64));
        pvmeta.insert("id".to_string(), Entry::String(pvh.uuid.clone()));

        let mut da0 = Box::new(LvmTextMap::new());
        da0.insert("offset".to_string(), Entry::Number(pvh.data_areas[0].offset as i64));
        da0.insert("size".to_string(), Entry::Number(pvh.data_areas[0].size as i64));
        pvmeta.insert("da0".to_string(), Entry::TextMap(da0));

        for (num, pvarea) in pvh.metadata_areas.iter().enumerate() {
            let mut mda = Box::new(LvmTextMap::new());
            mda.insert("offset".to_string(), Entry::Number(pvarea.offset as i64));
            mda.insert("size".to_string(), Entry::Number(pvarea.size as i64));
            mda.insert("ignore".to_string(), Entry::Number(0i64));
            mda.insert("free_sectors".to_string(), Entry::Number(0i64));
            pvmeta.insert(format!("mda{}", num), Entry::TextMap(mda));
        }

        pvmeta
    }
}

/// Scan a list of directories for block devices containing LVM PV labels.
pub fn pvheader_scan(dirs: &[&Path]) -> Result<Vec<PvHeader>> {

    let mut ret_vec = Vec::new();

    for dir in dirs {
        ret_vec.extend(try!(read_dir(dir))
            .into_iter()
            .filter_map(|dir_e| if dir_e.is_ok()
                        { Some(dir_e.unwrap().path()) } else {None} )
            .filter(|path| {
                (stat::stat(path).unwrap().st_mode & 0x6000) == 0x6000 }) // S_IFBLK
            .filter_map(|path| { PvHeader::find_in_dev(&path).ok() })
            .collect::<Vec<_>>());
    }

    Ok(ret_vec)
}
