// This code is based on https://github.com/Byron/json-tools .
// Copyright © 2015 Sebastian Thiel
//
// Permission is hereby granted, free of charge, to any person obtaining
// a copy of this software and associated documentation files (the
// “Software”), to deal in the Software without restriction, including
// without limitation the rights to use, copy, modify, merge, publish,
// distribute, sublicense, and/or sell copies of the Software, and to
// permit persons to whom the Software is furnished to do so, subject to
// the following conditions:
//
// The above copyright notice and this permission notice shall be
// included in all copies or substantial portions of the Software.

// Copyright © 2015 Andy Grover
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// Base a lexer for LVM2's text format on the more complex (hah) json format.
//
// Given a &[u8], the lexer produces a stream of tokens.
// get_textmap takes tokens and produces a TextMap nested
// structure. Finally vg_from_textmap converts into an actual
// VG struct, with associated LVs and PVs.
//

//! Parsing LVM's text-based configuration format.

use std::io;
use std::io::ErrorKind::Other;

use std::collections::BTreeMap;

use crate::{Error, Result};

#[derive(Debug, PartialEq, Clone)]
enum Token<'a> {
    /// `{`
    CurlyOpen,
    /// `}`
    CurlyClose,

    /// `[`
    BracketOpen,
    /// `]`
    BracketClose,

    /// `=`
    Equals,
    /// `,`
    Comma,

    /// A string , like `"foo"`
    String(&'a [u8]),

    Ident(&'a [u8]),

    /// An unsigned integer number
    Number(i64),

    Comment(&'a [u8]),

    /// The type of the token could not be identified.
    /// Should be removed if this lexer is ever to be feature complete
    Invalid(u8),
}

impl<'a> AsRef<str> for Token<'a> {
    fn as_ref(&self) -> &str {
        match *self {
            Token::CurlyOpen => "{",
            Token::CurlyClose => "}",
            Token::BracketOpen => "[",
            Token::BracketClose => "]",
            Token::Equals => "=",
            Token::Comma => ",",

            Token::Invalid(c) => panic!("Cannot convert invalid Token {}", c),
            _ => panic!("Cannot convert variant Tokens"),
        }
    }
}

struct Lexer<'a> {
    chars: &'a [u8],
    next_byte: Option<u8>,
    cursor: usize,
    next_is_ident: bool,
}

impl<'a> Lexer<'a> {
    /// Returns a new Lexer from a given byte iterator.
    fn new(chars: &'a [u8]) -> Lexer<'a> {
        Lexer {
            chars,
            next_byte: None,
            cursor: 0,
            next_is_ident: false,
        }
    }

    fn put_back(&mut self, c: u8) {
        debug_assert!(self.next_byte.is_none());
        self.next_byte = Some(c);
        self.cursor -= 1;
    }

    fn next_byte(&mut self) -> Option<u8> {
        match self.next_byte.take() {
            Some(c) => {
                self.cursor += 1;
                Some(c)
            }
            None => {
                if self.cursor >= self.chars.len() {
                    None
                } else {
                    let res = self.chars[self.cursor];
                    self.cursor += 1;
                    Some(res)
                }
            }
        }
    }
}

// Identifies the state of the lexer
enum Mode {
    Main,

    // tells position where these modes were started
    String(usize),
    Ident(usize),
    Number(usize),
    Comment(usize),
}

impl<'a> Iterator for Lexer<'a> {
    type Item = Token<'a>;

    /// Lex the underlying byte stream to generate tokens
    fn next(&mut self) -> Option<Token<'a>> {
        let mut state = Mode::Main;

        while let Some(c) = self.next_byte() {
            match state {
                Mode::Main => {
                    match c {
                        b'{' => {
                            self.next_is_ident = true;
                            return Some(Token::CurlyOpen);
                        }
                        b'}' => {
                            return Some(Token::CurlyClose);
                        }
                        b'"' => {
                            state = Mode::String(self.cursor - 1);
                        }
                        b'a'..=b'z' | b'A'..=b'Z' | b'_' | b'.' => {
                            state = Mode::Ident(self.cursor - 1);
                        }
                        b'0'..=b'9' | b'-' => {
                            if self.next_is_ident {
                                state = Mode::Ident(self.cursor - 1);
                            } else {
                                state = Mode::Number(self.cursor - 1);
                            }
                        }
                        b'#' => {
                            state = Mode::Comment(self.cursor - 1);
                        }
                        b'[' => {
                            return Some(Token::BracketOpen);
                        }
                        b']' => {
                            return Some(Token::BracketClose);
                        }
                        b'=' => {
                            return Some(Token::Equals);
                        }
                        b',' => {
                            return Some(Token::Comma);
                        }
                        b' ' | b'\n' | b'\t' | b'\0' => {
                            // ignore whitespace
                        }
                        _ => {
                            return Some(Token::Invalid(c));
                        }
                    }
                }
                Mode::String(first) => match c {
                    b'"' => {
                        return Some(Token::String(&self.chars[first + 1..self.cursor - 1]));
                    }
                    _ => {
                        continue;
                    }
                },
                Mode::Ident(first) => match c {
                    b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_' | b'.' | b'-' => {
                        continue;
                    }
                    _ => {
                        self.put_back(c);
                        self.next_is_ident = false;
                        return Some(Token::Ident(&self.chars[first..self.cursor]));
                    }
                },
                Mode::Number(first) => match c {
                    b'0'..=b'9' => {
                        continue;
                    }
                    _ => {
                        self.put_back(c);
                        let s =
                            String::from_utf8_lossy(&self.chars[first..self.cursor]).into_owned();
                        return Some(Token::Number(s.parse().unwrap()));
                    }
                },
                Mode::Comment(first) => match c {
                    b'\n' => {
                        self.put_back(c);
                        return Some(Token::Comment(&self.chars[first..self.cursor]));
                    }
                    _ => {
                        continue;
                    }
                },
            }
        }

        None
    }
}

