// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

#![allow(dead_code)]

extern crate melvin;

use std::io::{self, ErrorKind::Other};
use std::path;
//use std::path::Path;

use melvin::parser;
use melvin::{pvheader_scan, PvHeader};
use melvin::{Error, Result};

fn print_pvheaders() -> Result<()> {
    let dirs = vec![path::Path::new("/dev")];

    for pvheader in pvheader_scan(&dirs)? {
        println!("pvheader {:#?}", pvheader);
        println!("Hdr {:#?}", PvHeader::find_in_dev(&pvheader)?);
    }

    Ok(())
}

fn get_first_vg_meta() -> Result<(String, parser::LvmTextMap)> {
    let dirs = vec![path::Path::new("/dev")];

    for pv_path in pvheader_scan(&dirs)? {
        let pvheader = PvHeader::find_in_dev(&pv_path)?;
        let map = pvheader.read_metadata()?;

        // Find the textmap for the vg, among all the other stuff.
        // (It's the only textmap.)
        for (key, value) in map {
            if let parser::Entry::TextMap(x) = value {
                return Ok((key, *x));
            }
        }
    }

    Err(Error::Io(io::Error::new(Other, "dude")))
}

fn get_conf() -> Result<parser::LvmTextMap> {
    use std::fs;
    use std::io::Read;

    let mut f = fs::File::open("/etc/lvm/lvm.conf")?;

    let mut buf = Vec::new();
    f.read_to_end(&mut buf)?;

    parser::buf_to_textmap(&buf)
}

fn main() -> Result<()> {
    // println!("{:?}", PvHeader::initialize(Path::new("/dev/vdc1")));
    print_pvheaders()?;
    let (name, map) = get_first_vg_meta().unwrap();
    println!("name {} map {:#?}", name, map);
    //    let vg = parser::vg_from_textmap(&name, &map).expect("didn't get vg!");

    // let path1 = Path::new("/dev/vdc1");
    // let path2 = Path::new("/dev/vdc2");

    //    let pvh1 = PvHeader::find_in_dev(Path::new("/dev/vdc1")).expect("pvheader not found");

    // let _vg = VG::create("vg-dopey", vec![path1, path2]).expect("vgcreate failed yo");
    // vg.add_pv(&pvh1).unwrap();
    // vg.add_pv(&pvh2).unwrap();

    // match vg.new_linear_lv("grover125", 2021) {
    //     Ok(_) => {},
    //     Err(x) => {
    //         println!("err {:?}", x);
    //         return;
    //     }
    // };

    // match vg.lv_remove("grover125") {
    //     Ok(_) => {println!("yay")},
    //     Err(x) => {
    //         println!("err {:?}", x);
    //         return;
    //     }
    // };

    // for (lvname, lv) in &vg.lvs {
    //     println!("lv2 {:?}", lv);
    // }

    // let tm = get_conf().expect("could not read lvm.conf");
    // let locking_type = tm.textmap_from_textmap("global")
    //     .and_then(|g| g.i64_from_textmap("locking_type")).unwrap();

    // println!("locking_type = {}", locking_type);

    // let vgtm = vg.into();
    // let s = parser::textmap_to_buf(&vgtm);
    Ok(())
}
