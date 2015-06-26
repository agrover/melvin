#![feature(iter_arith)]
#![feature(collections)]

extern crate byteorder;
extern crate crc;
extern crate unix_socket;
extern crate nix;
extern crate libc;

use std::path;
use std::io::Result;
use std::io::Error;
use std::io::ErrorKind::Other;

mod parser;
mod lvmetad;
mod pvlabel;
mod dm;
mod lv;
mod vg;
mod pv;

#[allow(dead_code, non_camel_case_types)]
mod dm_ioctl;

use parser::LvmTextMap;

fn get_first_vg_meta() -> Result<(String, LvmTextMap)> {
    let dirs = vec![path::Path::new("/dev")];

    for pv in try!(pvlabel::scan_for_pvs(&dirs)) {
        let map = try!(pvlabel::textmap_from_dev(pv.as_path()));

        for (key, value) in map {
            match value {
                parser::Entry::TextMap(x) => return Ok((key, *x)),
                _ => {}
            }
        }
    }

    Err(Error::new(Other, "dude"))
}

fn main() {
    // println!("A");
    // let (name, mut map) = get_first_vg_meta().unwrap();
    // println!("B");
    // let vg = parser::vg_from_textmap(&name, &mut map);
    // println!("output {:?}", vg);

    match dm::list_devices() {
        Ok(x) => println!("{:?}", x),
        Err(x) => println!("error {}", x),
    }
}
