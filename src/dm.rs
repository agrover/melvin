use std::fs;
use std::io;
use std::os::unix::io::AsRawFd;
use std::mem;

use byteorder::{LittleEndian, ByteOrder};

use nix::sys::ioctl;

use dm_ioctl as dmi;

const DM_IOCTL: u8 = 0xfd;
const DM_CTL_PATH: &'static str= "/dev/mapper/control";

const DM_VERSION_MAJOR: u32 = 4;
const DM_VERSION_MINOR: u32 = 30;
const DM_VERSION_PATCHLEVEL: u32 = 0;

pub fn get_version() -> io::Result<(u32, u32, u32)> {

    let f = try!(fs::File::open(DM_CTL_PATH));

    let mut hdr: dmi::Struct_dm_ioctl = Default::default();
    hdr.version[0] = DM_VERSION_MAJOR;
    hdr.version[1] = DM_VERSION_MINOR;
    hdr.version[2] = DM_VERSION_PATCHLEVEL;

    let op = ioctl::op_read_write(DM_IOCTL, dmi::DM_VERSION_CMD as u8,
                            mem::size_of::<dmi::Struct_dm_ioctl>());

    match unsafe { ioctl::read_into(f.as_raw_fd(), op, &mut hdr) } {
        Err(_) => return Err((io::Error::last_os_error())),
        _ => {},
    };

    Ok((hdr.version[0], hdr.version[1], hdr.version[2]))
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

pub fn list_devices() -> io::Result<Vec<(String, u64)>> {

    let mut devs = Vec::new();

    let f = try!(fs::File::open(DM_CTL_PATH));

    let mut buf = [0u8; 10240];

    let hdr: &mut dmi::Struct_dm_ioctl = unsafe {mem::transmute(&mut buf)};

    let hdr_size = mem::size_of::<dmi::Struct_dm_ioctl>();
    hdr.version[0] = DM_VERSION_MAJOR;
    hdr.version[1] = DM_VERSION_MINOR;
    hdr.version[2] = DM_VERSION_PATCHLEVEL;
    hdr.data_size = buf.len() as u32;
    hdr.data_start = hdr_size as u32;

    let op = ioctl::op_read_write(DM_IOCTL, dmi::DM_LIST_DEVICES_CMD as u8, buf.len());

    match unsafe { ioctl::read_into(f.as_raw_fd(), op, &mut buf) } {
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