/// A Map that represents LVM metadata.
///
/// This is an intermediate representation between LVM's textual metadata format
/// and actual Rust structs. It is an associative map in which each entry can
/// refer to either a `Number`, a `String`, a `List`, or another `LvmTextMap`.
pub type LvmTextMap = BTreeMap<String, Entry>;

/// Each value in an LvmTextMap is an Entry.
#[derive(Debug, PartialEq, Clone)]
pub enum Entry {
    /// An integral numeric value
    Number(i64),
    /// A text string
    String(String),
    /// An ordered list of strings and numbers, possibly both
    List(Vec<Entry>),
    /// A nested LvmTextMap
    TextMap(Box<LvmTextMap>),
}

/// Operations that can be used to extract values from an `LvmTextMap`.
///
/// One usually knows the type of a given attribute in an `LvmTextMap`,
/// and an attribute of another type is a configuration error. These
/// methods return a reference to the value of the looked-for attribute,
/// or None.
pub trait TextMapOps {
    /// Get an i64 value from a LvmTextMap.
    fn i64_from_textmap(&self, name: &str) -> Option<i64>;
    /// Get a reference to a string in an LvmTextMap.
    fn string_from_textmap(&self, name: &str) -> Option<&str>;
    /// Get a reference to a List within an LvmTextMap.
    fn list_from_textmap(&self, name: &str) -> Option<&Vec<Entry>>;
    /// Get a reference to a nested LvmTextMap within an LvmTextMap.
    fn textmap_from_textmap(&self, name: &str) -> Option<&LvmTextMap>;
}

impl TextMapOps for LvmTextMap {
    fn i64_from_textmap(&self, name: &str) -> Option<i64> {
        match self.get(name) {
            Some(&Entry::Number(ref x)) => Some(*x),
            _ => None,
        }
    }
    fn string_from_textmap(&self, name: &str) -> Option<&str> {
        match self.get(name) {
            Some(&Entry::String(ref x)) => Some(x),
            _ => None,
        }
    }
    fn textmap_from_textmap(&self, name: &str) -> Option<&LvmTextMap> {
        match self.get(name) {
            Some(&Entry::TextMap(ref x)) => Some(x),
            _ => None,
        }
    }
    fn list_from_textmap(&self, name: &str) -> Option<&Vec<Entry>> {
        match self.get(name) {
            Some(&Entry::List(ref x)) => Some(x),
            _ => None,
        }
    }
}

fn find_matching_token<'a, 'b>(
    tokens: &'b [Token<'a>],
    begin: &Token<'a>,
    end: &Token<'a>,
) -> Result<&'b [Token<'a>]> {
    let mut brace_count = 0;

    for (i, x) in tokens.iter().enumerate() {
        match x {
            x if x == begin => {
                brace_count += 1;
            }
            x if x == end => {
                brace_count -= 1;
                if brace_count == 0 {
                    return Ok(&tokens[..i + 1]);
                }
            }
            _ => {}
        }
    }
    Err(Error::Io(io::Error::new(Other, "token mismatch")))
}

// lists can only contain strings and numbers, yay
fn get_list<'a>(tokens: &[Token<'a>]) -> Result<Vec<Entry>> {
    let mut v = Vec::new();

    assert_eq!(*tokens.first().unwrap(), Token::BracketOpen);
    assert_eq!(*tokens.last().unwrap(), Token::BracketClose);

    // Omit enclosing brackets
    for tok in &tokens[1..tokens.len() - 1] {
        match *tok {
            Token::Number(x) => v.push(Entry::Number(x)),
            Token::String(x) => v.push(Entry::String(String::from_utf8_lossy(x).into_owned())),
            Token::Comma => {}
            _ => {
                return Err(Error::Io(io::Error::new(
                    Other,
                    format!("Unexpected {:?}", *tok),
                )))
            }
        }
    }

    Ok(v)
}

