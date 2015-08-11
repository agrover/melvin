// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Volume Groups

use std::io::Result;
use std::io::Error;
use std::io::ErrorKind::Other;
use std::collections::BTreeMap;
use std::str::FromStr;
use std::borrow::Cow;

use time::now;
use nix::sys::utsname::uname;

use lv;
use lv::LV;
use lv::segment::Segment;
use pv;
use pv::PV;
use Device;
use pvlabel::{PvHeader, SECTOR_SIZE};
use parser::{
    LvmTextMap,
    TextMapOps,
    Entry,
    status_from_textmap,
};
use lvmetad;
use dm;
use dm::DM;
use util::{align_to, make_uuid};

const DEFAULT_EXTENT_SIZE: u64 = 8192;  // 4MiB

/// A Volume Group allows multiple Physical Volumes to be treated as a
/// storage pool that can then be used to allocate Logical Volumes.
#[derive(Debug, PartialEq, Clone)]
pub struct VG {
    /// Name.
    pub name: String,
    /// Uuid.
    pub id: String,
    /// The generation of metadata this VG represents.
    pub seqno: u64,
    /// Always "lvm2".
    format: String,
    /// Status.
    pub status: Vec<String>,
    /// Flags.
    pub flags: Vec<String>,
    /// Size of each extent, in 512-byte sectors.
    pub extent_size: u64,
    /// Maximum number of LVs, 0 means no limit.
    pub max_lv: u64,
    /// Maximum number of PVs, 0 means no limit.
    pub max_pv: u64,
    /// How many metadata copies (?)
    pub metadata_copies: u64,
    /// Physical Volumes within this volume group.
    pub pvs: BTreeMap<Device, PV>,
    /// Logical Volumes within this volume group.
    pub lvs: BTreeMap<String, LV>,
}

impl VG {
    /// Create a Volume Group from one or more initialized PvHeaders.
    pub fn create(name: &str, pvhs: Vec<PvHeader>) -> Result<VG> {
        if pvhs.len() == 0 {
            return Err(Error::new(Other, "One or more PvHeaders required"));
        }

        let metadata_areas = pvhs.iter()
            .map(|x| x.metadata_areas.len())
            .sum::<usize>();
        if metadata_areas == 0 {
            return Err(Error::new(Other, "PVs must have at least one metadata area"));
        }

        let mut vg = VG {
            name: name.to_string(),
            id: make_uuid(),
            seqno: 0,
            format: "lvm2".to_string(),
            status: vec!["READ".to_string(),
                         "WRITE".to_string(),
                         "RESIZEABLE".to_string()],
            flags: Vec::new(),
            extent_size: DEFAULT_EXTENT_SIZE,
            max_lv: 0,
            max_pv: 0,
            metadata_copies: 0,
            pvs: BTreeMap::new(),
            lvs: BTreeMap::new(),
        };

        for pv in &pvhs {
            try!(vg.add_pv(pv));
        }

        Ok(vg)
    }

    /// Add a non-affiliated PV to this VG.
    pub fn add_pv(&mut self, pvh: &PvHeader) -> Result<()> {
        // Check pv is not on an LV from the vg:
        // 1) is pv's major a devicemapper major?
        // 2) Walk dm deps (equiv. of LVM2 dev_manager_device_uses_vg)
        let dm_majors = dm::dev_majors();
        let dev = try!(Device::from_str(&pvh.dev_path.to_string_lossy()));
        if dm_majors.contains(&dev.major) {
            let dm = try!(DM::new(&self));
            if dm.depends_on(dev, &dm_majors) {
                return Err(Error::new(Other, "Dependency loops prohibited"));
            }
        }

        // check pv is not already in the VG or another VG
        // Does it have text metadata??
        if let Ok(metadata) = pvh.read_metadata() {
            // Find the textmap for the vg, among all the other stuff.
            // (It's the only textmap.)
            let mut vg_name = Cow::Borrowed("<unknown>");
            for (key, value) in metadata {
                match value {
                    Entry::TextMap(_) => {
                        vg_name = Cow::Owned(key);
                        break
                    },
                    _ => {}
                }
            }

            return Err(Error::new(Other, format!("PV already in VG {}", vg_name)));
        }

        let da = try!(pvh.data_areas.get(0)
                      .ok_or(Error::new(Other, "Could not find data area in PV")));

        // figure out how many extents fit in the PV's data area
        // pe_start aligned to extent size
        let dev_size_sectors = pvh.size / SECTOR_SIZE as u64;
        let pe_start_sectors = align_to(
            (da.offset / SECTOR_SIZE as u64) as usize,
            self.extent_size as usize) as u64;
        let mda1_size_sectors = match pvh.metadata_areas.get(1) {
            Some(pvarea) => pvarea.size / SECTOR_SIZE as u64,
            None => 0,
        };
        let area_size_sectors = dev_size_sectors - pe_start_sectors - mda1_size_sectors;
        let pe_count = area_size_sectors / self.extent_size;

        // if added PV had no MDAs then we could get this far and then fail
        if self.pvs.contains_key(&dev) {
            Err(Error::new(Other, "PV already in VG"))
        } else {
            self.pvs.insert(dev, PV {
                id: pvh.uuid.clone(),
                device: dev,
                status: vec!["ALLOCATABLE".to_string()],
                flags: Vec::new(),
                dev_size: dev_size_sectors,
                pe_start: pe_start_sectors,
                pe_count: pe_count,
            });

            self.commit()
        }
    }

