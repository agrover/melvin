#![feature(convert)]

extern crate byteorder;

use std::fs::File;
use std::io::Read;
use std::io::{Result, Error};
use std::io::ErrorKind::Other;

use byteorder::{LittleEndian, BigEndian, ByteOrder};

const LABEL_SCAN_SECTORS: usize = 4;
const MYPATH: &'static str = "/dev/mapper/luks-291e9250-200e-4c36-8e5e-66aa257669ed";

#[derive(Debug)]
struct Label {
    id: [u8; 8],
    sector: u64,
    crc: u32,
    offset: u32,
    type_: [u8; 8],
}

fn array_from_slice(slc: &[u8]) -> [u8; 8] {
    let mut array = [0u8; 8];
    for (&x, p) in slc.iter().zip(array.iter_mut()) {
        *p = x;
    }
    array
}

fn find_label() -> Result<Label> {

    let mut f = try!(File::open(MYPATH));

    let mut buf = vec![0; LABEL_SCAN_SECTORS * 512];

    try!(f.read(&mut buf));

    let label: Vec<u8> = "LABELONE".bytes().collect();

    for x in 0..LABEL_SCAN_SECTORS {
        if &buf[x*512..x*512+8] == label.as_slice() {
            let start = x*512;

            return Ok(Label{
                id: array_from_slice(&buf[start..start+8]),
                sector: LittleEndian::read_u64(&buf[start+8..start+16]),
                crc: LittleEndian::read_u32(&buf[start+16..start+20]),
                offset: LittleEndian::read_u32(&buf[start+20..start+24]),
                type_: array_from_slice(&buf[start+24..start+32]),
            })
        }
    }

    Err(Error::new(Other, "bad"))
}

fn main() {

    println!("{:?}", find_label());

}
