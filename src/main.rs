#![feature(iter_arith, result_expect)]

extern crate byteorder;
extern crate crc;
extern crate unix_socket;
extern crate nix;
extern crate libc;
extern crate uuid;
extern crate time;

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
use parser::TextMapOps;

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

fn get_conf() -> Result<LvmTextMap> {
    use std::fs;
    use std::io::Read;

    let mut f = try!(fs::File::open("/etc/lvm/lvm.conf"));

    let mut buf = Vec::new();
    try!(f.read_to_end(&mut buf));

    parser::into_textmap(&buf)
}

fn main() {
    // println!("A");
    // let (name, map) = get_first_vg_meta().unwrap();
    // println!("B {}", name);
    // let vg = parser::vg_from_textmap(&name, &map).expect("didn't get vg!");
    // println!("heyo {} {}", vg.extents(), vg.extent_size);
    // println!("output {:?}", vg);

    // match dm::list_devices() {
    //     Ok(x) => println!("{:?}", x),
    //     Err(x) => println!("error {}", x),
    // }

    let mut vgs = lvmetad::vgs_from_lvmetad().expect("could not get vgs from lvmetad");
    let mut vg = &mut vgs[0];
    for (lvname, lv) in &vg.lvs {
        println!("lv segments {:?}", lv.segments);
    }

    vg.new_linear_lv("grover!!!", 100);

    for (lvname, lv) in &vg.lvs {
        println!("lv2 {:?}", lv);
    }

    let tm = get_conf().expect("could not read lvm.conf");
    let locking_type = tm.textmap_from_textmap("global")
        .and_then(|g| g.i64_from_textmap("locking_type")).unwrap();

    println!("locking_type = {}", locking_type);

    println!("nodename {:?}", nix::sys::utsname::uname().nodename());
}