    /// Remove a PV. It must be unused by any LVs.
    pub fn remove_pv(&mut self, pvh: &PvHeader) -> Result<()> {
        let dev = try!(Device::from_str(&pvh.dev_path.to_string_lossy()));

        for (lvname, lv) in &self.lvs {
            for seg in &lv.segments {
                for &(seg_dev, _) in &seg.stripes {
                    if seg_dev == dev {
                        return Err(Error::new(
                            Other, format!("PV in use by LV {}", lvname)));
                    }
                }
            }
        }

        try!(self.pvs.remove(&dev)
             .ok_or(Error::new(Other, "Could not remove PV")));

        self.commit()
    }

    /// Create a new linear logical volume in the volume group.
    pub fn new_linear_lv(&mut self, name: &str, extent_size: u64) -> Result<()> {
        if self.lvs.contains_key(name) {
            return Err(Error::new(Other, "LV already exists"));
        }

        let mut contig_area = None;
        for (dev, areas) in self.free_areas() {
            for (start, len) in areas {
                if len >= extent_size {
                    contig_area = Some((dev, start));
                    break;
                }
            }
        }

        // we don't support multiple segments yet
        let (dev, area_start) = match contig_area {
            None => return Err(Error::new(Other, "no contiguous area for new LV")),
            Some(x) => x,
        };

        let segment = Segment {
            start_extent: 0,
            extent_count: extent_size,
            ty: "striped".to_string(),
            stripes: vec![(dev, area_start)],
        };

        let mut lv = LV {
            name: name.to_string(),
            id: make_uuid(),
            status: vec!["READ".to_string(),
                         "WRITE".to_string(),
                         "VISIBLE".to_string()],
            flags: Vec::new(),
            creation_host: uname().nodename().to_string(),
            creation_time: now().to_timespec().sec,
            segments: vec![segment],
            device: None,
        };

        // poke dm and tell it about a new device
        {
            let dm = try!(DM::new(self));
            try!(dm.activate_device(&mut lv));
        }

        self.lvs.insert(name.to_string(), lv);

        self.commit()
    }

    /// Destroy a logical volume.
    pub fn lv_remove(&mut self, name: &str) -> Result<()> {
        match self.lvs.remove(name) {
            None => Err(Error::new(Other, "LV not found in VG")),
            Some(mut lv) => {
                {
                    let dm = try!(DM::new(self));
                    try!(dm.deactivate_device(&mut lv));
                }

                self.commit()
            },
        }
    }

    /// The total number of extents in use in the volume group.
    pub fn extents_in_use(&self) -> u64 {
        self.lvs
            .values()
            .map(|x| x.used_extents())
            .sum()
    }

    /// The total number of free extents in the volume group.
    pub fn extents_free(&self) -> u64 {
        self.extents() - self.extents_in_use()
    }

    /// The total number of extents in the volume group.
    pub fn extents(&self) -> u64 {
        self.pvs
            .values()
            .map(|x| x.pe_count)
            .sum()
    }

