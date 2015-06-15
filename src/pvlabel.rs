use std::io::{Read, Result, Error, Seek, SeekFrom};
use std::io::ErrorKind::Other;
use std::path::{Path, PathBuf};
use std::fs;

use byteorder::{LittleEndian, ByteOrder};
use crc::{crc32, Hasher32};
use nix::sys::stat;

use parser;

use parser::{LvmTextMap, into_textmap};

const LABEL_SCAN_SECTORS: usize = 4;
const ID_LEN: usize = 32;
const MDA_MAGIC: &'static [u8] = b"\x20\x4c\x56\x4d\x32\x20\x78\x5b\x35\x41\x25\x72\x30\x4e\x2a\x3e";
const INITIAL_CRC: u32 = 0xf597a6cf;
const SECTOR_SIZE: usize = 512;

#[derive(Debug)]
struct LabelHeader {
    id: String,
    sector: u64,
    crc: u32,
    offset: u32,
    label: String,
}

#[derive(Debug)]
struct PvArea {
    offset: u64,
    size: u64,
}

#[derive(Debug)]
struct PvHeader {
    uuid: String,
    size: u64, // in bytes
    ext_version: u32,
    ext_flags: u32,
    data_areas: Vec<PvArea>,
    metadata_areas: Vec<PvArea>,
    bootloader_areas: Vec<PvArea>,
}


fn get_label_header(buf: &[u8]) -> Result<LabelHeader> {

    for x in 0..LABEL_SCAN_SECTORS {
        let sec_buf = &buf[x*SECTOR_SIZE..x*SECTOR_SIZE+SECTOR_SIZE];
        if &sec_buf[..8] == b"LABELONE" {
            let crc = LittleEndian::read_u32(&sec_buf[16..20]);
            crc32_ok(crc, &sec_buf[20..SECTOR_SIZE]);

            let sector = LittleEndian::read_u64(&sec_buf[8..16]);
            if sector != x as u64 {
                return Err(Error::new(Other, "Sector field should equal sector count"));
            }

            return Ok(LabelHeader{
                id: String::from_utf8_lossy(&sec_buf[..8]).into_owned(),
                sector: sector,
                crc: crc,
                offset: LittleEndian::read_u32(&sec_buf[20..24]) + (x*SECTOR_SIZE as usize) as u32,
                label: String::from_utf8_lossy(&sec_buf[24..32]).into_owned(),
            })
        }
    }

    Err(Error::new(Other, "Label not found"))
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

//
// PV HEADER LAYOUT:
// - static header (uuid and size)
// - 0+ data areas (actually max 1, usually 1; size 0 == "rest of blkdev")
// - blank entry
// - 0+ metadata areas (max 1, usually 1)
// - blank entry
// - 8 bytes of pvextension header
// - if version > 0
//   - 0+ bootloader areas (usually 0)
//
fn get_pv_header(buf: &[u8]) -> Result<PvHeader> {

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
        uuid: String::from_utf8_lossy(&buf[..ID_LEN]).into_owned(),
        size: LittleEndian::read_u64(&buf[ID_LEN..ID_LEN+8]),
        ext_version: ext_version,
        ext_flags: ext_flags,
        data_areas: da_vec,
        metadata_areas: md_vec,
        bootloader_areas: ba_vec,
    })
}

fn crc32_ok(val: u32, buf: &[u8]) -> bool {
    let mut digest = crc32::Digest::new(INITIAL_CRC);
    digest.write(&buf);
    let crc32 = digest.sum32();

    // TODO: all our crcs are failing, how come?
    if val != crc32 {
        println!("CRC32: input {:x} != calculated {:x}", val, crc32);
    }
    val == crc32
}

#[derive(Debug)]
struct RawLocn {
    offset: u64,
    size: u64,
    checksum: u32,
    flags: u32,
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
                flags: flags,
            })
        }
    }
}

fn find_pv_in_dev(path: &Path) -> Result<PvHeader> {

    let mut f = try!(fs::File::open(path));

    let mut buf = vec![0; LABEL_SCAN_SECTORS * 512];

    try!(f.read(&mut buf));

    let label_header = try!(get_label_header(&buf));
    let pvheader = try!(get_pv_header(&buf[label_header.offset as usize..]));

    return Ok(pvheader);
}

pub fn textmap_from_dev(path: &Path) -> Result<LvmTextMap> {

    let pvheader = try!(find_pv_in_dev(&path));

    let mut f = try!(fs::File::open(path));

    let mda_areas = pvheader.metadata_areas.len();
    if mda_areas != 1 {
        return Err(Error::new(Other, format!("Expecting 1 mda, found {}", mda_areas)));
    }

    let md = &pvheader.metadata_areas[0];
    try!(f.seek(SeekFrom::Start(md.offset)));
    let mut buf = vec![0; md.size as usize];
    try!(f.read(&mut buf));

    crc32_ok(LittleEndian::read_u32(&buf[..4]), &buf[4..512]);

    if &buf[4..20] != MDA_MAGIC {
        return Err(Error::new(
            Other, format!("'{}' doesn't match MDA_MAGIC",
                           String::from_utf8_lossy(&buf[4..20]))));
    }

    let ver = LittleEndian::read_u32(&buf[20..24]);
    if ver != 1 {
        return Err(Error::new(Other, "Bad version, expected 1"));
    }

    // TODO: validate these somehow
    //println!("mdah start {}", LittleEndian::read_u64(&buf[24..32]));
    //println!("mdah size {}", LittleEndian::read_u64(&buf[32..40]));

    let raw_locns: Vec<_> = iter_raw_locn(&buf[40..]).collect();
    let rlocn_len = raw_locns.len();
    if rlocn_len != 1 {
        return Err(Error::new(Other, format!("Expecting 1 rlocn, found {}", rlocn_len)));
    }

    let rl = &raw_locns[0];
    let rl_start = rl.offset as usize;
    let rl_end = rl_start + rl.size as usize;

    parser::into_textmap(&buf[rl_start..rl_end])
}

pub fn scan_for_pvs(dirs: &[&Path]) -> Result<Vec<PathBuf>> {

    let mut ret_vec = Vec::new();

    for dir in dirs {
        for direntry in try!(fs::read_dir(dir)) {
            let path = direntry.unwrap().path();
            let s = stat::stat(&path).unwrap();

            if (s.st_mode & 0x6000) == 0x6000 { // S_IFBLK
                if find_pv_in_dev(&path).is_ok() {
                    ret_vec.push(path);
                }
            }
        }
    }

    Ok(ret_vec)
}

