#![feature(result_expect)]

extern crate melvin;

use std::path;
use std::io::Result;
use std::io::Error;
use std::io::ErrorKind::Other;

use melvin::parser;
use melvin::parser::LvmTextMap;
use melvin::parser::TextMapOps;
use melvin::lvmetad;
use melvin::pvlabel;
use melvin::pvlabel::MDA;
use melvin::dm;


fn get_first_vg_meta() -> Result<(String, LvmTextMap)> {
    let dirs = vec![path::Path::new("/dev")];

    for pv in try!(pvlabel::scan_for_pvs(&dirs)) {
        let mut mda = try!(MDA::new(&pv));
        let map = try!(mda.read_metadata());

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

    parser::buf_to_textmap(&buf)
}

fn main() {
    let (name, map) = get_first_vg_meta().unwrap();
    let vg = parser::vg_from_textmap(&name, &map).expect("didn't get vg!");
    println!("heyo {} {}", vg.extents(), vg.extent_size);
    println!("output {:?}", vg);

    {
        let dm = dm::DM::new(&vg).unwrap();
        match dm.list_devices() {
            Ok(x) => println!("{:?}", x),
            Err(x) => println!("error {}", x),
        }
    }

    let mut vgs = lvmetad::vgs_from_lvmetad().expect("could not get vgs from lvmetad");
    let mut vg = vgs.pop().expect("no vgs in vgs");;

    vg.new_linear_lv("grover125", 100);
    vg.new_linear_lv("grover126", 200);

    for (lvname, lv) in &vg.lvs {
        println!("lv2 {:?}", lv);
    }

    let tm = get_conf().expect("could not read lvm.conf");
    let locking_type = tm.textmap_from_textmap("global")
        .and_then(|g| g.i64_from_textmap("locking_type")).unwrap();

    println!("locking_type = {}", locking_type);

    let vgtm = vg.into();
    let s = parser::textmap_to_buf(&vgtm);
}