    fn commit(&mut self) -> Result<()> {
        self.seqno += 1;

        let map: LvmTextMap = to_textmap(self);

        let mut disk_map = LvmTextMap::new();
        disk_map.insert("contents".to_string(),
                        Entry::String("Text Format Volume Group".to_string()));
        disk_map.insert("version".to_string(), Entry::Number(1));
        disk_map.insert("description".to_string(), Entry::String("".to_string()));
        disk_map.insert("creation_host".to_string(),
                        Entry::String(uname().nodename().to_string()));
        disk_map.insert("creation_time".to_string(),
                        Entry::Number(now().to_timespec().sec));
        disk_map.insert(self.name.clone(), Entry::TextMap(Box::new(map.clone())));

        // TODO: atomicity of updating pvs, metad, dm
        for pv in self.pvs.values() {
            if let Some(path) = pv.device.path() {
                let mut pvheader = PvHeader::find_in_dev(&path)
                    .expect("could not find pvheader");

                try!(pvheader.write_metadata(&disk_map));
            }
        }

        lvmetad::vg_update(&self.name, &map)
    }

    // Returns e.g. {"pv0": {0: 45, 47: 100, 100: 200} }
    // This means extents 0-44 are used, 45 and 46 are not,
    // 47-99 are used, then 100-199 are used.
    //
    // Adjacent used areas are not merged.
    //
    // PVs with no used areas are not in the outer map at all.
    //
    fn used_areas(&self) -> BTreeMap<Device, BTreeMap<u64, u64>> {
        let mut used_map = BTreeMap::new();

        for lv in self.lvs.values() {
            for seg in &lv.segments {
                for &(device, start) in &seg.stripes {
                    used_map.entry(device)
                        .or_insert(BTreeMap::new())
                        .insert(start as u64, seg.extent_count);
                }
            }
        }

        used_map
    }

    // Returns e.g. {"pv0": {45: 47, 200: 1000} }
    // (assuming pv0 has 1000 extents)
    //
    fn free_areas(&self) -> BTreeMap<Device, BTreeMap<u64, u64>> {
        let mut free_map = BTreeMap::new();

        for (dev, mut area_map) in self.used_areas() {

            // Insert an entry to mark the end of the PV so the fold works
            // correctly
            let pv = self.pvs.get(&dev)
                .expect("area map name refers to nonexistent PV");
            area_map.insert(pv.pe_count, 0);

            area_map.iter()
                .fold(0, |prev_end, (start, len)| {
                    if prev_end < *start {
                        free_map.entry(dev)
                            .or_insert(BTreeMap::new())
                            .insert(prev_end, start-prev_end);
                    }
                    start + len
                });
        }

        // Also return completely-unused PVs
        for (dev, pv) in &self.pvs {
            if !free_map.contains_key(dev) {
                let mut map = BTreeMap::new();
                map.insert(0, pv.pe_count);
                free_map.insert(*dev, map);
            }
        }

        free_map
    }
}

fn to_textmap(vg: &VG) -> LvmTextMap {
    let mut map = LvmTextMap::new();

    map.insert("id".to_string(), Entry::String(vg.id.clone()));
    map.insert("seqno".to_string(),
               Entry::Number(vg.seqno as i64));
    map.insert("format".to_string(), Entry::String(vg.format.clone()));

    map.insert("max_pv".to_string(), Entry::Number(0));
    map.insert("max_lv".to_string(), Entry::Number(0));

    map.insert("status".to_string(),
               Entry::List(
                   Box::new(
                       vg.status
                           .iter()
                           .map(|x| Entry::String(x.clone()))
                           .collect())));

    map.insert("flags".to_string(),
               Entry::List(
                   Box::new(
                       vg.flags
                           .iter()
                           .map(|x| Entry::String(x.clone()))
                           .collect())));

    map.insert("extent_size".to_string(),
               Entry::Number(vg.extent_size as i64));
    map.insert("metadata_copies".to_string(),
               Entry::Number(vg.metadata_copies as i64));

    // See comment in from_textmap() - we need to assign ordinals to
    // the PV map so the textmap can use "pv0"-style strings to link
    // pvs with LV stripes.
    let dev_to_idx: BTreeMap<Device, usize> = vg.pvs.values()
        .enumerate()
        .map(|(num, pv)| {
            (pv.device, num)
        })
        .collect();

    map.insert("physical_volumes".to_string(),
               Entry::TextMap(
                   Box::new(
                       vg.pvs
                           .iter()
                           .map(|(k, v)|
                                (format!("pv{}", dev_to_idx.get(k).unwrap()),
                                 Entry::TextMap(Box::new(
                                     pv::to_textmap(v)))))
                           .collect())));

    if !vg.lvs.is_empty() {
        map.insert(
            "logical_volumes".to_string(),
            Entry::TextMap(
                Box::new(
                    vg.lvs
                        .iter()
                        .map(|(k, v)|
                             (k.clone(),
                              Entry::TextMap(Box::new(
                                  lv::to_textmap(v, &dev_to_idx)))))
                        .collect())));
    }

    map
}

