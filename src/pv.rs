// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Physical Volumes

use std::io;
use std::io::Error;
use std::io::ErrorKind::Other;

use parser::{
    LvmTextMap,
    TextMapOps,
    Entry,
    status_from_textmap,
};

pub mod dev {
    use std::str::FromStr;
    use std::io;
    use std::io::Error;
    use std::io::ErrorKind::Other;
    use std::io::{BufReader, BufRead};
    use std::path::{Path, PathBuf};
    use std::fs::{File, PathExt};
    use std::os::unix::fs::MetadataExt;

    use parser::{
        LvmTextMap,
        Entry,
    };

    /// A struct containing the device's major and minor numbers
    ///
    /// Also allows conversion to/from a single 64bit value.
    #[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
    pub struct Device {
        /// Device major number
        pub major: u32,
        /// Device minor number
        pub minor: u8,
    }

    impl Device {
        /// Returns the path in `/dev` that corresponds with the device number
        pub fn path(&self) -> Option<PathBuf> {
            let f = File::open("/proc/partitions")
                .ok().expect("Could not open /proc/partitions");

            let reader = BufReader::new(f);

            for line in reader.lines().skip(2) {
                if let Ok(line) = line {
                    let spl: Vec<_> = line.split_whitespace().collect();

                    if spl[0].parse::<u32>().unwrap() == self.major
                        && spl[1].parse::<u8>().unwrap() == self.minor {
                            return Some(PathBuf::from(format!("/dev/{}", spl[3])));
                        }
                }
            }
            None
        }
    }

    impl FromStr for Device {
        type Err = io::Error;
        fn from_str(s: &str) -> io::Result<Device> {
            match s.parse::<i64>() {
                Ok(x) => Ok(Device::from(x as u64)),
                Err(_) => {
                    match Path::new(s).metadata() {
                        Ok(x) => Ok(Device::from(x.rdev())),
                        Err(x) => Err(x)
                    }
                }
            }
        }
    }

    impl From<u64> for Device {
        fn from(val: u64) -> Device {
            Device { major: (val >> 8) as u32, minor: (val & 0xff) as u8 }
        }
    }

    impl From<Device> for u64 {
        fn from(dev: Device) -> u64 {
            ((dev.major << 8) ^ (dev.minor as u32 & 0xff)) as u64
        }
    }

    /// Device may be a number or a path. Convert either into a Device.
    pub fn from_textmap(map: &LvmTextMap) -> io::Result<Device> {
        match map.get("device") {
            Some(&Entry::String(ref x)) => {
                match Device::from_str(x) {
                    Ok(x) => Ok(x),
                    Err(_) => Err(Error::new(Other, "could not parse string"))
                }
            },
            Some(&Entry::Number(x)) => Ok(Device::from(x as u64)),
            _ => Err(Error::new(Other, "device textmap parsing error")),
        }
    }
}

/// A Physical Volume that is part of a Volume Group.
#[derive(Debug, PartialEq)]
pub struct PV {
    /// Its UUID
    pub id: String,
    /// Device number for the block device the PV is on
    pub device: dev::Device,
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

/// Construct a PV from an LvmTextMap.
pub fn from_textmap(map: &LvmTextMap) -> io::Result<PV> {
    let err = || Error::new(Other, "pv textmap parsing error");

    let id = try!(map.string_from_textmap("id").ok_or(err()));
    let device = try!(dev::from_textmap(map));
    let dev_size = try!(map.i64_from_textmap("dev_size").ok_or(err()));
    let pe_start = try!(map.i64_from_textmap("pe_start").ok_or(err()));
    let pe_count = try!(map.i64_from_textmap("pe_count").ok_or(err()));

    let status = try!(status_from_textmap(map));

    let flags: Vec<_> = try!(map.list_from_textmap("flags").ok_or(err()))
        .iter()
        .filter_map(|item| match item {
            &Entry::String(ref x) => Some(x.clone()),
            _ => {None},
        })
        .collect();

    // If textmap came from lvmetad, it may also include sections
    // for data area (da0) and metadata area (mda0). These are not
    // in the on-disk text metadata, but in the binary PV header.
    // Don't know if we need them, omitting for now.

    Ok(PV {
        id: id.to_string(),
        device: device,
        status: status,
        flags: flags,
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

    map.insert("status".to_string(),
               Entry::List(
                   Box::new(
                       pv.status
                           .iter()
                           .map(|x| Entry::String(x.clone()))
                           .collect())));

    map.insert("flags".to_string(),
               Entry::List(
                   Box::new(
                       pv.flags
                           .iter()
                           .map(|x| Entry::String(x.clone()))
                           .collect())));

    map.insert("dev_size".to_string(), Entry::Number(pv.dev_size as i64));
    map.insert("pe_start".to_string(), Entry::Number(pv.pe_start as i64));
    map.insert("pe_count".to_string(), Entry::Number(pv.pe_count as i64));

    map
}
