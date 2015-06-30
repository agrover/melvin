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
}
