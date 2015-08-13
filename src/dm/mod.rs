// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Low-level devicemapper configuration of the running kernel.

#[allow(dead_code, non_camel_case_types)]
mod dm_ioctl;

use std::fs::File;
use std::io::{Result, Error, BufReader, BufRead};
use std::os::unix::io::AsRawFd;
use std::mem;
use std::slice;
use std::slice::bytes::copy_memory;
use std::collections::BTreeSet;

use byteorder::{NativeEndian, ByteOrder};

use nix::sys::ioctl;

use dm::dm_ioctl as dmi;
use lv::LV;
use vg::VG;
use util::align_to;
use ::Device;

const DM_IOCTL: u8 = 0xfd;
const DM_CTL_PATH: &'static str= "/dev/mapper/control";

const DM_VERSION_MAJOR: u32 = 4;
const DM_VERSION_MINOR: u32 = 30;
const DM_VERSION_PATCHLEVEL: u32 = 0;

const DM_NAME_LEN: usize = 128;
const DM_UUID_LEN: usize = 129;

// Status bits
//const DM_READONLY_FLAG: u32 = 1;
const DM_SUSPEND_FLAG: u32 = 2;
//const DM_PERSISTENT_DEV_FLAG: u32 = 8;

/// Major numbers used by DM.
pub fn dev_majors() -> BTreeSet<u32> {
    let mut set = BTreeSet::new();

    let f = File::open("/proc/devices")
        .ok().expect("Could not open /proc/devices");

    let reader = BufReader::new(f);

    for line in reader.lines()
        .filter_map(|x| x.ok())
        .skip_while(|x| x != "Block devices:")
        .skip(1) {
            let spl: Vec<_> = line.split_whitespace().collect();

            if spl[1] == "device-mapper" {
                set.insert(spl[0].parse::<u32>().unwrap());
            }
        }

    set
}

/// Context needed for communicating with devicemapper.
pub struct DM {
    file: File,
}

impl DM {
    /// Create a new context for communicating about a given VG with DM.
    pub fn new() -> Result<Self> {
        Ok(DM {
            file: try!(File::open(DM_CTL_PATH)),
        })
    }

    fn initialize_hdr(hdr: &mut dmi::Struct_dm_ioctl) -> () {
        hdr.version[0] = DM_VERSION_MAJOR;
        hdr.version[1] = DM_VERSION_MINOR;
        hdr.version[2] = DM_VERSION_PATCHLEVEL;

        hdr.data_start = mem::size_of::<dmi::Struct_dm_ioctl>() as u32;
    }

    fn hdr_set_name(hdr: &mut dmi::Struct_dm_ioctl, vg_name: &str, lv_name: &str) -> () {
        let name = format!("{}-{}", vg_name.replace("-", "--"),
                           lv_name.replace("-", "--"));
        let name_dest: &mut [u8; DM_NAME_LEN] = unsafe { mem::transmute(&mut hdr.name) };
        copy_memory(name.as_bytes(), &mut name_dest[..]);
    }

    fn hdr_set_uuid(hdr: &mut dmi::Struct_dm_ioctl, vg_uuid: &str, lv_uuid: &str) -> () {
        let uuid = format!("LVM-{}{}", vg_uuid.replace("-", ""), lv_uuid.replace("-", ""));
        let uuid_dest: &mut [u8; DM_UUID_LEN] = unsafe { mem::transmute(&mut hdr.uuid) };
        copy_memory(uuid.as_bytes(), &mut uuid_dest[..]);
    }

    /// Devicemapper version information: Major, Minor, and patchlevel versions.
    pub fn get_version(&self) -> Result<(u32, u32, u32)> {

        let mut hdr: dmi::Struct_dm_ioctl = Default::default();
        hdr.version[0] = DM_VERSION_MAJOR;
        hdr.version[1] = DM_VERSION_MINOR;
        hdr.version[2] = DM_VERSION_PATCHLEVEL;

        let op = ioctl::op_read_write(DM_IOCTL, dmi::DM_VERSION_CMD as u8,
                                      mem::size_of::<dmi::Struct_dm_ioctl>());

        match unsafe { ioctl::read_into(self.file.as_raw_fd(), op, &mut hdr) } {
            Err(_) => return Err((Error::last_os_error())),
            _ => {},
        };

        Ok((hdr.version[0], hdr.version[1], hdr.version[2]))
    }

