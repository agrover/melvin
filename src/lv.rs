// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Logical Volumes

use parser::{LvmTextMap, Entry};
use pv::Device;

/// A Logical Volume that is created from a Volume Group.
#[derive(Debug, PartialEq, Clone)]
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
    pub segments: Vec<Segment>,
    /// The major/minor number of the LV.
    pub device: Option<Device>,
}

impl LV {
    /// The total number of extents used by this logical volume.
    pub fn used_extents(&self) -> u64 {
        self.segments
            .iter()
            .map(|x| x.extent_count)
            .sum()
    }
}

impl From<LV> for LvmTextMap {
    fn from(lv: LV) -> LvmTextMap {
        let mut map = LvmTextMap::new();

        map.insert("id".to_string(), Entry::String(lv.id));

        map.insert("status".to_string(),
                   Entry::List(
                       Box::new(
                           lv.status
                               .into_iter()
                               .map(|x| Entry::String(x))
                               .collect())));

        map.insert("flags".to_string(),
                   Entry::List(
                       Box::new(
                           lv.flags
                               .into_iter()
                               .map(|x| Entry::String(x))
                               .collect())));

        map.insert("creation_host".to_string(),
                   Entry::String(lv.creation_host));
        map.insert("creation_time".to_string(),
                   Entry::Number(lv.creation_time as i64));

        map.insert("segment_count".to_string(),
                   Entry::Number(lv.segments.len() as i64));

        for (i, seg) in lv.segments.into_iter().enumerate() {
            map.insert(format!("segment{}", i+1),
                       Entry::TextMap(
                           Box::new(seg.into())));
        }

        map
    }
}

/// A Logical Volume Segment.
#[derive(Debug, PartialEq, Clone)]
pub struct Segment {
    /// A mostly-useless name.
    pub name: String,
    /// The first extent within the LV this segment comprises.
    pub start_extent: u64,
    /// How many extents this segment comprises
    pub extent_count: u64,
    /// The segment type.
    pub ty: String,
    /// If >1, Segment is striped across multiple PVs.
    pub stripes: Vec<(String, u64)>,
}

impl From<Segment> for LvmTextMap {
    fn from(seg: Segment) -> LvmTextMap {
        let mut map = LvmTextMap::new();

        map.insert("start_extent".to_string(),
                   Entry::Number(seg.start_extent as i64));
        map.insert("extent_count".to_string(),
                   Entry::Number(seg.extent_count as i64));
        map.insert("type".to_string(),
                   Entry::String(seg.ty));
        map.insert("stripe_count".to_string(),
                   Entry::Number(seg.stripes.len() as i64));

        map.insert("stripes".to_string(),
                   Entry::List(
                       Box::new(
                           seg.stripes
                               .into_iter()
                               .map(|(k, v)|
                                    vec![
                                        Entry::String(k),
                                        Entry::Number(v as i64)
                                            ]
                                    .into_iter())
                               .flat_map(|x| x)
                               .collect())));
        map
    }
}
