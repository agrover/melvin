#![feature(iter_arith, result_expect, path_ext, slice_bytes)]
//#![warn(missing_docs)]

extern crate byteorder;
extern crate crc;
extern crate unix_socket;
extern crate nix;
extern crate libc;
extern crate uuid;
extern crate time;

pub mod parser;
pub mod lvmetad;
pub mod pvlabel;
pub mod dm;
pub mod lv;
pub mod vg;
pub mod pv;
mod util;

#[allow(dead_code, non_camel_case_types)]
mod dm_ioctl;
