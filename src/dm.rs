use std::fs;
use std::io::Result;
use std::os::unix::io::AsRawFd;
use std::default;

use nix::sys::ioctl;

const DM_IOCTL: u8 = 0xfd;
const DM_CTL_PATH: &'static str= "/dev/mapper/control";


#[repr(C)]
struct DmIoctl {
    major_version: u32,	/* in/out */
    minor_version: u32,	/* in/out */
    patch_version: u32,	/* in/out */
    data_size: u32,	/* total size of data passed in including this struct */
    data_start: u32,	/* offset to start of data relative to start of this struct */

    target_count: u32,	/* in/out */
    open_count: i32,	/* out */
    flags: u32,		/* in/out */

    event_nr: u32,    	/* in/out */
    padding: u32,

    dev: u64,		/* in/out */

    name: [u8; 128],	/* device name */
    uuid: [u8; 129],	/* unique identifier for the block device */
    data: [u8; 7],	/* padding or data */
}

impl Default for DmIoctl {
    fn default() -> Self {
        DmIoctl {
            major_version: 4,
            minor_version: 30,
            patch_version: 0,
            data_size: 0,
            data_start: 0,
            target_count: 0,
            open_count: 0,
            flags: 0,
            event_nr: 0,
            padding: 0,
            dev: 0,
            name: [0; 128],
            uuid: [0; 129],
            data: [0; 7],
        }
    }
}

pub fn dostuff() -> Result<()> {

    let mut f = try!(fs::File::open(DM_CTL_PATH));

    let rawfd = f.as_raw_fd();

    println!("rawfd {}", rawfd);

    let mut x: DmIoctl = Default::default();

    let op = ioctl::op_read(DM_IOCTL, 0, 312);

    println!("op {:?}", op);

    unsafe {
        ioctl::read_into(rawfd, op, &mut x);
    }
    println!("k patchlevel {:?}", x.patch_version);

    Ok(())
}

