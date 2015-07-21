//! Physical Volumes

use std::str::FromStr;
use std::io::{BufReader, BufRead};
use std::path::{Path, PathBuf};
use std::fs::{File, PathExt};
use std::os::unix::fs::MetadataExt;

use parser::{LvmTextMap, Entry};

/// A struct containing the device's major and minor numbers
///
/// Also allows conversion to/from a single 64bit value.
#[derive(Debug, PartialEq, Clone)]
pub struct Device {
    /// Device major number
    pub major: u32,
    /// Device minor number
    pub minor: u8,
}

impl Device {
    /// Returns the path in `/dev` that corresponds with the device number
    pub fn path(&self) -> Option<PathBuf> {
        let f = File::open("/proc/partitions")
            .ok().expect("Could not open /proc/partitions");

        let reader = BufReader::new(f);

        for line in reader.lines().skip(2) {
            if let Ok(line) = line {
                let spl: Vec<_> = line.split_whitespace().collect();

                if spl[0].parse::<u32>().unwrap() == self.major
                    && spl[1].parse::<u8>().unwrap() == self.minor {
                        return Some(PathBuf::from(format!("/dev/{}", spl[3])));
                    }
            }
        }
        None
    }
}

/// Errors that can occur when converting from a String into a Device
pub enum LvmDeviceError {
    /// IO Error
    IoError,
}

impl FromStr for Device {
    type Err = LvmDeviceError;
    fn from_str(s: &str) -> Result<Device, LvmDeviceError> {
        match s.parse::<i64>() {
            Ok(x) => Ok(Device::from(x)),
            Err(_) => {
                match Path::new(s).metadata() {
                    Ok(x) => Ok(Device::from(x.dev() as i64)),
                    Err(_) => Err(LvmDeviceError::IoError)
                }
            }
        }
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

/// A Physical Volume.
#[derive(Debug, PartialEq, Clone)]
pub struct PV {
    /// The mostly-useless name
    pub name: String,
    /// Its UUID
    pub id: String,
    /// Device number for the block device the PV is on
    pub device: Device,
    /// Status
    pub status: Vec<String>,
    /// Flags
    pub flags: Vec<String>,
    /// The device's size, in bytes
    pub dev_size: u64,
    /// The offset in sectors of where the first extent starts
    pub pe_start: u64,
    /// The number of extents in the PV
    pub pe_count: u64,
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