/// Construct a `VG` from its name and an `LvmTextMap`.
pub fn from_textmap(name: &str, map: &LvmTextMap) -> Result<VG> {

    let err = || Error::new(Other, "vg textmap parsing error");

    let id = try!(map.string_from_textmap("id").ok_or(err()));
    let seqno = try!(map.i64_from_textmap("seqno").ok_or(err()));
    let format = try!(map.string_from_textmap("format").ok_or(err()));
    let extent_size = try!(map.i64_from_textmap("extent_size").ok_or(err()));
    let max_lv = try!(map.i64_from_textmap("max_lv").ok_or(err()));
    let max_pv = try!(map.i64_from_textmap("max_pv").ok_or(err()));
    let metadata_copies = try!(map.i64_from_textmap("metadata_copies").ok_or(err()));

    let status = try!(status_from_textmap(map));

    let flags: Vec<_> = try!(map.list_from_textmap("flags").ok_or(err()))
        .iter()
        .filter_map(|item| match item {
            &Entry::String(ref x) => Some(x.clone()),
            _ => {None},
        })
        .collect();


    // While the textmap uses "pv0"-style names to link physical
    // volume definitions with LV segment stripes, we do not want to
    // use these internally, because what if "pv0" is unused and is
    // removed from the VG? When writing out metadata, the remaining
    // PV should then be labeled "pv0".
    //
    // Instead, we index PVs by Device, but only after letting
    // segment::from_textmap() (via lv::from_textmap) use the
    // str_to_pv map to translate its "pv0" references to Devices as
    // well.
    //
    let str_to_pv = try!(
        map.textmap_from_textmap("physical_volumes").ok_or(err())
            .and_then(|tm| {
                let mut ret_map = BTreeMap::new();

                for (key, value) in tm {
                    match value {
                        &Entry::TextMap(ref pv_dict) => {
                            ret_map.insert(
                                key.to_string(),
                                try!(pv::from_textmap(pv_dict)));
                        },
                        _ => return Err(
                            Error::new(Other, "expected PV textmap")),
                    };
                }

                Ok(ret_map)
            }));

    // "logical_volumes" may be absent
    let lvs = match map.textmap_from_textmap("logical_volumes") {
        Some(tm) => {
            let mut ret_map = BTreeMap::new();

            for (key, value) in tm {
                match value {
                    &Entry::TextMap(ref lv_dict) => {
                        ret_map.insert(
                            key.to_string(),
                            try!(lv::from_textmap(key, lv_dict, &str_to_pv)));
                    },
                    _ => return Err(
                        Error::new(Other, "expected LV textmap")),
                }
            }

            ret_map
        },
        None => BTreeMap::new(),
    };

    let pvs = str_to_pv.into_iter()
        .map(|(_, pv)| (pv.device, pv))
        .collect();

    let mut vg = VG {
        name: name.to_string(),
        id: id.to_string(),
        seqno: seqno as u64,
        format: format.to_string(),
        status: status,
        flags: flags,
        extent_size: extent_size as u64,
        max_lv: max_lv as u64,
        max_pv: max_pv as u64,
        metadata_copies: metadata_copies as u64,
        pvs: pvs,
        lvs: lvs,
    };

    let dm_devices = {
        let dm = try!(DM::new(&vg));
        try!(dm.list_devices())
    };

    for (lvname, dev) in dm_devices {
        if let Some(lv) = vg.lvs.get_mut(&lvname) {
            lv.device = Some(dev.into());
        }
    }

    Ok(vg)
}
