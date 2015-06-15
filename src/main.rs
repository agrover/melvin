#![feature(collections)]

extern crate byteorder;
extern crate crc;
extern crate unix_socket;
extern crate nix;

use std::path;
use std::io::Result;
use std::io::Error;
use std::io::ErrorKind::Other;
use std::collections::btree_map::BTreeMap;

mod lexer;
mod lvmetad;
mod pvlabel;

use lexer::{Entry, LvmTextMap, MapFromMeta};

#[derive(Debug, PartialEq, Clone)]
struct VG {
    name: String,
    id: String,
    seqno: u64,
    format: String,
    status: Vec<String>,
    flags: Vec<String>,
    extent_size: u64,
    max_lv: u64,
    max_pv: u64,
    metadata_copies: u64,
    pvs: Vec<PV>,
    lvs: Vec<LV>,
}

#[derive(Debug, PartialEq, Clone)]
struct PV {
    name: String,
    id: String,
    status: Vec<String>,
    //flags
    dev_size: u64,
    pe_start: u64,
    pe_count: u64,
}

#[derive(Debug, PartialEq, Clone)]
struct LV {
    name: String,
    id: String,
    status: Vec<String>,
    //flags
    creation_host: String,
    creation_time: u64,
    segment_count: u64,
    //segments
}


fn get_first_vg_meta() -> Result<(String, LvmTextMap)> {
    let dirs = vec![path::Path::new("/dev")];

    for pv in try!(pvlabel::scan_for_pvs(&dirs)) {
        let map = try!(pvlabel::metadata_from_dev(pv.as_path()));

        for (key, value) in map {
            match value {
                lexer::Entry::Dict(x) => return Ok((key, *x)),
                _ => {}
            }
        }

    }

    Err(Error::new(Other, "dude"))
}

fn pvs_from_meta(map: LvmTextMap) -> Result<Vec<PV>> {
    let err = || Error::new(Other, "dude");

    let mut ret_vec = Vec::new();

    for (key, value) in map {
        let mut pv_dict = match value {
            Entry::Dict(x) => *x,
            _ => return Err(Error::new(Other, "dude")),
        };

        let id = try!(pv_dict.string_from_meta("id").ok_or(err()));
        let dev_size = try!(pv_dict.u64_from_meta("dev_size").ok_or(err()));
        let pe_start = try!(pv_dict.u64_from_meta("pe_start").ok_or(err()));
        let pe_count = try!(pv_dict.u64_from_meta("pe_count").ok_or(err()));

        let status_list = try!(pv_dict.list_from_meta("status").ok_or(err()));
        let status: Vec<_> = status_list.into_iter()
            .filter_map(|item| match item { Entry::String(x) => Some(x), _ => {None}})
            .collect();

        ret_vec.push(PV {
            name: key,
            id: id,
            status: status,
            dev_size: dev_size,
            pe_start: pe_start,
            pe_count: pe_count,
            });
    }

    Ok(ret_vec)
}

fn lvs_from_meta(map: LvmTextMap) -> Result<Vec<LV>> {
    let err = || Error::new(Other, "dude");

    let mut ret_vec = Vec::new();

    for (key, value) in map {
        let mut lv_dict = match value {
            Entry::Dict(x) => *x,
            _ => return Err(Error::new(Other, "dude")),
        };

        let id = try!(lv_dict.string_from_meta("id").ok_or(err()));
        let creation_host = try!(lv_dict.string_from_meta("creation_host").ok_or(err()));
        let creation_time = try!(lv_dict.u64_from_meta("creation_time").ok_or(err()));
        let segment_count = try!(lv_dict.u64_from_meta("segment_count").ok_or(err()));

        let status_list = try!(lv_dict.list_from_meta("status").ok_or(err()));
        let status: Vec<_> = status_list.into_iter()
            .filter_map(|item| match item { Entry::String(x) => Some(x), _ => {None}})
            .collect();

        ret_vec.push(LV {
            name: key,
            id: id,
            status: status,
            creation_host: creation_host,
            creation_time: creation_time,
            segment_count: segment_count,
            });
    }

    Ok(ret_vec)
}

fn vg_from_meta(name: &str, map: &mut LvmTextMap) -> Result<VG> {

    let err = || Error::new(Other, "dude");

    let id = try!(map.string_from_meta("id").ok_or(err()));
    let seqno = try!(map.u64_from_meta("seqno").ok_or(err()));
    let format = try!(map.string_from_meta("format").ok_or(err()));
    let extent_size = try!(map.u64_from_meta("extent_size").ok_or(err()));
    let max_lv = try!(map.u64_from_meta("max_lv").ok_or(err()));
    let max_pv = try!(map.u64_from_meta("max_pv").ok_or(err()));
    let metadata_copies = try!(map.u64_from_meta("metadata_copies").ok_or(err()));

    let status_list = try!(map.list_from_meta("status").ok_or(err()));
    let status: Vec<_> = status_list.into_iter()
        .filter_map(|item| match item { Entry::String(x) => Some(x), _ => {None}})
        .collect();

    let flags_list = try!(map.list_from_meta("flags").ok_or(err()));
    let flags: Vec<_> = flags_list.into_iter()
        .filter_map(|item| match item { Entry::String(x) => Some(x), _ => {None}})
        .collect();

    let mut pv_meta = try!(map.dict_from_meta("physical_volumes").ok_or(err()));
    let pvs = try!(pvs_from_meta(pv_meta));

    let mut lv_meta = try!(map.dict_from_meta("logical_volumes").ok_or(err()));
    let lvs = try!(lvs_from_meta(lv_meta));

    let vg = VG {
        name: name.to_string(),
        id: id,
        seqno: seqno,
        format: format,
        status: status,
        flags: flags,
        extent_size: extent_size,
        max_lv: max_lv,
        max_pv: max_pv,
        metadata_copies: metadata_copies,
        pvs: pvs,
        lvs: lvs,
    };

    println!("VG is {:?}", vg);

    Ok(vg)
}

fn main() {
    println!("A");
    let (name, mut map) = get_first_vg_meta().unwrap();
    println!("B");
    let vg = vg_from_meta(&name, &mut map);
    println!("C");
}