    /// Returns a list of tuples containing DM device names within this VG,
    /// and their major/minor device numbers.
    pub fn list_devices(&self, vg: &VG) -> Result<Vec<(String, Device)>> {
        let mut buf = [0u8; 16 * 1024];
        let mut hdr: &mut dmi::Struct_dm_ioctl = unsafe {mem::transmute(&mut buf)};

        Self::initialize_hdr(&mut hdr);
        hdr.data_size = buf.len() as u32;

        let op = ioctl::op_read_write(DM_IOCTL, dmi::DM_LIST_DEVICES_CMD as u8, buf.len());

        match unsafe { ioctl::read_into(self.file.as_raw_fd(), op, &mut buf) } {
            Err(_) => return Err((Error::last_os_error())),
            _ => {},
        };

        let mut devs = Vec::new();
        if (hdr.data_size - hdr.data_start as u32) != 0 {
            let mut result = &buf[hdr.data_start as usize..];

            loop {
                let slc = slice_to_null(&result[12..]).expect("Bad data from ioctl");
                let devno = NativeEndian::read_u64(&result[..8]);
                let dm_name = String::from_utf8_lossy(slc);
                let mut vg_name = vg.name().replace("-", "--");
                vg_name.push('-');

                // Return only devices within this VG
                if dm_name.starts_with(&vg_name) {
                    let lv_name = dm_name.trim_left_matches(&vg_name).replace("--", "-");
                    devs.push((lv_name, devno.into()));
                }

                let next = NativeEndian::read_u32(&result[8..12]);
                if next == 0 { break }

                result = &result[next as usize..];
            }
        }

        Ok(devs)
    }

    /// Query DM for which devices depend on this device.
    pub fn list_deps(&self, dev: Device) -> Result<Vec<Device>> {
        let mut buf = [0u8; 16 * 1024];
        let mut hdr: &mut dmi::Struct_dm_ioctl = unsafe {mem::transmute(&mut buf)};

        Self::initialize_hdr(&mut hdr);
        hdr.data_size = buf.len() as u32;
        hdr.dev = dev.into();

        let op = ioctl::op_read_write(DM_IOCTL, dmi::DM_TABLE_DEPS_CMD as u8, buf.len());

        match unsafe { ioctl::read_into(self.file.as_raw_fd(), op, &mut buf) } {
            Err(_) => return Err((Error::last_os_error())),
            _ => {},
        };

        // TODO: Check DM_BUFFER_FULL_FLAG for:
        // DM_DEVICE_LIST_VERSIONS, DM_DEVICE_LIST, DM_DEVICE_DEPS,
        // DM_DEVICE_STATUS, DM_DEVICE_TABLE, DM_DEVICE_WAITEVENT,
        // DM_DEVICE_TARGET_MSG

        let mut devs = Vec::new();
        if (hdr.data_size - hdr.data_start as u32) != 0 {
            let result = &buf[hdr.data_start as usize..];
            let entries = NativeEndian::read_u32(&result[..4]) as usize;

            for entry in 0..entries {
                let dev = &result[(8*entry)+8..(8*entry)+16];
                devs.push(Device::from(NativeEndian::read_u64(&dev)));
            }
        }

        Ok(devs)
    }

    /// Recursively walk DM deps to see if device is present
    pub fn depends_on(&self, dev: Device, dm_majors: &BTreeSet<u32>) -> bool {
        if !dm_majors.contains(&dev.major) {
            return false;
        }

        if let Ok(dep_list) = self.list_deps(dev) {
            for d in dep_list {
                if d == dev {
                    return true;
                } else if self.depends_on(d, dm_majors) {
                    return true;
                }
            }
        }

        false
    }

    fn create_device(&self, vg: &VG, lv: &mut LV) -> Result<()> {
        let mut hdr: dmi::Struct_dm_ioctl = Default::default();

        Self::initialize_hdr(&mut hdr);
        Self::hdr_set_name(&mut hdr, &vg.name(), &lv.name);
        Self::hdr_set_uuid(&mut hdr, &vg.id(), &lv.id);
        hdr.data_size = hdr.data_start;

        let op = ioctl::op_read_write(DM_IOCTL, dmi::DM_DEV_CREATE_CMD as u8,
                                      mem::size_of::<dmi::Struct_dm_ioctl>());

        match unsafe { ioctl::read_into(self.file.as_raw_fd(), op, &mut hdr) } {
            Err(_) => return Err((Error::last_os_error())),
            _ => { }
        };

        lv.device = Some(Device::from(hdr.dev));

        Ok(())
    }

    fn remove_device(&self, vg: &VG, lv: &LV) -> Result<()> {
        let mut hdr: dmi::Struct_dm_ioctl = Default::default();

        Self::initialize_hdr(&mut hdr);
        hdr.data_size = hdr.data_start;
        Self::hdr_set_name(&mut hdr, &vg.name(), &lv.name);

        let op = ioctl::op_read_write(DM_IOCTL, dmi::DM_DEV_REMOVE_CMD as u8,
                                      mem::size_of::<dmi::Struct_dm_ioctl>());

        match unsafe { ioctl::read_into(self.file.as_raw_fd(), op, &mut hdr) } {
            Err(_) => return Err((Error::last_os_error())),
            _ => Ok(())
        }
    }

