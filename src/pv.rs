#[derive(Debug, PartialEq, Clone)]
pub struct PV {
    pub name: String,
    pub id: String,
    pub status: Vec<String>,
    pub flags: Vec<String>,
    pub dev_size: u64,
    pub pe_start: u64,
    pub pe_count: u64,
}

impl PV {

}
