// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

#![feature(iter_arith, path_ext, slice_bytes)]
#![warn(missing_docs)]

//! Melvin is a library for configuring logical volumes in the style of [LVM](https://www.sourceware.org/lvm2/)
//! Melvin is not compatible with LVM.

extern crate byteorder;
extern crate crc;
extern crate unix_socket;
extern crate nix;
extern crate libc;
extern crate uuid;
extern crate time;

pub mod dm;
pub mod parser;
mod metad;
mod pvlabel;
mod lv;
mod vg;
mod pv;
mod util;

pub use vg::VG;
pub use pv::PV;
pub use pv::dev::Device;
pub use lv::LV;
pub use metad::vg_list;
pub use pvlabel::{PvHeader, pvheader_scan};
