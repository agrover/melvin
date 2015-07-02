use lv::LV;
use pv::PV;
use std::collections::btree_map::BTreeMap;

#[derive(Debug, PartialEq, Clone)]
pub struct VG {
    pub name: String,
    pub id: String,
    pub seqno: u64,
    pub format: String,
    pub status: Vec<String>,
    pub flags: Vec<String>,
    pub extent_size: u64,
    pub max_lv: u64,
    pub max_pv: u64,
    pub metadata_copies: u64,
    pub pvs: BTreeMap<String, PV>,
    pub lvs: BTreeMap<String, LV>,
}

impl VG {
    pub fn extents_in_use(&self) -> u64 {
        self.lvs
            .values()
            .map(|x| x.used_extents())
            .sum()
    }

    pub fn extents_free(&self) -> u64 {
        self.extents() - self.extents_in_use()
    }

    pub fn extents(&self) -> u64 {
        self.pvs
            .values()
            .map(|x| x.pe_count)
            .sum()
    }

    pub fn used_areas(&self) -> BTreeMap<String, BTreeMap<u64, u64>> {
        let mut used_map = BTreeMap::new();

        // pretty sure this is only correct for my system...
        for (lvname, lv) in &self.lvs {
            for seg in &lv.segments {
                for &(ref pvname, start) in &seg.stripes {
                    used_map.entry(pvname.to_string()).or_insert(BTreeMap::new())
                        .insert(start as u64, seg.extent_count);
                }
            }
        }

        used_map
    }

    pub fn free_areas(&self) -> BTreeMap<String, BTreeMap<u64, u64>> {
        let mut free_map = BTreeMap::new();

        for (pvname, area_map) in &mut self.used_areas() {

            // Insert an entry to mark the end of the PV so the fold works correctly
            let pv = self.pvs.get(pvname).expect("area map name refers to nonexistent PV");
            area_map.insert(pv.pe_count, 0);

            area_map.iter()
                .fold(0, |prev_end, (start, len)| {
                    if prev_end < *start {
                        free_map.entry(pvname.clone()).or_insert(BTreeMap::new())
                            .insert(prev_end, start-prev_end);
                    }
                    start + len
                });
        }

        free_map
    }
}
