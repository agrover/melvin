// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Logical Volumes

use std::io::Result;
use std::io::Error;
use std::io::ErrorKind::Other;
use std::collections::BTreeMap;

use parser::{
    LvmTextMap,
    TextMapOps,
    Entry,
    status_from_textmap,
};
use Device;
use PV;

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
    pub segments: Vec<Box<segment::Segment>>,
    /// The major/minor number of the LV.
    pub device: Option<Device>,
}

impl LV {
    /// The total number of extents used by this logical volume.
    pub fn used_extents(&self) -> u64 {
        self.segments
            .iter()
            .map(|x| x.extent_count())
            .sum()
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
pub fn from_textmap(name: &str, map: &LvmTextMap, pvs: &BTreeMap<String, PV>) -> Result<LV> {
    let err = || Error::new(Other, "lv textmap parsing error");

    let id = try!(map.string_from_textmap("id").ok_or(err()));
    let creation_host = try!(map.string_from_textmap("creation_host")
                             .ok_or(err()));
    let creation_time = try!(map.i64_from_textmap("creation_time")
                             .ok_or(err()));
    let segment_count = try!(map.i64_from_textmap("segment_count")
                             .ok_or(err()));

    let segments: Vec<_> = (0..segment_count)
        .map(|num| {
            let name = format!("segment{}", num+1);
            let seg_dict = try!(map.textmap_from_textmap(&name).ok_or(err()));
            segment::from_textmap(seg_dict, pvs)
        })
        .filter_map(|seg| seg.ok())
        .collect();


    let status = try!(status_from_textmap(map));

    let flags: Vec<_> = try!(map.list_from_textmap("flags").ok_or(err()))
        .iter()
        .filter_map(|item| match item {
            &Entry::String(ref x) => Some(x.clone()),
            _ => {None},
        })
        .collect();

    Ok(LV {
        name: name.to_string(),
        id: id.to_string(),
        status: status,
        flags: flags,
        creation_host: creation_host.to_string(),
        creation_time: creation_time,
        segments: segments,
        device: None,
    })
}

pub fn to_textmap(lv: &LV, dev_to_idx: &BTreeMap<Device, usize>) -> LvmTextMap {
    let mut map = LvmTextMap::new();

    map.insert("id".to_string(), Entry::String(lv.id.clone()));

    map.insert("status".to_string(),
               Entry::List(
                   Box::new(
                       lv.status
                           .iter()
                           .map(|x| Entry::String(x.clone()))
                           .collect())));

    map.insert("flags".to_string(),
               Entry::List(
                   Box::new(
                       lv.flags
                           .iter()
                           .map(|x| Entry::String(x.clone()))
                           .collect())));

    map.insert("creation_host".to_string(),
               Entry::String(lv.creation_host.clone()));
    map.insert("creation_time".to_string(),
               Entry::Number(lv.creation_time as i64));

    map.insert("segment_count".to_string(),
               Entry::Number(lv.segments.len() as i64));

    for (i, seg) in lv.segments.iter().enumerate() {
        map.insert(format!("segment{}", i+1),
                   Entry::TextMap(
                       Box::new(seg.to_textmap(dev_to_idx))));
    }

    map
}

pub mod segment {
    use std::io::Result;
    use std::io::Error;
    use std::io::ErrorKind::Other;
    use std::collections::BTreeMap;
    use std::fmt;

    use parser::{
        LvmTextMap,
        TextMapOps,
        Entry,
    };
    use PV;
    use VG;
    use Device;

    /// Used to treat segment types polymorphically
    pub trait Segment : fmt::Debug {
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

    pub fn from_textmap(map: &LvmTextMap, pvs: &BTreeMap<String, PV>)
                        -> Result<Box<Segment>> {
        match map.string_from_textmap("type") {
            Some("striped") => StripedSegment::from_textmap(map, pvs),
            Some("thin-pool") => ThinpoolSegment::from_textmap(map, pvs),
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
        pub fn from_textmap(map: &LvmTextMap, pvs: &BTreeMap<String, PV>)
                            -> Result<Box<Segment>> {
            let err = || Error::new(Other, "segment textmap parsing error");

            let stripe_list = try!(map.list_from_textmap("stripes").ok_or(err()));

            let mut stripes: Vec<_> = Vec::new();
            for slc in stripe_list.chunks(2) {
                let name = match &slc[0] {
                    &Entry::String(ref x) => {
                        let pv = try!(pvs.get(x).ok_or(err()));
                        pv.device
                    },
                    _ => return Err(err())
                };
                let val = match slc[1] {
                    Entry::Number(x) => x,
                    _ => return Err(err())
                };
                stripes.push((name, val as u64));
            }

            Ok(Box::new(StripedSegment {
                start_extent: try!(
                    map.i64_from_textmap("start_extent").ok_or(err())) as u64,
                extent_count: try!(
                    map.i64_from_textmap("extent_count").ok_or(err())) as u64,
                stripes: stripes,
                stripe_size: map.i64_from_textmap("start_extent").map(|x| x as u64),
            }))
        }
    }

    impl Segment for StripedSegment {
        fn to_textmap(&self, dev_to_idx: &BTreeMap<Device, usize>)
                          -> LvmTextMap {
            let mut map = LvmTextMap::new();

            map.insert("start_extent".to_string(),
                       Entry::Number(self.start_extent as i64));
            map.insert("extent_count".to_string(),
                       Entry::Number(self.extent_count as i64));
            map.insert("type".to_string(),
                       Entry::String("striped".to_string()));
            map.insert("stripe_count".to_string(),
                       Entry::Number(self.stripes.len() as i64));
            if let Some(stripe_size) = self.stripe_size {
                map.insert("stripe_size".to_string(),
                           Entry::Number(stripe_size as i64));
            }

            map.insert("stripes".to_string(),
                       Entry::List(
                           Box::new(
                               self.stripes
                                   .iter()
                                   .map(|&(k, v)| {
                                       let name = format!(
                                           "pv{}", dev_to_idx.get(&k).unwrap());
                                       vec![
                                           Entry::String(name),
                                           Entry::Number(v as i64)
                                               ]
                                           .into_iter()
                                   })
                                   .flat_map(|x| x)
                                   .collect())));
            map
        }

        fn start_extent(&self) -> u64 {
            self.start_extent
        }

        fn extent_count(&self) -> u64 {
            self.extent_count
        }

        fn pv_dependencies(&self) -> Vec<Device> {
            self.stripes.iter()
                .map(|&(dev, _)| dev)
                .collect()
        }

        // returns (device, start_extent, length)
        fn used_areas(&self) -> Vec<(Device, u64, u64)> {
            self.stripes.iter()
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
                format!("{}:{} {}", dev.major, dev.minor,
                        (start_ext * vg.extent_size()) + pv.pe_start)
            } else {
                let stripes: Vec<_> = self.stripes.iter()
                    .map(|&(dev, start_ext)| {
                        let pv = vg.pv_get(dev).unwrap();
                        format!("{}:{} {}", dev.major, dev.minor,
                                (start_ext * vg.extent_size()) + pv.pe_start)
                            })
                    .collect();

                format!("{} {} {}",
                        self.stripes.len(),
                        self.stripe_size.unwrap(),
                        stripes.join(" "))
            }
        }
    }

    #[derive(Debug, PartialEq)]
    pub enum DiscardPolicy {
        Passdown,
        NoPassdown,
        Ignore,
    }

    /// A Thinpool Logical Volume Segment, tying together data and metadata LVs
    #[derive(Debug, PartialEq)]
    pub struct ThinpoolSegment {
        /// The first extent within the LV this segment comprises.
        pub start_extent: u64,
        /// How many extents this segment comprises
        pub extent_count: u64,
        /// The name of the metadata LV
        pub metadata_lv: String,
        /// The name of the data LV
        pub data_lv: String,
        /// The transaction ID
        pub transaction_id: u64,
        /// The chunk size.
        pub chunk_size: u64,
        /// The discard policy.
        pub discards: DiscardPolicy,
        /// Whether to zero new blocks.
        pub zero_new_blocks: bool,
    }

    impl ThinpoolSegment {
        pub fn from_textmap(map: &LvmTextMap, _pvs: &BTreeMap<String, PV>)
                            -> Result<Box<Segment>> {
            let err = || Error::new(Other, "thinpool segment textmap parsing error");

            let discards = match map.string_from_textmap("discards") {
                Some("passdown") => DiscardPolicy::Passdown,
                Some("nopassdown") => DiscardPolicy::NoPassdown,
                Some("ignore") => DiscardPolicy::Ignore,
                _ => return Err(Error::new(
                    Other, "Invalid text for \"discards\" in thinpool segment")),
            };

            Ok(Box::new(ThinpoolSegment {
                start_extent: try!(
                    map.i64_from_textmap("start_extent").ok_or(err())) as u64,
                extent_count: try!(
                    map.i64_from_textmap("extent_count").ok_or(err())) as u64,
                metadata_lv: try!(
                    map.string_from_textmap("metadata").ok_or(err())).to_string(),
                data_lv: try!(
                    map.string_from_textmap("pool").ok_or(err())).to_string(),
                transaction_id: try!(
                    map.i64_from_textmap("transaction_id").ok_or(err())) as u64,
                chunk_size: try!(
                    map.i64_from_textmap("chunk_size").ok_or(err())) as u64,
                discards: discards,
                zero_new_blocks: try!(
                    map.i64_from_textmap("start_extent").ok_or(err())) != 0,
            }))
        }
    }

    impl Segment for ThinpoolSegment {
        fn to_textmap(&self, _dev_to_idx: &BTreeMap<Device, usize>)
                      -> LvmTextMap {
            let mut map = LvmTextMap::new();

            let discards = match self.discards {
                DiscardPolicy::Passdown => "passdown".to_string(),
                DiscardPolicy::NoPassdown => "nopassdown".to_string(),
                DiscardPolicy::Ignore => "ignore".to_string(),
            };

            map.insert("start_extent".to_string(),
                       Entry::Number(self.start_extent as i64));
            map.insert("extent_count".to_string(),
                       Entry::Number(self.extent_count as i64));
            map.insert("type".to_string(),
                       Entry::String("thin-pool".to_string()));
            map.insert("metadata".to_string(),
                       Entry::String(self.metadata_lv.clone()));
            map.insert("pool".to_string(),
                       Entry::String(self.data_lv.clone()));
            map.insert("transaction_id".to_string(),
                       Entry::Number(self.transaction_id as i64));
            map.insert("chunk_size".to_string(),
                       Entry::Number(self.chunk_size as i64));
            map.insert("discards".to_string(), Entry::String(discards));
            map.insert("zero_new_blocks".to_string(),
                       Entry::Number(self.zero_new_blocks as i64));

            map
        }

        fn start_extent(&self) -> u64 {
            self.start_extent
        }

        fn extent_count(&self) -> u64 {
            self.extent_count
        }

        // None, they're all on subordinate devs
        fn pv_dependencies(&self) -> Vec<Device> {
            Vec::new()
        }

        // None, they're all on subordinate devs
        fn used_areas(&self) -> Vec<(Device, u64, u64)> {
            Vec::new()
        }

        fn dm_type(&self) -> &'static str {
            "thin-pool"
        }

        fn dm_params(&self, vg: &VG) -> String {
            let chunks = (self.extent_count * vg.extent_size()) / self.chunk_size;
            let mut ctor = format!(
                "{} {} {} {}",
                self.metadata_lv,
                self.data_lv,
                self.chunk_size,
                chunks / 5); // 80% low water mark

            if !self.zero_new_blocks {
                ctor.push_str(" skip_block_zeroing");
            }

            match self.discards {
                DiscardPolicy::Passdown => {},
                DiscardPolicy::NoPassdown => ctor.push_str(" no_discard_passdown"),
                DiscardPolicy::Ignore => ctor.push_str( " ignore_discard"),
            };

            ctor
        }
    }
}
