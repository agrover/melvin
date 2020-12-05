// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Logical Volumes

use std::collections::BTreeMap;
use std::io;
use std::io::ErrorKind::Other;

use devicemapper::{
    Device, DmName, LinearDev, LinearDevTargetParams, LinearTargetParams, Sectors, TargetLine, DM,
};

use crate::parser::{status_from_textmap, Entry, LvmTextMap, TextMapOps};
use crate::PV;
use crate::{Error, Result};

/// A Logical Volume that is created from a Volume Group.
#[derive(Debug)]
pub struct LV {
    /// The name.
    pub name: String,
    /// The UUID.
    pub id: String,
    /// The status.
    pub status: Vec<String>,
    /// Flags.
    pub flags: Vec<String>,
    /// Created by this host.
    pub creation_host: String,
    /// Created at this Unix time.
    pub creation_time: i64,
    /// A list of the segments comprising the LV.
    pub segments: Vec<Box<dyn segment::Segment>>,
    /// The major/minor number of the LV.
    pub device: LinearDev,
}

impl LV {
    /// The total number of extents used by this logical volume.
    pub fn used_extents(&self) -> u64 {
        self.segments.iter().map(|x| x.extent_count()).sum()
    }
}

impl PartialEq for LV {
    fn eq(&self, other: &LV) -> bool {
        self.name == other.name
    }
}

pub fn used_areas(lv: &LV) -> Vec<(Device, u64, u64)> {
    let mut v = Vec::new();
    for seg in &lv.segments {
        v.extend(seg.used_areas())
    }
    v
}

/// Construct an LV from an LvmTextMap.
pub fn from_textmap(
    name: &str,
    vg_name: &str,
    map: &LvmTextMap,
    pvs: &BTreeMap<String, PV>,
) -> Result<LV> {
    let err = || Error::Io(io::Error::new(Other, "lv textmap parsing error"));

    let id = map.string_from_textmap("id").ok_or_else(err)?;
    let creation_host = map.string_from_textmap("creation_host").ok_or_else(err)?;
    let creation_time = map.i64_from_textmap("creation_time").ok_or_else(err)?;
    let segment_count = map.i64_from_textmap("segment_count").ok_or_else(err)?;

    let segments: Vec<_> = (0..segment_count)
        .filter_map(|num| {
            let name = format!("segment{}", num + 1);
            map.textmap_from_textmap(&name)
                .map(|seg_dict| segment::from_textmap(seg_dict, pvs))
        })
        .filter_map(|seg| seg.ok())
        .collect();

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

    let dev_name = format!("{}-{}", vg_name.replace("-", "--"), name.replace("-", "--"));

    let dm = DM::new()?;
    let mut logical_start_offset = Sectors(0);

    let mut lines = Vec::new();
    for segment in &segments {
        // TODO: sketchy [0]
        let (dev, off, len) = segment.used_areas()[0];
        // TODO: need to convert from extents to segments???
        lines.push(TargetLine::new(
            logical_start_offset,
            len.into(),
            LinearDevTargetParams::Linear(LinearTargetParams::new(dev, off.into())),
        ));
        logical_start_offset += len.into();
    }
    let linear_dev = LinearDev::setup(&dm, DmName::new(&dev_name)?, None, lines)?;

    Ok(LV {
        name: name.to_string(),
        id: id.to_string(),
        status,
        flags,
        creation_host: creation_host.to_string(),
        creation_time,
        segments,
        device: linear_dev,
    })
}

pub fn to_textmap(lv: &LV, dev_to_idx: &BTreeMap<Device, usize>) -> LvmTextMap {
    let mut map = LvmTextMap::new();

    map.insert("id".to_string(), Entry::String(lv.id.clone()));

    map.insert(
        "status".to_string(),
        Entry::List(lv.status.iter().map(|x| Entry::String(x.clone())).collect()),
    );

    map.insert(
        "flags".to_string(),
        Entry::List(lv.flags.iter().map(|x| Entry::String(x.clone())).collect()),
    );

    map.insert(
        "creation_host".to_string(),
        Entry::String(lv.creation_host.clone()),
    );
    map.insert(
        "creation_time".to_string(),
        Entry::Number(lv.creation_time as i64),
    );

    map.insert(
        "segment_count".to_string(),
        Entry::Number(lv.segments.len() as i64),
    );

    for (i, seg) in lv.segments.iter().enumerate() {
        map.insert(
            format!("segment{}", i + 1),
            Entry::TextMap(Box::new(seg.to_textmap(dev_to_idx))),
        );
    }

    map
}

pub mod segment {
    use std::collections::BTreeMap;
    use std::fmt;
    use std::io::Error;
    use std::io::ErrorKind::Other;
    use std::io::Result;

    use devicemapper::Device;

    use crate::parser::{Entry, LvmTextMap, TextMapOps};
    use crate::PV;
    use crate::VG;

