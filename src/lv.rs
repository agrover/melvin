use parser::{LvmTextMap, Entry};

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

impl Into<LvmTextMap> for LV {
    fn into(self) -> LvmTextMap {
        let mut map = LvmTextMap::new();

        map.insert("id".to_string(), Entry::String(self.id));

        map.insert("status".to_string(),
                   Entry::List(
                       Box::new(
                           self.status
                               .into_iter()
                               .map(|x| Entry::String(x))
                               .collect())));

        map.insert("flags".to_string(),
                   Entry::List(
                       Box::new(
                           self.flags
                               .into_iter()
                               .map(|x| Entry::String(x))
                               .collect())));

        map.insert("creation_host".to_string(),
                   Entry::String(self.creation_host));
        map.insert("creation_time".to_string(),
                   Entry::Number(self.creation_time as i64));

        map.insert("segment_count".to_string(),
                   Entry::Number(self.segments.len() as i64));

        for (i, seg) in self.segments.into_iter().enumerate() {
            map.insert(format!("segment{}", i+1),
                       Entry::TextMap(
                           Box::new(seg.into())));
        }

        map
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

impl Into<LvmTextMap> for Segment {
    fn into(self) -> LvmTextMap {
        let mut map = LvmTextMap::new();

        map.insert("start_extent".to_string(),
                   Entry::Number(self.start_extent as i64));
        map.insert("extent_count".to_string(),
                   Entry::Number(self.extent_count as i64));
        map.insert("type".to_string(), Entry::String(self.ty));
        map.insert("stripe_count".to_string(), Entry::Number(self.stripes.len() as i64));

        map.insert("stripes".to_string(),
                   Entry::List(
                       Box::new(
                           self.stripes
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
