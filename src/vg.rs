// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Volume Groups

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::io;
use std::io::ErrorKind::Other;
use std::path::Path;
use std::str::FromStr;

use devicemapper::{
    DevId, Device, DmFlags, DmName, DmOptions, LinearDev, LinearDevTargetParams,
    LinearTargetParams, Sectors, TargetLine, DM,
};
use nix::sys::utsname::uname;
use time::now;

use crate::lv;
use crate::lv::segment;
use crate::lv::LV;
use crate::parser::{status_from_textmap, Entry, LvmTextMap, TextMapOps};
use crate::pv;
use crate::pv::PV;
use crate::pvlabel::{PvHeader, SECTOR_SIZE};
use crate::util::{align_to, make_uuid};
use crate::{Error, Result};

const DEFAULT_EXTENT_SIZE: u64 = 8192; // 4MiB

/// A Volume Group allows multiple Physical Volumes to be treated as a
/// storage pool that can then be used to allocate Logical Volumes.
#[derive(Debug, PartialEq)]
pub struct VG {
    /// Name.
    name: String,
    /// Uuid.
    id: String,
    /// The generation of metadata this VG represents.
    seqno: u64,
    /// Always "lvm2".
    format: String,
    /// Status.
    status: Vec<String>,
    /// Flags.
    flags: Vec<String>,
    /// Size of each extent, in 512-byte sectors.
    extent_size: u64,
    /// Maximum number of LVs, 0 means no limit.
    max_lv: u64,
    /// Maximum number of PVs, 0 means no limit.
    max_pv: u64,
    /// How many metadata copies (?)
    metadata_copies: u64,
    /// Physical Volumes within this volume group.
    pvs: BTreeMap<Device, PV>,
    /// Logical Volumes within this volume group.
    lvs: BTreeMap<String, LV>,
}

impl VG {
    /// Create a Volume Group from one or more PVs.
    pub fn create(name: &str, pv_paths: Vec<&Path>) -> Result<VG> {
        if pv_paths.is_empty() {
            return Err(Error::Io(io::Error::new(
                Other,
                "One or more paths to PVs required",
            )));
        }

        let pvhs = {
            let mut v = Vec::new();
            for path in &pv_paths {
                v.push(PvHeader::find_in_dev(path)?);
            }
            v
        };

        let metadata_areas = pvhs.iter().map(|x| x.metadata_areas.len()).sum::<usize>();
        if metadata_areas == 0 {
            return Err(Error::Io(io::Error::new(
                Other,
                "PVs must have at least one metadata area",
            )));
        }

        let mut vg = VG {
            name: name.to_string(),
            id: make_uuid(),
            seqno: 0,
            format: "lvm2".to_string(),
            status: vec![
                "READ".to_string(),
                "WRITE".to_string(),
                "RESIZEABLE".to_string(),
            ],
            flags: Vec::new(),
            extent_size: DEFAULT_EXTENT_SIZE,
            max_lv: 0,
            max_pv: 0,
            metadata_copies: 0,
            pvs: BTreeMap::new(),
            lvs: BTreeMap::new(),
        };

        for path in &pv_paths {
            vg.pv_add(path)?;
        }

        Ok(vg)
    }

    /// Construct a `VG` from its name and an `LvmTextMap`.
    pub fn from_textmap(name: &str, map: &LvmTextMap) -> Result<VG> {
        let err = || Error::Io(io::Error::new(Other, "vg textmap parsing error"));

        let id = map.string_from_textmap("id").ok_or_else(err)?;
        let seqno = map.i64_from_textmap("seqno").ok_or_else(err)?;
        let format = map.string_from_textmap("format").ok_or_else(err)?;
        let extent_size = map.i64_from_textmap("extent_size").ok_or_else(err)?;
        let max_lv = map.i64_from_textmap("max_lv").ok_or_else(err)?;
        let max_pv = map.i64_from_textmap("max_pv").ok_or_else(err)?;
        let metadata_copies = map.i64_from_textmap("metadata_copies").ok_or_else(err)?;

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
        let str_to_pv = map
            .textmap_from_textmap("physical_volumes")
            .ok_or_else(err)
            .and_then(|tm| {
                let mut ret_map = BTreeMap::new();

                for (key, value) in tm {
                    match value {
                        Entry::TextMap(ref pv_dict) => {
                            ret_map.insert(key.to_string(), pv::from_textmap(pv_dict)?);
                        }
                        _ => return Err(Error::Io(io::Error::new(Other, "expected PV textmap"))),
                    };
                }

                Ok(ret_map)
            })?;

        // "logical_volumes" may be absent
        let lvs = match map.textmap_from_textmap("logical_volumes") {
            Some(tm) => {
                let mut ret_map = BTreeMap::new();

                for (key, value) in tm {
                    match value {
                        Entry::TextMap(ref lv_dict) => {
                            ret_map.insert(
                                key.to_string(),
                                lv::from_textmap(key, name, lv_dict, &str_to_pv)?,
                            );
                        }
                        _ => return Err(Error::Io(io::Error::new(Other, "expected LV textmap"))),
                    }
                }

                ret_map
            }
            None => BTreeMap::new(),
        };

        let pvs = str_to_pv
            .into_iter()
            .map(|(_, pv)| (pv.device, pv))
            .collect();

        Ok(VG {
            name: name.to_string(),
            id: id.to_string(),
            seqno: seqno as u64,
            format: format.to_string(),
            status,
            flags,
            extent_size: extent_size as u64,
            max_lv: max_lv as u64,
            max_pv: max_pv as u64,
            metadata_copies: metadata_copies as u64,
            pvs,
            lvs,
        })
    }