    /// Used to treat segment types polymorphically
    pub trait Segment: fmt::Debug {
        /// Convert this segment to an LvmTextMap.
        fn to_textmap(&self, dev_to_idx: &BTreeMap<Device, usize>) -> LvmTextMap;
        /// Returns the first extent of the segment.
        fn start_extent(&self) -> u64;
        /// Returns how many extents are in the segment.
        fn extent_count(&self) -> u64;
        /// Returns which PVs the segment depends on.
        fn pv_dependencies(&self) -> Vec<Device>;
        /// Returns areas that make up the segment.
        fn used_areas(&self) -> Vec<(Device, u64, u64)>;
        /// Returns the name of the DM target that handles this segment.
        fn dm_type(&self) -> &'static str;
        /// Generates the parameters to send to DM for this segment.
        fn dm_params(&self, vg: &VG) -> String;
    }

    pub fn from_textmap(map: &LvmTextMap, pvs: &BTreeMap<String, PV>) -> Result<Box<dyn Segment>> {
        match map.string_from_textmap("type") {
            Some("striped") => StripedSegment::from_textmap(map, pvs),
            _ => unimplemented!(),
        }
    }

    /// A striped Logical Volume Segment.
    #[derive(Debug, PartialEq)]
    pub struct StripedSegment {
        /// The first extent within the LV this segment comprises.
        pub start_extent: u64,
        /// How many extents this segment comprises
        pub extent_count: u64,
        /// Hoy many 512-byte sectors per stripe
        pub stripe_size: Option<u64>,
        /// Stripes contain the Device and the starting PV extent.
        pub stripes: Vec<(Device, u64)>,
    }

    impl StripedSegment {
        pub fn from_textmap(
            map: &LvmTextMap,
            pvs: &BTreeMap<String, PV>,
        ) -> Result<Box<dyn Segment>> {
            let err = || Error::new(Other, "striped segment textmap parsing error");

            let stripe_list = map.list_from_textmap("stripes").ok_or_else(err)?;

            let mut stripes = Vec::new();
            for slc in stripe_list.chunks(2) {
                let dev = match &slc[0] {
                    Entry::String(ref x) => {
                        let pv = pvs.get(x).ok_or_else(err)?;
                        pv.device
                    }
                    _ => return Err(err()),
                };
                let val = match slc[1] {
                    Entry::Number(x) => x,
                    _ => return Err(err()),
                };
                stripes.push((dev, val as u64));
            }

            Ok(Box::new(StripedSegment {
                start_extent: map.i64_from_textmap("start_extent").ok_or_else(err)? as u64,
                extent_count: map.i64_from_textmap("extent_count").ok_or_else(err)? as u64,
                stripes,
                // optional
                stripe_size: map.i64_from_textmap("start_extent").map(|x| x as u64),
            }))
        }
    }

    impl Segment for StripedSegment {
        fn to_textmap(&self, dev_to_idx: &BTreeMap<Device, usize>) -> LvmTextMap {
            let mut map = LvmTextMap::new();

            map.insert(
                "start_extent".to_string(),
                Entry::Number(self.start_extent as i64),
            );
            map.insert(
                "extent_count".to_string(),
                Entry::Number(self.extent_count as i64),
            );
            map.insert("type".to_string(), Entry::String("striped".to_string()));
            map.insert(
                "stripe_count".to_string(),
                Entry::Number(self.stripes.len() as i64),
            );
            if let Some(stripe_size) = self.stripe_size {
                map.insert("stripe_size".to_string(), Entry::Number(stripe_size as i64));
            }

            map.insert(
                "stripes".to_string(),
                Entry::List(
                    self.stripes
                        .iter()
                        .map(|&(k, v)| {
                            let name = format!("pv{}", dev_to_idx.get(&k).unwrap());
                            vec![Entry::String(name), Entry::Number(v as i64)].into_iter()
                        })
                        .flatten()
                        .collect(),
                ),
            );
            map
        }

        fn start_extent(&self) -> u64 {
            self.start_extent
        }

        fn extent_count(&self) -> u64 {
            self.extent_count
        }

        fn pv_dependencies(&self) -> Vec<Device> {
            self.stripes.iter().map(|&(dev, _)| dev).collect()
        }

        // returns (device, start_extent, length)
        fn used_areas(&self) -> Vec<(Device, u64, u64)> {
            self.stripes
                .iter()
                .map(|&(dev, ext)| (dev, ext, self.extent_count))
                .collect()
        }

        fn dm_type(&self) -> &'static str {
            if self.stripes.len() == 1 {
                "linear"
            } else {
                "striped"
            }
        }

        fn dm_params(&self, vg: &VG) -> String {
            if self.stripes.len() == 1 {
                let (dev, start_ext) = self.stripes[0];
                let pv = vg.pv_get(dev).unwrap();
                format!(
                    "{}:{} {}",
                    dev.major,
                    dev.minor,
                    (start_ext * vg.extent_size()) + pv.pe_start
                )
            } else {
                let stripes: Vec<_> = self
                    .stripes
                    .iter()
                    .map(|&(dev, start_ext)| {
                        let pv = vg.pv_get(dev).unwrap();
                        format!(
                            "{}:{} {}",
                            dev.major,
                            dev.minor,
                            (start_ext * vg.extent_size()) + pv.pe_start
                        )
                    })
                    .collect();

                format!(
                    "{} {} {}",
                    self.stripes.len(),
                    self.stripe_size.unwrap(),
                    stripes.join(" ")
                )
            }
        }
    }
}