    fn load_device(&self, vg: &VG, lv: &LV) -> Result<()> {
        let sectors_per_extent = vg.extent_size();
        let mut targs = Vec::new();

        // Construct targets first, since we need to know how many & size
        // before initializing the header.
        for seg in &lv.segments {
            let mut targ: dmi::Struct_dm_target_spec = Default::default();
            targ.sector_start = seg.start_extent() * sectors_per_extent;
            targ.length = seg.extent_count() * sectors_per_extent;
            targ.status = 0;

            let mut dst: &mut [u8] = unsafe {
                mem::transmute(&mut targ.target_type[..])
            };
            copy_memory(seg.dm_type().as_bytes(), &mut dst);

            let mut params = seg.dm_params(vg);

            let pad_bytes = align_to(
                params.len() + 1usize, 8usize) - params.len();
            params.extend(vec!["\0"; pad_bytes]);

            targ.next = (mem::size_of::<dmi::Struct_dm_target_spec>()
                         + params.len()) as u32;

            targs.push((targ, params));
        }

        let mut hdr: dmi::Struct_dm_ioctl = Default::default();

        Self::initialize_hdr(&mut hdr);
        Self::hdr_set_name(&mut hdr, &vg.name(), &lv.name);

        hdr.data_start = mem::size_of::<dmi::Struct_dm_ioctl>() as u32;
        hdr.data_size = hdr.data_start + targs.iter()
            .map(|&(t, _)| t.next)
            .sum::<u32>();
        hdr.target_count = targs.len() as u32;

        // Flatten into buf
        let mut buf: Vec<u8> = Vec::with_capacity(hdr.data_size as usize);
        unsafe {
            let ptr: *mut u8 = mem::transmute(&mut hdr);
            let slc = slice::from_raw_parts(ptr, hdr.data_start as usize);
            buf.extend(slc);
        }

        for (targ, param) in targs {
            unsafe {
                let ptr: *mut u8 = mem::transmute(&targ);
                let slc = slice::from_raw_parts(
                    ptr, mem::size_of::<dmi::Struct_dm_target_spec>());
                buf.extend(slc);
            }

            buf.extend(param.as_bytes());
        }

        let op = ioctl::op_read_write(DM_IOCTL, dmi::DM_TABLE_LOAD_CMD as u8, buf.len());

        match unsafe { ioctl::read_into_ptr(self.file.as_raw_fd(), op, buf.as_mut_ptr()) } {
            Err(_) => return Err((Error::last_os_error())),
            _ => Ok(())
        }
    }

    fn suspend_device(&self, vg: &VG, lv: &LV) -> Result<()> {
        let mut hdr: dmi::Struct_dm_ioctl = Default::default();

        Self::initialize_hdr(&mut hdr);
        hdr.data_size = hdr.data_start;
        Self::hdr_set_name(&mut hdr, &vg.name(), &lv.name);
        hdr.flags = DM_SUSPEND_FLAG;

        let op = ioctl::op_read_write(DM_IOCTL, dmi::DM_DEV_SUSPEND_CMD as u8,
                                      mem::size_of::<dmi::Struct_dm_ioctl>());

        match unsafe { ioctl::read_into(self.file.as_raw_fd(), op, &mut hdr) } {
            Err(_) => return Err((Error::last_os_error())),
            _ => Ok(())
        }
    }

    fn resume_device(&self, vg: &VG, lv: &LV) -> Result<()> {
        let mut hdr: dmi::Struct_dm_ioctl = Default::default();

        Self::initialize_hdr(&mut hdr);
        hdr.data_size = hdr.data_start;
        Self::hdr_set_name(&mut hdr, &vg.name(), &lv.name);
        // DM_SUSPEND_FLAG not set = resume

        let op = ioctl::op_read_write(DM_IOCTL, dmi::DM_DEV_SUSPEND_CMD as u8,
                                      mem::size_of::<dmi::Struct_dm_ioctl>());

        match unsafe { ioctl::read_into(self.file.as_raw_fd(), op, &mut hdr) } {
            Err(_) => return Err((Error::last_os_error())),
            _ => Ok(())
        }
    }

    /// Activate a Logical Volume.
    ///
    /// Also populates the LV's device field.
    pub fn activate_device(&self, vg: &VG, lv: &mut LV) -> Result<()> {
        try!(self.create_device(vg, lv));
        try!(self.load_device(vg, lv));
        self.resume_device(vg, lv)
    }

    /// Deactivate a Logical Volume.
    pub fn deactivate_device(&self, vg: &VG, lv: &LV) -> Result<()> {
        try!(self.suspend_device(vg, lv));
        self.remove_device(vg, lv)
    }
}

//
// Return up to the first \0, or None
//
fn slice_to_null(slc: &[u8]) -> Option<&[u8]> {
    for (i, c) in slc.iter().enumerate() {
        if *c == b'\0' { return Some(&slc[..i]) };
    }
    None
}
