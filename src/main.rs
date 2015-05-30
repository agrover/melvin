extern crate byteorder;
extern crate crc;

use std::fs::File;
use std::io::{Read, Result, Error, Seek, SeekFrom};
use std::io::ErrorKind::Other;

use byteorder::{LittleEndian, ByteOrder};

use crc::{crc32, Hasher32};

const LABEL_SCAN_SECTORS: usize = 4;
const MYPATH: &'static str = "/dev/mapper/luks-291e9250-200e-4c36-8e5e-66aa257669ed";
const ID_LEN: usize = 32;
const MDA_MAGIC: &'static str = "\x20\x4c\x56\x4d\x32\x20\x78\x5b\x35\x41\x25\x72\x30\x4e\x2a\x3e";
const INITIAL_CRC: u32 = 0xf597a6cf;
const SECTOR_SIZE: usize = 512;


#[derive(Debug)]
struct Label {
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


fn get_label(buf: &[u8]) -> Result<Label> {
    for x in 0..LABEL_SCAN_SECTORS {
        let sec_buf = &buf[x*SECTOR_SIZE..x*SECTOR_SIZE+SECTOR_SIZE];
        if &sec_buf[..8] == b"LABELONE" {
            let crc = LittleEndian::read_u32(&sec_buf[16..20]);
            crc32_ok(crc, &sec_buf[20..SECTOR_SIZE]);

            let sector = LittleEndian::read_u64(&sec_buf[8..16]);
            if sector != x as u64 {
                println!("sector field is {} in sector {}", sector, x);
            }

            return Ok(Label{
                id: String::from_utf8_lossy(&sec_buf[..8]).into_owned(),
                sector: sector,
                crc: crc,
                offset: LittleEndian::read_u32(&sec_buf[20..24]) + (x*SECTOR_SIZE as usize) as u32,
                label: String::from_utf8_lossy(&sec_buf[24..32]).into_owned(),
            })
        }
    }

    Err(Error::new(Other, "bad"))
}

fn get_pv_header(buf: &[u8]) -> Result<PvHeader> {

    let mut da_buf = &buf[ID_LEN+8..];
    let mut da_vec = Vec::new();

    loop {
        let da_off = LittleEndian::read_u64(&da_buf[..8]);
        if da_off == 0 { break; }

        da_vec.push(PvArea {
            offset: da_off,
            size: LittleEndian::read_u64(&da_buf[8..16]),
        });

        da_buf = &da_buf[16..];
    }

    let mut md_vec = Vec::new();

    // metadata list is after a null entry for the data areas
    da_buf = &da_buf[16..];

    loop {
        let da_off = LittleEndian::read_u64(&da_buf[..8]);
        if da_off == 0 { break; }

        md_vec.push(PvArea {
            offset: da_off,
            size: LittleEndian::read_u64(&da_buf[8..16]),
        });

        da_buf = &da_buf[16..];
    }

    // pv extension is after a null entry for the metadata areas
    da_buf = &da_buf[16..];

    let ext_version = LittleEndian::read_u32(&da_buf[..4]);
    let mut ext_flags = 0;
    let mut ba_vec = Vec::new();

    if ext_version != 0 {
        ext_flags = LittleEndian::read_u32(&da_buf[4..8]);

        da_buf = &da_buf[8..];

        loop {
            let da_off = LittleEndian::read_u64(&da_buf[..8]);
            if da_off == 0 { break; }

            ba_vec.push(PvArea {
                offset: da_off,
                size: LittleEndian::read_u64(&da_buf[8..16]),
            });

            da_buf = &da_buf[16..];
        }
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
    if val != crc32 {
        println!("CRC32: input {:x} != calculated {:x}", val, crc32);
    }
    val == crc32
}


fn parse_mda_header(buf: &[u8]) -> () {

    let crc1 = LittleEndian::read_u32(&buf[..4]);

    // TODO: why is this failing?
    crc32_ok(crc1, &buf[4..512]);
}

fn find_stuff(path: &str) -> Result<Label> {

    let mut f = try!(File::open(path));

    let mut buf = vec![0; LABEL_SCAN_SECTORS * 512];

    try!(f.read(&mut buf));

    let label = try!(get_label(&buf));

    let pvheader = try!(get_pv_header(&buf[label.offset as usize..]));

    for md in &pvheader.metadata_areas {
        try!(f.seek(SeekFrom::Start(md.offset)));

        println!("AA {} {}", md.offset, md.size);

        let mut buf = vec![0; md.size as usize];

        try!(f.read(&mut buf));

        parse_mda_header(&buf);
    }

    println!("pvheader {:?}", pvheader);

    return Ok(label);
}

fn main() {

    let label = find_stuff(MYPATH).unwrap();

    println!("{:?}", label);
}
