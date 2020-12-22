// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Physical Volumes

use std::fs::File;
use std::io;
use std::io::ErrorKind::Other;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

use devicemapper::Device;
use nix::sys::stat;

use crate::parser::{status_from_textmap, Entry, LvmTextMap, TextMapOps};
use crate::{Error, Result};

pub fn dev_from_textmap(map: &LvmTextMap) -> Result<Device> {
    let entry = map
        .get("device")
        .ok_or_else(|| Error::Io(io::Error::new(Other, "device textmap parsing error")))?;

    let val = match entry {
        Entry::String(s) => stat::stat(&**s)?.st_rdev as i64,
        &Entry::Number(x) => x,
        _ => {
            return Err(Error::Io(io::Error::new(
                Other,
                "device textmap parsing error",
            )))
        }
    };

    Ok(Device::from(val as u64))
}

/// A Physical Volume that is part of a Volume Group.
#[derive(Debug, PartialEq)]
pub struct PV {
    /// Its UUID
    pub id: String,
    /// Device number for the block device the PV is on
    pub device: Device,
    /// Status
    pub status: Vec<String>,
    /// Flags
    pub flags: Vec<String>,
    /// The device's size, in sectors
    pub dev_size: u64,
    /// The offset in sectors of where the first extent starts
    pub pe_start: u64,
    /// The number of extents in the PV
    pub pe_count: u64,
}

impl PV {
    pub fn path(&self) -> Option<PathBuf> {
        let f = File::open("/proc/partitions").expect("Could not open /proc/partitions");

        let reader = BufReader::new(f);

        for line in reader.lines().skip(2) {
            if let Ok(line) = line {
                let spl: Vec<_> = line.split_whitespace().collect();

                if spl[0].parse::<u32>().unwrap() == self.device.major
                    && spl[1].parse::<u32>().unwrap() == self.device.minor
                {
                    return Some(PathBuf::from(format!("/dev/{}", spl[3])));
                }
            }
        }
        None
    }
}

/// Construct a PV from an LvmTextMap.
pub fn from_textmap(map: &LvmTextMap) -> Result<PV> {
    let err = || Error::Io(io::Error::new(Other, "pv textmap parsing error"));

    let id = map.string_from_textmap("id").ok_or_else(err)?;
    let device = dev_from_textmap(map)?;
    let dev_size = map.i64_from_textmap("dev_size").ok_or_else(err)?;
    let pe_start = map.i64_from_textmap("pe_start").ok_or_else(err)?;
    let pe_count = map.i64_from_textmap("pe_count").ok_or_else(err)?;

    let status = status_from_textmap(map)?;

    let flags: Vec<_> = map
        .list_from_textmap("flags")
        .ok_or_else(err)?
        .iter()
        .filter_map(|item| match item {
            Entry::String(ref x) => Some(x.clone()),
            _ => None,
        })
        .collect();

    Ok(PV {
        id: id.to_string(),
        device,
        status,
        flags,
        dev_size: dev_size as u64,
        pe_start: pe_start as u64,
        pe_count: pe_count as u64,
    })
}

pub fn to_textmap(pv: &PV) -> LvmTextMap {
    let mut map = LvmTextMap::new();

    map.insert("id".to_string(), Entry::String(pv.id.clone()));
    let tmp: u64 = pv.device.into();
    map.insert("device".to_string(), Entry::Number(tmp as i64));

    map.insert(
        "status".to_string(),
        Entry::List(pv.status.iter().map(|x| Entry::String(x.clone())).collect()),
    );

    map.insert(
        "flags".to_string(),
        Entry::List(pv.flags.iter().map(|x| Entry::String(x.clone())).collect()),
    );

    map.insert("dev_size".to_string(), Entry::Number(pv.dev_size as i64));
    map.insert("pe_start".to_string(), Entry::Number(pv.pe_start as i64));
    map.insert("pe_count".to_string(), Entry::Number(pv.pe_count as i64));

    map
}
