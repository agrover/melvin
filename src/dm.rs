use std::fs;
use std::fs::File;
use std::io;
use std::os::unix::io::AsRawFd;
use std::mem;
use std::slice;
use std::slice::bytes::copy_memory;
use std::io::Error;
use std::io::ErrorKind::Other;

use byteorder::{LittleEndian, ByteOrder};

use nix::sys::ioctl;

use dm_ioctl as dmi;
use lv::LV;
use vg::VG;
use util::align_to;
use pv;

const DM_IOCTL: u8 = 0xfd;
const DM_CTL_PATH: &'static str= "/dev/mapper/control";

const DM_VERSION_MAJOR: u32 = 4;
const DM_VERSION_MINOR: u32 = 30;
const DM_VERSION_PATCHLEVEL: u32 = 0;

pub struct DM<'a> {
    file: File,
    vg: &'a VG,
}

impl <'a> DM<'a> {
    pub fn new(vg: &'a VG) -> io::Result<Self> {
        Ok(DM {
            file: try!(File::open(DM_CTL_PATH)),
            vg: vg,
        })
    }

    fn get_version(&self) -> io::Result<(u32, u32, u32)> {

        let mut hdr: dmi::Struct_dm_ioctl = Default::default();
        hdr.version[0] = DM_VERSION_MAJOR;
        hdr.version[1] = DM_VERSION_MINOR;
        hdr.version[2] = DM_VERSION_PATCHLEVEL;

        let op = ioctl::op_read_write(DM_IOCTL, dmi::DM_VERSION_CMD as u8,
                                      mem::size_of::<dmi::Struct_dm_ioctl>());

        match unsafe { ioctl::read_into(self.file.as_raw_fd(), op, &mut hdr) } {
            Err(_) => return Err((io::Error::last_os_error())),
            _ => {},
        };

        Ok((hdr.version[0], hdr.version[1], hdr.version[2]))
    }

    fn list_devices(&self) -> io::Result<Vec<(String, u64)>> {

        let mut devs = Vec::new();

        let mut buf = [0u8; 10240];

        let hdr: &mut dmi::Struct_dm_ioctl = unsafe {mem::transmute(&mut buf)};

        let hdr_size = mem::size_of::<dmi::Struct_dm_ioctl>();
        hdr.version[0] = DM_VERSION_MAJOR;
        hdr.version[1] = DM_VERSION_MINOR;
        hdr.version[2] = DM_VERSION_PATCHLEVEL;
        hdr.data_size = buf.len() as u32;
        hdr.data_start = hdr_size as u32;

        let op = ioctl::op_read_write(DM_IOCTL, dmi::DM_LIST_DEVICES_CMD as u8, buf.len());

        match unsafe { ioctl::read_into(self.file.as_raw_fd(), op, &mut buf) } {
            Err(_) => return Err((io::Error::last_os_error())),
            _ => {},
        };

        if (hdr.data_size - hdr_size as u32) != 0 {
            let mut result = &buf[hdr_size..];

            loop {
                let slc = slice_to_null(&result[12..]).expect("Bad data from ioctl");
                let devno = LittleEndian::read_u64(&result[..8]);
                devs.push((String::from_utf8_lossy(slc).into_owned(), devno));

                let next = LittleEndian::read_u32(&result[8..12]);
                if next == 0 { break }

                result = &result[next as usize..];
            }
        }

        Ok(devs)
    }

    fn initialize_hdr(lv: &LV, hdr: &mut dmi::Struct_dm_ioctl) -> () {
        hdr.version[0] = DM_VERSION_MAJOR;
        hdr.version[1] = DM_VERSION_MINOR;
        hdr.version[2] = DM_VERSION_PATCHLEVEL;

        // Transmute [i8; 128] to [u8; 128]
        let name_dest: &mut [u8; 128] = unsafe { mem::transmute(&mut hdr.name) };
        copy_memory(&lv.name.as_bytes(), &mut name_dest[..]);

        let uuid_dest: &mut [u8; 129] = unsafe { mem::transmute(&mut hdr.uuid) };
        copy_memory(&lv.id.as_bytes(), &mut uuid_dest[..]);
    }

