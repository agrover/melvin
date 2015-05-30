
extern crate byteorder;

use std::fs::File;
use std::io::Read;
use std::io::{Result, Error};
use std::io::ErrorKind::Other;

use byteorder::{LittleEndian, ByteOrder};

const LABEL_SCAN_SECTORS: usize = 4;
const MYPATH: &'static str = "/dev/mapper/luks-291e9250-200e-4c36-8e5e-66aa257669ed";
const ID_LEN: usize = 32;

#[derive(Debug)]
struct Label {
    id: String,
    sector: u64,
    crc: u32,
    offset: u32,
    type_: String,
}

#[derive(Debug)]
struct DataArea {
    offset: u64,
    size: u64,
}

#[derive(Debug)]
struct MetadataArea {
    offset: u64,
    size: u64,
}

#[derive(Debug)]
struct BootloaderArea {
    offset: u64,
    size: u64,
}

#[derive(Debug)]
struct PvHeader {
    uuid: String,
    size: u64, // in bytes
    ext_version: u32,
    ext_flags: u32,
    data_areas: Vec<DataArea>,
    metadata_areas: Vec<MetadataArea>,
    bootloader_areas: Vec<BootloaderArea>,
}


fn get_label(buf: &[u8]) -> Result<Label> {
    for x in 0..LABEL_SCAN_SECTORS {
        if &buf[x*512..x*512+8] == b"LABELONE" {
            let start = x*512;

            return Ok(Label{
                id: String::from_utf8_lossy(&buf[start..start+8]).into_owned(),
                sector: LittleEndian::read_u64(&buf[start+8..start+16]),
                crc: LittleEndian::read_u32(&buf[start+16..start+20]),
                offset: LittleEndian::read_u32(&buf[start+20..start+24]) + start as u32,
                type_: String::from_utf8_lossy(&buf[start+24..start+32]).into_owned(),
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

        da_vec.push(DataArea {
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

        md_vec.push(MetadataArea {
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

            ba_vec.push(BootloaderArea {
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

fn find_stuff(path: &str) -> Result<Label> {

    let mut f = try!(File::open(path));

    let mut buf = vec![0; LABEL_SCAN_SECTORS * 512];

    try!(f.read(&mut buf));

    let label = try!(get_label(&buf));

    let pvheader = try!(get_pv_header(&buf[label.offset as usize..]));

    println!("pvheader {:?}", pvheader);

    return Ok(label);
}

fn main() {

    let label = find_stuff(MYPATH).unwrap();

    println!("{:?}", label);

}
