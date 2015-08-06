// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Volume Groups

use std::io::Result;
use std::io::Error;
use std::io::ErrorKind::Other;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::str::FromStr;
use std::borrow::Cow;

use time::now;
use nix::sys::utsname::uname;

use lv::{LV, Segment};
use pv::{PV, Device};
use pvlabel::PvHeader;
use parser::{LvmTextMap, Entry};
use lvmetad;
use dm::DM;
use util::{align_to, make_uuid};

/// A Volume Group.
#[derive(Debug, PartialEq, Clone)]
pub struct VG {
    /// Name.
    pub name: String,
    /// Uuid.
    pub id: String,
    /// The generation of metadata this VG represents.
    pub seqno: u64,
    /// Always "LVM2".
    pub format: String,
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
    pub pvs: BTreeMap<String, PV>,
    /// Logical Volumes within this volume group.
    pub lvs: BTreeMap<String, LV>,
}

impl VG {
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

    // Recursively walk DM deps to see if device is present
    fn depends_on(dev: Device, dm_majors: &BTreeSet<u32>, dm: &DM) -> bool {
        if !dm_majors.contains(&dev.major) {
            return false;
        }

        if let Ok(dep_list) = dm.list_deps(dev) {
            for d in dep_list {
                if d == dev {
                    return true;
                } else if Self::depends_on(d, dm_majors, dm) {
                    return true;
                }
            }
        }

        false
    }

    /// Add a non-affiliated PV to this VG.
    pub fn add_pv(&mut self, pvh: &PvHeader) -> Result<()> {
        // Check pv is not on an LV from the vg:
        // 1) is pv's major a devicemapper major?
        // 2) Walk dm deps (equiv. of LVM2 dev_manager_device_uses_vg)
        let dm_majors = DM::dm_majors();
        let dev = try!(Device::from_str(&pvh.dev_path.to_string_lossy()));
        {
            let dm = try!(DM::new(&self));
            if Self::depends_on(dev, &dm_majors, &dm) {
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
        let pe_start = align_to(da.offset as usize, self.extent_size as usize) as u64;
        let mda1_size = match pvh.metadata_areas.get(1) {
            Some(pvarea) => pvarea.size,
            None => 0,
        };
        let area_size = pvh.size - pe_start - mda1_size;
        let pe_count = area_size / self.extent_size;

        let name = format!("pv{}", self.pvs.len());

        // make a PV and add it to self
        let pv = PV {
            name: name.clone(),
            id: pvh.uuid.clone(),
            device: dev,
            status: vec!["ALLOCATABLE".to_string()],
            flags: Vec::new(),
            dev_size: pvh.size,
            pe_start: pe_start,
            pe_count: pe_count,
        };

        self.pvs.insert(name, pv);

        self.commit()
    }

    /// Create a new linear logical volume in the volume group.
    pub fn new_linear_lv(&mut self, name: &str, extent_size: u64) -> Result<()> {
        if self.lvs.contains_key(name) {
            return Err(Error::new(Other, "LV already exists"));
        }

        let mut contig_area = None;
        for (pvname, areas) in self.free_areas() {
            for (start, len) in areas {
                if len >= extent_size {
                    contig_area = Some((pvname, start));
                    break;
                }
            }
        }

        // we don't support multiple segments yet
        let (pv_with_area, area_start) = match contig_area {
            None => return Err(Error::new(Other, "no contiguous area for new LV")),
            Some(x) => x,
        };

        let segment = Segment {
            name: "segment1".to_string(),
            start_extent: 0,
            extent_count: extent_size,
            ty: "striped".to_string(),
            stripes: vec![(pv_with_area, area_start)],
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

    fn commit(&mut self) -> Result<()> {
        let map: LvmTextMap = self.clone().into();

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
    fn used_areas(&self) -> BTreeMap<String, BTreeMap<u64, u64>> {
        let mut used_map = BTreeMap::new();

        for lv in self.lvs.values() {
            for seg in &lv.segments {
                for &(ref pvname, start) in &seg.stripes {
                    used_map.entry(pvname.to_string())
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
    fn free_areas(&self) -> BTreeMap<String, BTreeMap<u64, u64>> {
        let mut free_map = BTreeMap::new();

        for (pvname, mut area_map) in self.used_areas() {

            // Insert an entry to mark the end of the PV so the fold works
            // correctly
            let pv = self.pvs.get(&pvname)
                .expect("area map name refers to nonexistent PV");
            area_map.insert(pv.pe_count, 0);

            area_map.iter()
                .fold(0, |prev_end, (start, len)| {
                    if prev_end < *start {
                        free_map.entry(pvname.clone())
                            .or_insert(BTreeMap::new())
                            .insert(prev_end, start-prev_end);
                    }
                    start + len
                });
        }

        // Also return completely-unused PVs
        for (pvname, pv) in &self.pvs {
            if !free_map.contains_key(pvname) {
                let mut map = BTreeMap::new();
                map.insert(0, pv.pe_count);
                free_map.insert(pvname.clone(), map);
            }
        }

        free_map
    }
}

impl From<VG> for LvmTextMap {
    fn from(vg: VG) -> Self {
        let mut map = LvmTextMap::new();

        map.insert("id".to_string(), Entry::String(vg.id));
        map.insert("seqno".to_string(),
                   Entry::Number(vg.seqno as i64 + 1));
        map.insert("format".to_string(), Entry::String(vg.format));

        map.insert("max_pv".to_string(), Entry::Number(0));
        map.insert("max_lv".to_string(), Entry::Number(0));

        map.insert("status".to_string(),
                   Entry::List(
                       Box::new(
                           vg.status
                               .into_iter()
                               .map(|x| Entry::String(x))
                               .collect())));

        map.insert("flags".to_string(),
                   Entry::List(
                       Box::new(
                           vg.flags
                               .into_iter()
                               .map(|x| Entry::String(x))
                               .collect())));

        map.insert("extent_size".to_string(),
                   Entry::Number(vg.extent_size as i64));
        map.insert("metadata_copies".to_string(),
                   Entry::Number(vg.metadata_copies as i64));
        map.insert("physical_volumes".to_string(),
                   Entry::TextMap(
                       Box::new(
                           vg.pvs
                               .into_iter()
                               .map(|(k, v)|
                                    (k, Entry::TextMap(Box::new(v.into()))))
                               .collect())));

        map.insert("logical_volumes".to_string(),
                   Entry::TextMap(
                       Box::new(
                           vg.lvs
                               .into_iter()
                               .map(|(k, v)|
                                    (k, Entry::TextMap(Box::new(v.into()))))
                               .collect())));

        map
    }
}