    /// Add a non-affiliated PV to this VG.
    pub fn pv_add(&mut self, path: &Path) -> Result<()> {
        let pvh = PvHeader::find_in_dev(path)?;

        // Check pv is not on an LV from the vg:
        // 1) is pv's major a devicemapper major?
        // 2) Walk dm deps (equiv. of LVM2 dev_manager_device_uses_vg)
        let dev = Device::from_str(&path.to_string_lossy())?;
        // let dm_majors = dm::dev_majors();
        // if dm_majors.contains(&dev.major) {
        //     let dm = DM::new()?;
        //     if dm.depends_on(dev, &dm_majors) {
        //         return Err(Error::new(Other, "Dependency loops prohibited"));
        //     }
        // }

        // Check to ensure device is not already in VG as this could happen
        // if PV has no MDAs
        if self.pvs.contains_key(&dev) {
            return Err(Error::Io(io::Error::new(Other, "PV already in VG")));
        }

        // check pv is not already in the VG or another VG
        // Does it have text metadata??
        if let Ok(metadata) = pvh.read_metadata() {
            // Find the textmap for the vg, among all the other stuff.
            // (It's the only textmap.)
            let mut vg_name = Cow::Borrowed("<unknown>");
            for (key, value) in metadata {
                if let Entry::TextMap(_) = value {
                    vg_name = Cow::Owned(key);
                    break;
                }
            }

            return Err(Error::Io(io::Error::new(
                Other,
                format!("PV already in VG {}", vg_name),
            )));
        }

        let da = pvh
            .data_areas
            .get(0)
            .ok_or_else(|| Error::Io(io::Error::new(Other, "Could not find data area in PV")))?;

        // figure out how many extents fit in the PV's data area
        // pe_start aligned to extent size
        let dev_size_sectors = pvh.size / SECTOR_SIZE as u64;
        let pe_start_sectors = align_to(
            (da.offset / SECTOR_SIZE as u64) as usize,
            self.extent_size as usize,
        ) as u64;
        let mda1_size_sectors = match pvh.metadata_areas.get(1) {
            Some(pvarea) => pvarea.size / SECTOR_SIZE as u64,
            None => 0,
        };
        let area_size_sectors = dev_size_sectors - pe_start_sectors - mda1_size_sectors;
        let pe_count = area_size_sectors / self.extent_size;

        self.pvs.insert(
            dev,
            PV {
                id: pvh.uuid.clone(),
                device: dev,
                status: vec!["ALLOCATABLE".to_string()],
                flags: Vec::new(),
                dev_size: dev_size_sectors,
                pe_start: pe_start_sectors,
                pe_count,
            },
        );

        self.commit()
    }

    /// Remove a PV. It must be unused by any LVs.
    pub fn pv_remove(&mut self, pvh: &PvHeader) -> Result<()> {
        let dev = Device::from_str(&pvh.dev_path.to_string_lossy())?;

        for (lvname, lv) in &self.lvs {
            for seg in &lv.segments {
                for seg_dev in seg.pv_dependencies() {
                    if seg_dev == dev {
                        return Err(Error::Io(io::Error::new(
                            Other,
                            format!("PV in use by LV {}", lvname),
                        )));
                    }
                }
            }
        }

        self.pvs
            .remove(&dev)
            .ok_or_else(|| Error::Io(io::Error::new(Other, "Could not remove PV")))?;

        self.commit()
    }

