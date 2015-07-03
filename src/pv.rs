use parser::{LvmTextMap, TextMapOps, Entry};

#[derive(Debug, PartialEq, Clone)]
pub struct PV {
    pub name: String,
    pub id: String,
    pub device: String,
    pub status: Vec<String>,
    pub flags: Vec<String>,
    pub dev_size: u64,
    pub pe_start: u64, // in sectors
    pub pe_count: u64, // in extents
}

impl PV {
    pub fn to_textmap(self) -> LvmTextMap {
        let mut map = LvmTextMap::new();

        map.insert("id".to_string(), Entry::String(self.id));
        map.insert("device".to_string(), Entry::String(self.device));

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

        map.insert("dev_size".to_string(), Entry::Number(self.dev_size as i64));
        map.insert("pe_start".to_string(), Entry::Number(self.pe_start as i64));
        map.insert("pe_count".to_string(), Entry::Number(self.pe_count as i64));

        map
    }
}
