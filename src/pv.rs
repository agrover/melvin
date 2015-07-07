use std::str::FromStr;
use std::num::ParseIntError;
use std::io::Error;
use std::path::Path;
use std::fs::PathExt;
use std::os::unix::fs::MetadataExt;

use parser::{LvmTextMap, TextMapOps, Entry};

#[derive(Debug, PartialEq, Clone)]
pub struct Device {
    pub major: u32,
    pub minor: u8,
}

pub enum LvmDeviceError {
    ParseIntError,
    IoError,
}

impl FromStr for Device {
    type Err = LvmDeviceError;
    fn from_str(s: &str) -> Result<Device, LvmDeviceError> {
        let val = match s.parse::<i64>() {
            Ok(x) => x,
            Err(_) => {
                let path = Path::new(s);
                match path.metadata() {
                    Ok(x) => x.dev() as i64,
                    Err(_) => return Err(LvmDeviceError::IoError)
                }
            }
        };
        Ok(Device::from(val))
    }
}

impl From<i64> for Device {
    fn from(val: i64) -> Device {
        Device { major: (val >> 8) as u32, minor: (val & 0xff) as u8 }
    }
}

impl Into<i64> for Device {
    fn into(self) -> i64 {
        ((self.major << 8) ^ (self.minor as u32 & 0xff)) as i64
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct PV {
    pub name: String,
    pub id: String,
    pub device: Device,
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
        map.insert("device".to_string(), Entry::Number(self.device.into()));

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