    /// Create a new linear logical volume in the volume group.
    pub fn lv_create_linear(&mut self, name: &str, extent_size: u64) -> Result<()> {
        if self.lvs.contains_key(name) {
            return Err(Error::Io(io::Error::new(Other, "LV already exists")));
        }

        let (dev, area_start, len) = {
            let mut contig_area = None;
            for (dev, areas) in self.free_areas() {
                for (start, len) in areas {
                    if len >= extent_size {
                        contig_area = Some((dev, start, len));
                        break;
                    }
                }
            }

            if let Some(contig) = contig_area {
                contig
            } else {
                return Err(Error::Io(io::Error::new(
                    Other,
                    "no contiguous area for new LV",
                )));
            }
        };

        let segment = Box::new(segment::StripedSegment {
            start_extent: 0,
            extent_count: extent_size,
            stripes: vec![(dev, area_start)],
            stripe_size: None,
        });

        let lv_name = format!(
            "{}-{}",
            self.name.replace("-", "--"),
            name.replace("-", "--")
        );

        let params = LinearTargetParams::new(Device::from(u64::from(dev)), Sectors(area_start));
        let table = vec![TargetLine::new(
            Sectors(0),
            Sectors(len),
            LinearDevTargetParams::Linear(params),
        )];

        // poke dm and tell it about a new device
        let dm = DM::new()?;
        let new_linear = LinearDev::setup(
            &dm,
            DmName::new(&lv_name).expect("valid format"),
            None,
            table,
        )?;

        let lv = LV {
            name: name.to_string(),
            id: make_uuid(),
            status: vec![
                "READ".to_string(),
                "WRITE".to_string(),
                "VISIBLE".to_string(),
            ],
            flags: Vec::new(),
            creation_host: uname().nodename().to_string(),
            creation_time: now().to_timespec().sec,
            segments: vec![segment],
            device: new_linear,
        };

        self.lvs.insert(name.to_string(), lv);

        self.commit()
    }

    /// Destroy a logical volume.
    pub fn lv_remove(&mut self, name: &str) -> Result<()> {
        match self.lvs.remove(name) {
            None => Err(Error::Io(io::Error::new(Other, "LV not found in VG"))),
            Some(lv) => {
                let dm = DM::new()?;
                let name = DmName::new(&lv.name)?;
                dm.device_suspend(
                    &DevId::Name(name),
                    &DmOptions::new().set_flags(DmFlags::DM_SUSPEND),
                )?;
                dm.device_remove(&DevId::Name(name), &DmOptions::new())?;

                self.commit()
            }
        }
    }

    /// The total number of extents in use in the volume group.
    pub fn extents_in_use(&self) -> u64 {
        self.lvs.values().map(|x| x.used_extents()).sum()
    }

    /// The total number of free extents in the volume group.
    pub fn extents_free(&self) -> u64 {
        self.extents() - self.extents_in_use()
    }

    /// The total number of extents in the volume group.
    pub fn extents(&self) -> u64 {
        self.pvs.values().map(|x| x.pe_count).sum()
    }

    fn commit(&mut self) -> Result<()> {
        self.seqno += 1;

        let map: LvmTextMap = to_textmap(self);

        let mut disk_map = LvmTextMap::new();
        disk_map.insert(
            "contents".to_string(),
            Entry::String("Melvin Text Format Volume Group".to_string()),
        );
        disk_map.insert("version".to_string(), Entry::Number(1));
        disk_map.insert("description".to_string(), Entry::String("".to_string()));
        disk_map.insert(
            "creation_host".to_string(),
            Entry::String(uname().nodename().to_string()),
        );
        disk_map.insert(
            "creation_time".to_string(),
            Entry::Number(now().to_timespec().sec),
        );
        disk_map.insert(self.name.clone(), Entry::TextMap(Box::new(map)));

        // TODO: atomicity of updating pvs, metad, dm
        for pv in self.pvs.values() {
            if let Some(path) = pv.path() {
                let mut pvheader = PvHeader::find_in_dev(&path).expect("could not find pvheader");

                pvheader.write_metadata(&disk_map)?;
            }
        }

        Ok(())
    }

