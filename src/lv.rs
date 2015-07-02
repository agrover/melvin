use std::io::Result;
use std::io::Error;
use std::io::ErrorKind::Other;
use vg::VG;

#[derive(Debug, PartialEq, Clone)]
pub struct LV {
    pub name: String,
    pub id: String,
    pub status: Vec<String>,
    pub flags: Vec<String>,
    pub creation_host: String,
    pub creation_time: i64,
    pub segments: Vec<Segment>,
}

impl LV {

    pub fn used_extents(&self) -> u64 {
        self.segments
            .iter()
            .map(|x| x.extent_count)
            .sum()
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct Segment {
    pub name: String,
    pub start_extent: u64,
    pub extent_count: u64,
    pub ty: String,
    pub stripes: Vec<(String, u64)>,
}
