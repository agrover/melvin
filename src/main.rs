#![feature(collections)]

extern crate byteorder;
extern crate crc;
extern crate unix_socket;
extern crate nix;

use std::path;
use std::io::Result;
use std::io::Error;
use std::io::ErrorKind::Other;

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
    flags: Vec<String>,
    dev_size: u64,
    pe_start: u64,
    pe_count: u64,
}

#[derive(Debug, PartialEq, Clone)]
struct Segment {
    name: String,
    start_extent: u64,
    extent_count: u64,
    ty: String,
    stripe_count: u64,
    stripes: Vec<(String, u64)>,
}

#[derive(Debug, PartialEq, Clone)]
struct LV {
    name: String,
    id: String,
    status: Vec<String>,
    flags: Vec<String>,
    creation_host: String,
    creation_time: u64,
    segment_count: u64,
    segments: Vec<Segment>,
}


fn get_first_vg_meta() -> Result<(String, LvmTextMap)> {
    let dirs = vec![path::Path::new("/dev")];

    for pv in try!(pvlabel::scan_for_pvs(&dirs)) {
        let map = try!(pvlabel::textmap_from_dev(pv.as_path()));

        for (key, value) in map {
            match value {
                lexer::Entry::TextMap(x) => return Ok((key, *x)),
                _ => {}
            }
        }
    }

    Err(Error::new(Other, "dude"))
}

fn pvs_from_textmap(map: LvmTextMap) -> Result<Vec<PV>> {
    let err = || Error::new(Other, "dude");

    let mut ret_vec = Vec::new();

    for (key, value) in map {
        let mut pv_dict = match value {
            Entry::TextMap(x) => *x,
            _ => return Err(Error::new(Other, "dude")),
        };

        let id = try!(pv_dict.string_from_textmap("id").ok_or(err()));
        let dev_size = try!(pv_dict.u64_from_textmap("dev_size").ok_or(err()));
        let pe_start = try!(pv_dict.u64_from_textmap("pe_start").ok_or(err()));
        let pe_count = try!(pv_dict.u64_from_textmap("pe_count").ok_or(err()));

        let status: Vec<_> = try!(pv_dict.list_from_textmap("status").ok_or(err()))
            .into_iter()
            .filter_map(|item| match item { Entry::String(x) => Some(x), _ => {None}})
            .collect();

        let flags: Vec<_> = try!(pv_dict.list_from_textmap("flags").ok_or(err()))
            .into_iter()
            .filter_map(|item| match item { Entry::String(x) => Some(x), _ => {None}})
            .collect();

        ret_vec.push(PV {
            name: key,
            id: id,
            status: status,
            flags: flags,
            dev_size: dev_size,
            pe_start: pe_start,
            pe_count: pe_count,
            });
    }

    Ok(ret_vec)
}

fn segments_from_textmap(segment_count: u64, map: &mut LvmTextMap) ->Result<Vec<Segment>> {
    let err = || Error::new(Other, "dude");

    let mut segments = Vec::new();
    for i in 0..segment_count {
        let name = format!("segment{}", i+1);
        let mut seg_dict = try!(map.textmap_from_textmap(&name).ok_or(err()));

        let mut stripes: Vec<_> = Vec::new();
        let mut stripe_list = try!(seg_dict.list_from_textmap("stripes").ok_or(err()));

        while stripe_list.len()/2 != 0 {
            let name = match stripe_list.remove(0) {
                Entry::String(x) => x, _ => return Err(err())
            };
            let val = match stripe_list.remove(0) {
                Entry::Number(x) => x, _ => return Err(err())
            };
            stripes.push((name, val));
        }

        segments.push(Segment{
            name: name,
            start_extent: try!(seg_dict.u64_from_textmap("start_extent").ok_or(err())),
            extent_count: try!(seg_dict.u64_from_textmap("extent_count").ok_or(err())),
            ty: try!(seg_dict.string_from_textmap("type").ok_or(err())),
            stripe_count: try!(seg_dict.u64_from_textmap("stripe_count").ok_or(err())),
            stripes: stripes,
        });
    }

    Ok(segments)
}

fn lvs_from_textmap(map: LvmTextMap) -> Result<Vec<LV>> {
    let err = || Error::new(Other, "dude");

    let mut ret_vec = Vec::new();

    for (key, value) in map {
        let mut lv_dict = match value {
            Entry::TextMap(x) => *x,
            _ => return Err(Error::new(Other, "dude")),
        };

        let id = try!(lv_dict.string_from_textmap("id").ok_or(err()));
        let creation_host = try!(lv_dict.string_from_textmap("creation_host")
                                 .ok_or(err()));
        let creation_time = try!(lv_dict.u64_from_textmap("creation_time")
                                 .ok_or(err()));
        let segment_count = try!(lv_dict.u64_from_textmap("segment_count")
                                 .ok_or(err()));

        let segments = try!(segments_from_textmap(segment_count, &mut lv_dict));

        let status: Vec<_> = try!(lv_dict.list_from_textmap("status").ok_or(err()))
            .into_iter()
            .filter_map(|item| match item { Entry::String(x) => Some(x), _ => {None}})
            .collect();

        let flags: Vec<_> = try!(lv_dict.list_from_textmap("flags").ok_or(err()))
            .into_iter()
            .filter_map(|item| match item { Entry::String(x) => Some(x), _ => {None}})
            .collect();

        ret_vec.push(LV {
            name: key,
            id: id,
            status: status,
            flags: flags,
            creation_host: creation_host,
            creation_time: creation_time,
            segment_count: segment_count,
            segments: segments,
            });
    }

    Ok(ret_vec)
}

fn vg_from_textmap(name: &str, map: &mut LvmTextMap) -> Result<VG> {

    let err = || Error::new(Other, "dude");

    let id = try!(map.string_from_textmap("id").ok_or(err()));
    let seqno = try!(map.u64_from_textmap("seqno").ok_or(err()));
    let format = try!(map.string_from_textmap("format").ok_or(err()));
    let extent_size = try!(map.u64_from_textmap("extent_size").ok_or(err()));
    let max_lv = try!(map.u64_from_textmap("max_lv").ok_or(err()));
    let max_pv = try!(map.u64_from_textmap("max_pv").ok_or(err()));
    let metadata_copies = try!(map.u64_from_textmap("metadata_copies").ok_or(err()));

    let status: Vec<_> = try!(map.list_from_textmap("status").ok_or(err()))
        .into_iter()
        .filter_map(|item| match item { Entry::String(x) => Some(x), _ => {None}})
        .collect();

    let flags: Vec<_> = try!(map.list_from_textmap("flags").ok_or(err()))
        .into_iter()
        .filter_map(|item| match item { Entry::String(x) => Some(x), _ => {None}})
        .collect();

    let pv_meta = try!(map.textmap_from_textmap("physical_volumes").ok_or(err()));
    let pvs = try!(pvs_from_textmap(pv_meta));

    let lv_meta = try!(map.textmap_from_textmap("logical_volumes").ok_or(err()));
    let lvs = try!(lvs_from_textmap(lv_meta));

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

    Ok(vg)
}

fn main() {
    println!("A");
    let (name, mut map) = get_first_vg_meta().unwrap();
    println!("B");
    let vg = vg_from_textmap(&name, &mut map);
    println!("output {:?}", vg);
}