    // Returns used areas in the format: {Device: {start: len} }
    //
    // e.g. with {<Device 3:1>: {0: 45, 47: 100, 147: 200} }
    // extents 0-44 (inclusive) are used, 45 and 46 are not, 47-146
    // are used, then 147-346 are used.
    //
    // Adjacent used areas are not merged.
    //
    // PVs with no used areas are not in the outer map at all.
    //
    fn used_areas(&self) -> BTreeMap<Device, BTreeMap<u64, u64>> {
        let mut used_map = BTreeMap::new();

        for lv in self.lvs.values() {
            for (device, start, len) in lv::used_areas(lv) {
                used_map
                    .entry(device)
                    .or_insert_with(BTreeMap::new)
                    .insert(start, len);
            }
        }

        used_map
    }

    // Returns unused areas in the format: {Device: {start: len} }
    //
    // e.g. assuming the same <Device 3:1> as above and it has 1000
    // extents, calling free_areas would result in:
    // {<Device 3:1>: {45: 2, 347: 653} }
    //
    fn free_areas(&self) -> BTreeMap<Device, BTreeMap<u64, u64>> {
        let mut free_map = BTreeMap::new();

        for (dev, mut area_map) in self.used_areas() {
            // Insert an entry to mark the end of the PV so the fold works
            // correctly
            let pv = self
                .pvs
                .get(&dev)
                .expect("area map name refers to nonexistent PV");
            area_map.insert(pv.pe_count, 0);

            area_map.iter().fold(0, |prev_end, (start, len)| {
                if prev_end < *start {
                    free_map
                        .entry(dev)
                        .or_insert_with(BTreeMap::new)
                        .insert(prev_end, start - prev_end);
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

    /// Returns a list of PV Devices that make up the VG.
    pub fn pv_list(&self) -> Vec<Device> {
        self.pvs.keys().copied().collect()
    }

    /// Returns a reference to the PV matching the Device.
    pub fn pv_get(&self, dev: Device) -> Option<&PV> {
        self.pvs.get(&dev)
    }

    /// Returns a list of the names of LVs in the VG.
    pub fn lv_list(&self) -> Vec<String> {
        self.lvs.keys().cloned().collect()
    }

    /// Returns a reference to the LV matching the name.
    pub fn lv_get(&self, name: &str) -> Option<&LV> {
        self.lvs.get(name)
    }

    /// Returns the name of the VG.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the UUID of the VG.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Returns how many 512-byte sectors make up each extent in the VG.
    pub fn extent_size(&self) -> u64 {
        self.extent_size
    }
}

fn to_textmap(vg: &VG) -> LvmTextMap {
    let mut map = LvmTextMap::new();

    map.insert("id".to_string(), Entry::String(vg.id.clone()));
    map.insert("seqno".to_string(), Entry::Number(vg.seqno as i64));
    map.insert("format".to_string(), Entry::String(vg.format.clone()));

    map.insert("max_pv".to_string(), Entry::Number(0));
    map.insert("max_lv".to_string(), Entry::Number(0));

    map.insert(
        "status".to_string(),
        Entry::List(vg.status.iter().map(|x| Entry::String(x.clone())).collect()),
    );

    map.insert(
        "flags".to_string(),
        Entry::List(vg.flags.iter().map(|x| Entry::String(x.clone())).collect()),
    );

    map.insert(
        "extent_size".to_string(),
        Entry::Number(vg.extent_size as i64),
    );
    map.insert(
        "metadata_copies".to_string(),
        Entry::Number(vg.metadata_copies as i64),
    );

    // See comment in from_textmap() - we need to assign ordinals to
    // the PV map so the textmap can use "pv0"-style strings to link
    // pvs with LV stripes.
    let dev_to_idx: BTreeMap<Device, usize> = vg
        .pvs
        .values()
        .enumerate()
        .map(|(num, pv)| (pv.device, num))
        .collect();

    map.insert(
        "physical_volumes".to_string(),
        Entry::TextMap(Box::new(
            vg.pvs
                .iter()
                .map(|(k, v)| {
                    (
                        format!("pv{}", dev_to_idx.get(k).unwrap()),
                        Entry::TextMap(Box::new(pv::to_textmap(v))),
                    )
                })
                .collect(),
        )),
    );

    if !vg.lvs.is_empty() {
        map.insert(
            "logical_volumes".to_string(),
            Entry::TextMap(Box::new(
                vg.lvs
                    .iter()
                    .map(|(k, v)| {
                        (
                            k.clone(),
                            Entry::TextMap(Box::new(lv::to_textmap(v, &dev_to_idx))),
                        )
                    })
                    .collect(),
            )),
        );
    }

    map
}