// TODO: More appropriate error type than Result
fn get_textmap<'a>(tokens: &[Token<'a>]) -> Result<LvmTextMap> {
    let mut ret: LvmTextMap = BTreeMap::new();

    assert_eq!(*tokens.first().unwrap(), Token::CurlyOpen);
    assert_eq!(*tokens.last().unwrap(), Token::CurlyClose);

    let mut cur = 1;

    while tokens[cur] != Token::CurlyClose {
        let ident = match tokens[cur] {
            Token::Ident(x) => String::from_utf8_lossy(x).into_owned(),
            Token::Comment(_) => {
                cur += 1;
                continue;
            }
            _ => {
                return Err(Error::Io(io::Error::new(
                    Other,
                    format!("Unexpected {:?} when seeking ident", tokens[cur]),
                )))
            }
        };

        cur += 1;
        match tokens[cur] {
            Token::Equals => {
                cur += 1;
                match tokens[cur] {
                    Token::Number(x) => {
                        cur += 1;
                        ret.insert(ident, Entry::Number(x));
                    }
                    Token::String(x) => {
                        cur += 1;
                        ret.insert(
                            ident,
                            Entry::String(String::from_utf8_lossy(x).into_owned()),
                        );
                    }
                    Token::BracketOpen => {
                        let slc = find_matching_token(
                            &tokens[cur..],
                            &Token::BracketOpen,
                            &Token::BracketClose,
                        )?;
                        ret.insert(ident, Entry::List(get_list(&slc)?));
                        cur += slc.len();
                    }
                    _ => {
                        return Err(Error::Io(io::Error::new(
                            Other,
                            format!("Unexpected {:?} as rvalue", tokens[cur]),
                        )))
                    }
                }
            }
            Token::CurlyOpen => {
                let slc =
                    find_matching_token(&tokens[cur..], &Token::CurlyOpen, &Token::CurlyClose)?;
                ret.insert(ident, Entry::TextMap(Box::new(get_textmap(&slc)?)));
                cur += slc.len();
            }
            _ => {
                return Err(Error::Io(io::Error::new(
                    Other,
                    format!("Unexpected {:?} after an ident", tokens[cur]),
                )))
            }
        };
    }

    Ok(ret)
}

/// Generate an `LvmTextMap` from a textual LVM configuration string.
///
/// LVM uses the same configuration file format for it's on-disk metadata,
/// as well as for the lvm.conf configuration file.
pub fn buf_to_textmap(buf: &[u8]) -> Result<LvmTextMap> {
    let mut tokens: Vec<Token> = Vec::new();

    // LVM vsn1 is implicitly a map at the top level, so add
    // the appropriate tokens
    tokens.push(Token::CurlyOpen);
    tokens.extend(&mut Lexer::new(&buf));
    tokens.push(Token::CurlyClose);

    get_textmap(&tokens)
}

/// Status may be either a string or a list of strings. Convert either
/// into a list of strings.
pub fn status_from_textmap(map: &LvmTextMap) -> Result<Vec<String>> {
    match map.get("status") {
        Some(&Entry::String(ref x)) => Ok(vec![x.clone()]),
        Some(&Entry::List(ref x)) => Ok({
            x.iter()
                .filter_map(|item| match item {
                    Entry::String(ref x) => Some(x.clone()),
                    _ => None,
                })
                .collect()
        }),
        _ => Err(Error::Io(io::Error::new(
            Other,
            "status textmap parsing error",
        ))),
    }
}

/// Generate a textual LVM configuration string from an LvmTextMap.
pub fn textmap_to_buf(tm: &LvmTextMap) -> Vec<u8> {
    let mut vec = Vec::new();

    for (k, v) in tm {
        match v {
            Entry::String(ref x) => {
                vec.extend(k.as_bytes());
                vec.extend(b" = \"");
                vec.extend(x.as_bytes());
                vec.extend(b"\"\n");
            }
            &Entry::Number(ref x) => {
                vec.extend(k.as_bytes());
                vec.extend(b" = ");
                vec.extend(format!("{}\n", x).as_bytes());
            }
            &Entry::List(ref x) => {
                vec.extend(k.as_bytes());
                vec.extend(b" = [");
                let z: Vec<_> = x
                    .iter()
                    .map(|x| match x {
                        Entry::String(ref x) => format!("\"{}\"", x),
                        Entry::Number(ref x) => format!("{}", x),
                        _ => panic!("should not be in lists"),
                    })
                    .collect();
                vec.extend(z.join(", ").as_bytes());
                vec.extend(b"]\n");
            }
            &Entry::TextMap(ref x) => {
                vec.extend(k.as_bytes());
                vec.extend(b" {\n");
                vec.extend(textmap_to_buf(x));
                vec.extend(b"}\n");
            }
        };
    }

    vec
}
