use std::str::FromStr;
use std::io::{BufReader, BufRead};
use std::path::{Path, PathBuf};
use std::fs::{File, PathExt};
use std::os::unix::fs::MetadataExt;

use parser::{LvmTextMap, Entry};

#[derive(Debug, PartialEq, Clone)]
pub struct Device {
    pub major: u32,
    pub minor: u8,
}

impl Device {
    pub fn path(&self) -> Option<PathBuf> {
        let f = File::open("/proc/partitions")
            .ok().expect("Could not open /proc/partitions");

        let reader = BufReader::new(f);

        for line in reader.lines().skip(2) {
            if let Ok(line) = line {
                let spl: Vec<_> = line
                    .split(char::is_whitespace)
                    .filter(|x| x.len() != 0)
                    .collect();

                if spl[0].parse::<u32>().unwrap() == self.major
                    && spl[1].parse::<u8>().unwrap() == self.minor {
                        return Some(PathBuf::from(format!("/dev/{}", spl[3])));
                    }
            }
        }
        None
    }
}

pub enum LvmDeviceError {
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

impl From<Device> for i64 {
    fn from(dev: Device) -> i64 {
        ((dev.major << 8) ^ (dev.minor as u32 & 0xff)) as i64
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

impl From<PV> for LvmTextMap {
    fn from(pv: PV) -> LvmTextMap {
        let mut map = LvmTextMap::new();

        map.insert("id".to_string(), Entry::String(pv.id));
        map.insert("device".to_string(), Entry::Number(pv.device.into()));

        map.insert("status".to_string(),
                   Entry::List(
                       Box::new(
                           pv.status
                               .into_iter()
                               .map(|x| Entry::String(x))
                               .collect())));

        map.insert("flags".to_string(),
                   Entry::List(
                       Box::new(
                           pv.flags
                               .into_iter()
                               .map(|x| Entry::String(x))
                               .collect())));

        map.insert("dev_size".to_string(), Entry::Number(pv.dev_size as i64));
        map.insert("pe_start".to_string(), Entry::Number(pv.pe_start as i64));
        map.insert("pe_count".to_string(), Entry::Number(pv.pe_count as i64));

        map
    }
}
