#![feature(collections)]

extern crate byteorder;
extern crate crc;
extern crate unix_socket;
extern crate nix;

use std::path;

mod lexer;
mod lvmetad;
mod pvlabel;

fn main() {

    let dirs = vec![path::Path::new("/dev")];

    let pvs = pvlabel::scan_for_pvs(&dirs).unwrap();

    let x = lvmetad::request(b"token_update", true).unwrap();

    println!("pv_list {}", String::from_utf8_lossy(&x));

    lexer::lex_and_structize(&x);

    let x = lvmetad::request(b"pv_list", true).unwrap();

    println!("pv_list {}", String::from_utf8_lossy(&x));

//    let x = lvmetad::request(b"vg_list", true).unwrap();

//    println!("vg_list {}", String::from_utf8_lossy(&x));

//    let x = lvmetad::request(b"dump", false).unwrap();

//    println!("dump {}", String::from_utf8_lossy(&x));

    println!("{:?}", pvlabel::metadata_from_dev(&pvs[0]));
}
