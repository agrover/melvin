// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Logical Volumes

use std::io::Result;
use std::io::Error;
use std::io::ErrorKind::Other;

use parser::{
    LvmTextMap,
    TextMapOps,
    Entry,
    status_from_textmap,
};
use Device;

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
    pub segments: Vec<segment::Segment>,
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

/// Construct an LV from an LvmTextMap.
pub fn from_textmap(name: &str, map: &LvmTextMap) -> Result<LV> {
    let err = || Error::new(Other, "lv textmap parsing error");

    let id = try!(map.string_from_textmap("id").ok_or(err()));
    let creation_host = try!(map.string_from_textmap("creation_host")
                             .ok_or(err()));
    let creation_time = try!(map.i64_from_textmap("creation_time")
                             .ok_or(err()));
    let segment_count = try!(map.i64_from_textmap("segment_count")
                             .ok_or(err()));

    // let segments = try!(segments_from_textmap(segment_count as u64, &map));
    let segments: Vec<_> = (0..segment_count)
        .map(|num| {
            let name = format!("segment{}", num+1);
            let seg_dict = try!(map.textmap_from_textmap(&name).ok_or(err()));
            segment::from_textmap(seg_dict)
        })
        .filter_map(|seg| seg.ok())
        .collect();


    let status = try!(status_from_textmap(map));

    let flags: Vec<_> = try!(map.list_from_textmap("flags").ok_or(err()))
        .into_iter()
        .filter_map(|item| match item { &Entry::String(ref x) => Some(x.clone()), _ => {None}})
        .collect();

    Ok(LV {
        name: name.into(),
        id: id.to_string(),
        status: status,
        flags: flags,
        creation_host: creation_host.to_string(),
        creation_time: creation_time,
        segments: segments,
        device: None,
    })
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

pub mod segment {
    use std::io::Result;
    use std::io::Error;
    use std::io::ErrorKind::Other;

    use parser::{
        LvmTextMap,
        TextMapOps,
        Entry,
    };

    /// A Logical Volume Segment.
    #[derive(Debug, PartialEq, Clone)]
    pub struct Segment {
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

    pub fn from_textmap(map: &LvmTextMap) -> Result<Segment> {
        let err = || Error::new(Other, "segment textmap parsing error");

        let stripe_list = try!(map.list_from_textmap("stripes").ok_or(err()));

        let mut stripes: Vec<_> = Vec::new();
        for slc in stripe_list.chunks(2) {
            let name = match &slc[0] {
                &Entry::String(ref x) => x.clone(),
                _ => return Err(err())
            };
            let val = match slc[1] {
                Entry::Number(x) => x,
                _ => return Err(err())
            };
            stripes.push((name, val as u64));
        }

        Ok(Segment {
            start_extent: try!(
                map.i64_from_textmap("start_extent").ok_or(err())) as u64,
            extent_count: try!(
                map.i64_from_textmap("extent_count").ok_or(err())) as u64,
            ty: try!(
                map.string_from_textmap("type").ok_or(err())).to_string(),
            stripes: stripes,
        })
    }
}
