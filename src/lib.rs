// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

#![feature(iter_arith, result_expect, path_ext, slice_bytes)]
#![warn(missing_docs)]

//! Melvin is pure Rust library for configuring [LVM](https://www.sourceware.org/lvm2/).

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
