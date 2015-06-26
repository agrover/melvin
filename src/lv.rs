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
    pub creation_time: u64,
    pub segment_count: u64,
    pub segments: Vec<Segment>,
}

impl LV {

    fn new(vg: &mut VG, name: &str) -> Result<()> {
        if vg.lvs.contains_key(name) {
            return Err(Error::new(Other, "LV already exists"));
        }

        vg.lvs.insert(name.to_string(), LV {
            name: name.to_string(),
            id: "".to_string(),
            status: Vec::new(),
            flags: Vec::new(),
            creation_host: "".to_string(),
            creation_time: 0,
            segment_count: 0,
            segments: Vec::new(),
            });

        Ok(())
    }

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
    pub stripe_count: u64,
    pub stripes: Vec<(String, u64)>,
}
