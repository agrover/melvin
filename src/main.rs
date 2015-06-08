#![feature(collections)]

extern crate byteorder;
extern crate crc;
extern crate unix_socket;
extern crate nix;

use std::path;

mod lexer;
mod lvmetad;
mod pvlabel;

use lexer::{Lexer, Token};


fn main() {

    let dirs = vec![path::Path::new("/dev")];

    let pvs = pvlabel::scan_for_pvs(&dirs);

    //open_lvmetad();
}

fn lex_and_print(s: &[u8]) -> () {
    for token in Lexer::new(&s) {
        match token {
            Token::String(x) => { println!("string {}", String::from_utf8_lossy(x)) },
            Token::Comment(x) => { println!("comment {}", String::from_utf8_lossy(x)) },
            Token::Number(x) => { println!("number {}", x) },
            Token::Ident(x) => { println!("ident {}", String::from_utf8_lossy(x)) },
            _ => { println!("{:?}", token); },
        };
    }
}