    fn create_device(&self, lv: &mut LV) -> io::Result<()> {
        let mut hdr: dmi::Struct_dm_ioctl = Default::default();

        Self::initialize_hdr(lv, &mut hdr);

        let op = ioctl::op_read_write(DM_IOCTL, dmi::DM_DEV_CREATE_CMD as u8,
                                      mem::size_of::<dmi::Struct_dm_ioctl>());

        match unsafe { ioctl::read_into(self.file.as_raw_fd(), op, &mut hdr) } {
            Err(_) => return Err((io::Error::last_os_error())),
            _ => { }
        };

        lv.device = Some(pv::Device::from(hdr.dev as i64));

        Ok(())
    }

    fn load_device(&self, lv: &LV) -> io::Result<()> {
        let sectors_per_extent = self.vg.extent_size;
        let mut tgt = Vec::new();

        for seg in &lv.segments {
            for &(ref pvname, ref loc) in &seg.stripes {
                let err = || Error::new(Other, "dm load_device error");
                let pv = try!(self.vg.pvs.get(pvname).ok_or(err()));

                let mut ts: dmi::Struct_dm_target_spec = Default::default();
                ts.sector_start = seg.start_extent * sectors_per_extent;
                ts.length = seg.extent_count * sectors_per_extent;
                ts.status = 0;

                let mut dst: &mut [u8] = unsafe {
                    mem::transmute(&mut ts.target_type[..])
                };
                copy_memory(b"linear", &mut dst);

                let mut params = Vec::new();
                params.extend(format!("{}:{} {}",
                                      pv.device.major,
                                      pv.device.minor,
                                      (loc * sectors_per_extent) + pv.pe_start).as_bytes());

                let pad_bytes = align_to(
                    params.len() + 1usize, 8usize) - params.len();
                params.extend(vec![0; pad_bytes]);

                ts.next = (mem::size_of::<dmi::Struct_dm_target_spec>()
                           + params.len()) as u32;

                tgt.push((ts, params));
            }
        }

        let targets_len: u32 = tgt
            .iter()
            .map(|&(ts, _)| ts.next)
            .sum();

        let mut hdr: dmi::Struct_dm_ioctl = Default::default();
        let hdr_len = mem::size_of::<dmi::Struct_dm_ioctl>() as u32;

        Self::initialize_hdr(lv, &mut hdr);

        hdr.data_start = hdr_len as u32;
        hdr.data_size = hdr_len + targets_len;
        hdr.target_count = tgt.len() as u32;

        let mut buf: Vec<u8> = Vec::with_capacity(hdr.data_size as usize);
        unsafe {
            let ptr: *mut u8 = mem::transmute(&mut hdr);
            let slc = slice::from_raw_parts(ptr, hdr_len as usize);
            buf.extend(slc);
        }

        for (ts, param) in tgt {
            unsafe {
                let ptr: *mut u8 = mem::transmute(&ts);
                let slc = slice::from_raw_parts(
                    ptr, mem::size_of::<dmi::Struct_dm_target_spec>());
                buf.extend(slc);
            }

            buf.extend(&param);
        }

        println!("FF {} {}", hdr_len, buf.len());

        let op = ioctl::op_read_write(DM_IOCTL, dmi::DM_TABLE_LOAD_CMD as u8, buf.len());

        match unsafe { ioctl::read_into(self.file.as_raw_fd(), op, &mut buf) } {
            Err(_) => return Err((io::Error::last_os_error())),
            _ => Ok(())
        }
    }

    fn resume_device(&self, lv: &LV) -> io::Result<()> {
        let mut hdr: dmi::Struct_dm_ioctl = Default::default();

        Self::initialize_hdr(lv, &mut hdr);

        // TODO: broken, need to pass some flags
        let op = ioctl::op_read_write(DM_IOCTL, dmi::DM_DEV_SUSPEND_CMD as u8,
                                      mem::size_of::<dmi::Struct_dm_ioctl>());

        match unsafe { ioctl::read_into(self.file.as_raw_fd(), op, &mut hdr) } {
            Err(_) => return Err((io::Error::last_os_error())),
            _ => { }
        };

        Ok(())
    }

    pub fn activate_device(&self, lv: &mut LV) -> io::Result<()> {

        // TODO: name/uuid mangle?

        try!(self.get_version());

        try!(self.list_devices());

        try!(self.create_device(lv));

        try!(self.load_device(lv));

        try!(self.resume_device(lv));

        Ok(())
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
