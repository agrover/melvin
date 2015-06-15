// Copyright © 2015 Sebastian Thiel
// Copyright © 2015 Andy Grover
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
//
// THE SOFTWARE IS PROVIDED “AS IS”, WITHOUT WARRANTY OF ANY KIND,
// EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
// MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
// NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE
// LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
// OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION
// WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

// Base a lexer for LVM2's text format on the more complex (hah) json format.
// This code is based on https://github.com/Byron/json-tools.

//
// Given a &[u8], the lexer produces a stream of tokens.
// get_textmap takes tokens and produces a TextMap nested
// structure. Finally vg_from_textmap converts into an actual
// VG struct, with associated LVs and PVs.
//


use std::io;
use std::io::Result;
use std::io::Error;
use std::io::ErrorKind::Other;

use std::collections::btree_map::BTreeMap;

#[derive(Debug, PartialEq, Clone)]
pub enum Token<'a> {
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
    String(&'a[u8]),

    Ident(&'a[u8]),

    /// An unsigned integer number
    Number(u64),

    Comment(&'a[u8]),

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

pub struct Lexer<'a> {
    chars: &'a[u8],
    next_byte: Option<u8>,
    cursor: usize,
}

impl<'a> Lexer<'a> {
    /// Returns a new Lexer from a given byte iterator.
    pub fn new(chars: &'a[u8]) -> Lexer<'a> {
        Lexer {
            chars: chars,
            next_byte: None,
            cursor: 0,
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
            },
            None => {
                if self.cursor >= self.chars.len() {
                    None
                }
                else {
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
                            return Some(Token::CurlyOpen);
                        },
                        b'}' => {
                            return Some(Token::CurlyClose);
                        },
                        b'"' => {
                            state = Mode::String(self.cursor - 1);
                        },
                        b'a' ... b'z' | b'A' ... b'Z' | b'_' | b'.' => {
                            state = Mode::Ident(self.cursor - 1);
                        },
                        b'0' ... b'9' => {
                            state = Mode::Number(self.cursor - 1);
                        },
                        b'#' => {
                            state = Mode::Comment(self.cursor - 1);
                        },
                        b'[' => {
                            return Some(Token::BracketOpen);
                        },
                        b']' => {
                            return Some(Token::BracketClose);
                        },
                        b'=' => {
                            return Some(Token::Equals);
                        },
                        b',' => {
                            return Some(Token::Comma);
                        },
                        b' ' | b'\n' | b'\t' | b'\0' => {
                            // ignore whitespace
                        }
                        _ => {
                            return Some(Token::Invalid(c));
                        },
                    }
                }
                Mode::String(first) => {
                    match c {
                        b'"' => {
                            return Some(Token::String(
                                &self.chars[first+1..self.cursor-1]));
                        },
                        _ => {
                            continue;
                        }
                    }
                },
                Mode::Ident(first) => {
                    match c {
                        b'a' ... b'z' | b'A' ... b'Z' | b'0' ... b'9'
                            | b'_' | b'.' | b'-' => {
                                continue;
                            }
                        _ => {
                            self.put_back(c);
                            return Some(Token::Ident(
                                &self.chars[first..self.cursor]));
                        }
                    }
                },
                Mode::Number(first) => {
                    match c {
                        b'0' ... b'9' => {
                            continue;
                        },
                        _ => {
                            self.put_back(c);
                            // HACK
                            // If followed by =, we're not a number we're an ident.
                            // Only time this should be needed is
                            // dump() device_to_pvid section. Otherwise idents
                            // never start with 0..9
                            if self.chars[self.cursor..self.cursor+2].contains(&b'=') {
                                return Some(
                                    Token::Ident(&self.chars[first..self.cursor]));
                            } else {
                                let s = String::from_utf8_lossy(
                                    &self.chars[first..self.cursor]).into_owned();
                                return Some(
                                    Token::Number(s.parse().unwrap()));
                            }
                        }
                    }
                }
                Mode::Comment(first) => {
                    match c {
                        b'\n' => {
                            self.put_back(c);
                            return Some(Token::Comment(
                                &self.chars[first..self.cursor]));
                        }
                        _ => {
                            continue;
                        }
                    }
                },
            }
        }

        None
    }
}

pub type LvmTextMap = BTreeMap<String, Entry>;

#[derive(Debug, PartialEq, Clone)]
pub enum Entry {
    Number(u64),
    String(String),
    TextMap(Box<LvmTextMap>),
    List(Box<Vec<Entry>>),
}

pub trait MapFromMeta {
    fn u64_from_textmap(&mut self, name: &str) -> Option<u64>;
    fn string_from_textmap(&mut self, name: &str) -> Option<String>;
    fn textmap_from_textmap(&mut self, name: &str) -> Option<LvmTextMap>;
    fn list_from_textmap(&mut self, name: &str) -> Option<Vec<Entry>>;
}

impl MapFromMeta for LvmTextMap {
    fn u64_from_textmap(&mut self, name: &str) -> Option<u64> {
        match self.remove(name) {
            Some(Entry::Number(x)) => Some(x),
            _ => None
        }
    }
    fn string_from_textmap(&mut self, name: &str) -> Option<String> {
        match self.remove(name) {
            Some(Entry::String(x)) => Some(x),
            _ => None
        }
    }
    fn textmap_from_textmap(&mut self, name: &str) -> Option<LvmTextMap> {
        match self.remove(name) {
            Some(Entry::TextMap(x)) => Some(*x),
            _ => None
        }
    }
    fn list_from_textmap(&mut self, name: &str) -> Option<Vec<Entry>> {
        match self.remove(name) {
            Some(Entry::List(x)) => Some(*x),
            _ => None
        }
    }
}

fn find_matching_token<'a, 'b>(tokens: &'b[Token<'a>], begin: &Token<'a>, end: &Token<'a>) -> io::Result<&'b[Token<'a>]> {
    let mut brace_count = 0;

    for (i, x) in tokens.iter().enumerate() {
        match x {
            x if x == begin => {
                brace_count += 1;
            },
            x if x == end => {
                brace_count -= 1;
                if brace_count == 0 {
                    return Ok(&tokens[..i+1]);
                }
            },
            _ => {},
        }
    }
    Err(Error::new(Other, "token mismatch"))
}

// lists can only contain strings and numbers, yay
pub fn get_list<'a>(tokens: &[Token<'a>]) -> io::Result<Vec<Entry>> {
    let mut v = Vec::new();

    assert_eq!(*tokens.first().unwrap(), Token::BracketOpen);
    assert_eq!(*tokens.last().unwrap(), Token::BracketClose);

    // Omit enclosing brackets
    for tok in &tokens[1..tokens.len()-1] {
        match *tok {
            Token::Number(x) => v.push(Entry::Number(x)),
            Token::String(x) => v.push(Entry::String(String::from_utf8_lossy(x).into_owned())),
            Token::Comma => { },
            _ => return Err(Error::new(
                Other, format!("Unexpected {:?}", *tok)))
        }
    }

    Ok(v)
}

fn get_textmap<'a>(tokens: &[Token<'a>]) -> io::Result<LvmTextMap> {
    let mut ret: LvmTextMap = BTreeMap::new();

    assert_eq!(*tokens.first().unwrap(), Token::CurlyOpen);
    assert_eq!(*tokens.last().unwrap(), Token::CurlyClose);

    let mut cur = 1;

    while tokens[cur] != Token::CurlyClose {

        let ident = match tokens[cur] {
            Token::Ident(x) => String::from_utf8_lossy(x).into_owned(),
            Token::Comment(_) => {
                cur += 1;
                continue
            },
            _ => return Err(Error::new(
                Other, format!("Unexpected {:?} when seeking ident", tokens[cur])))
        };

        cur += 1;
        match tokens[cur] {
            Token::Equals => {
                cur += 1;
                match tokens[cur] {
                    Token::Number(x) => {
                        cur += 1;
                        ret.insert(ident, Entry::Number(x));
                    },
                    Token::String(x) => {
                        cur += 1;
                        ret.insert(ident, Entry::String(
                            String::from_utf8_lossy(x).into_owned()));
                    },
                    Token::BracketOpen => {
                        let slc = try!(find_matching_token(
                            &tokens[cur..], &Token::BracketOpen, &Token::BracketClose));
                        ret.insert(ident, Entry::List(
                            Box::new(try!(get_list(&slc)))));
                        cur += slc.len();
                    }
                    _ => return Err(Error::new(
                        Other, format!("Unexpected {:?} as rvalue", tokens[cur])))
                }
            },
            Token::CurlyOpen => {
                let slc = try!(find_matching_token(
                    &tokens[cur..], &Token::CurlyOpen, &Token::CurlyClose));
                ret.insert(ident, Entry::TextMap(
                    Box::new(try!(get_textmap(&slc)))));
                cur += slc.len();
            }
            _ => return Err(Error::new(
                Other, format!("Unexpected {:?} after an ident", tokens[cur])))
        };
    }

    Ok(ret)
}

pub fn into_textmap(buf: &[u8]) -> io::Result<LvmTextMap> {

    let mut tokens: Vec<Token> = Vec::new();

    // LVM vsn1 is implicitly a map at the top level, so add
    // the appropriate tokens
    tokens.push(Token::CurlyOpen);
    tokens.append(&mut Lexer::new(&buf).collect());
    tokens.push(Token::CurlyClose);

    get_textmap(&tokens)
}

#[derive(Debug, PartialEq, Clone)]
pub struct VG {
    name: String,
    id: String,
    seqno: u64,
    format: String,
    status: Vec<String>,
    flags: Vec<String>,
    extent_size: u64,
    max_lv: u64,
    max_pv: u64,
    metadata_copies: u64,
    pvs: Vec<PV>,
    lvs: Vec<LV>,
}

#[derive(Debug, PartialEq, Clone)]
pub struct PV {
    name: String,
    id: String,
    status: Vec<String>,
    flags: Vec<String>,
    dev_size: u64,
    pe_start: u64,
    pe_count: u64,
}

#[derive(Debug, PartialEq, Clone)]
pub struct Segment {
    name: String,
    start_extent: u64,
    extent_count: u64,
    ty: String,
    stripe_count: u64,
    stripes: Vec<(String, u64)>,
}

#[derive(Debug, PartialEq, Clone)]
pub struct LV {
    name: String,
    id: String,
    status: Vec<String>,
    flags: Vec<String>,
    creation_host: String,
    creation_time: u64,
    segment_count: u64,
    segments: Vec<Segment>,
}

fn pvs_from_textmap(map: LvmTextMap) -> Result<Vec<PV>> {
    let err = || Error::new(Other, "dude");

    let mut ret_vec = Vec::new();

    for (key, value) in map {
        let mut pv_dict = match value {
            Entry::TextMap(x) => *x,
            _ => return Err(Error::new(Other, "dude")),
        };

        let id = try!(pv_dict.string_from_textmap("id").ok_or(err()));
        let dev_size = try!(pv_dict.u64_from_textmap("dev_size").ok_or(err()));
        let pe_start = try!(pv_dict.u64_from_textmap("pe_start").ok_or(err()));
        let pe_count = try!(pv_dict.u64_from_textmap("pe_count").ok_or(err()));

        let status: Vec<_> = try!(pv_dict.list_from_textmap("status").ok_or(err()))
            .into_iter()
            .filter_map(|item| match item { Entry::String(x) => Some(x), _ => {None}})
            .collect();

        let flags: Vec<_> = try!(pv_dict.list_from_textmap("flags").ok_or(err()))
            .into_iter()
            .filter_map(|item| match item { Entry::String(x) => Some(x), _ => {None}})
            .collect();

        ret_vec.push(PV {
            name: key,
            id: id,
            status: status,
            flags: flags,
            dev_size: dev_size,
            pe_start: pe_start,
            pe_count: pe_count,
            });
    }

    Ok(ret_vec)
}

fn segments_from_textmap(segment_count: u64, map: &mut LvmTextMap) ->Result<Vec<Segment>> {
    let err = || Error::new(Other, "dude");

    let mut segments = Vec::new();
    for i in 0..segment_count {
        let name = format!("segment{}", i+1);
        let mut seg_dict = try!(map.textmap_from_textmap(&name).ok_or(err()));

        let mut stripes: Vec<_> = Vec::new();
        let mut stripe_list = try!(seg_dict.list_from_textmap("stripes").ok_or(err()));

        while stripe_list.len()/2 != 0 {
            let name = match stripe_list.remove(0) {
                Entry::String(x) => x, _ => return Err(err())
            };
            let val = match stripe_list.remove(0) {
                Entry::Number(x) => x, _ => return Err(err())
            };
            stripes.push((name, val));
        }

        segments.push(Segment{
            name: name,
            start_extent: try!(seg_dict.u64_from_textmap("start_extent").ok_or(err())),
            extent_count: try!(seg_dict.u64_from_textmap("extent_count").ok_or(err())),
            ty: try!(seg_dict.string_from_textmap("type").ok_or(err())),
            stripe_count: try!(seg_dict.u64_from_textmap("stripe_count").ok_or(err())),
            stripes: stripes,
        });
    }

    Ok(segments)
}

fn lvs_from_textmap(map: LvmTextMap) -> Result<Vec<LV>> {
    let err = || Error::new(Other, "dude");

    let mut ret_vec = Vec::new();

    for (key, value) in map {
        let mut lv_dict = match value {
            Entry::TextMap(x) => *x,
            _ => return Err(Error::new(Other, "dude")),
        };

        let id = try!(lv_dict.string_from_textmap("id").ok_or(err()));
        let creation_host = try!(lv_dict.string_from_textmap("creation_host")
                                 .ok_or(err()));
        let creation_time = try!(lv_dict.u64_from_textmap("creation_time")
                                 .ok_or(err()));
        let segment_count = try!(lv_dict.u64_from_textmap("segment_count")
                                 .ok_or(err()));

        let segments = try!(segments_from_textmap(segment_count, &mut lv_dict));

        let status: Vec<_> = try!(lv_dict.list_from_textmap("status").ok_or(err()))
            .into_iter()
            .filter_map(|item| match item { Entry::String(x) => Some(x), _ => {None}})
            .collect();

        let flags: Vec<_> = try!(lv_dict.list_from_textmap("flags").ok_or(err()))
            .into_iter()
            .filter_map(|item| match item { Entry::String(x) => Some(x), _ => {None}})
            .collect();

        ret_vec.push(LV {
            name: key,
            id: id,
            status: status,
            flags: flags,
            creation_host: creation_host,
            creation_time: creation_time,
            segment_count: segment_count,
            segments: segments,
            });
    }

    Ok(ret_vec)
}

pub fn vg_from_textmap(name: &str, map: &mut LvmTextMap) -> Result<VG> {

    let err = || Error::new(Other, "dude");

    let id = try!(map.string_from_textmap("id").ok_or(err()));
    let seqno = try!(map.u64_from_textmap("seqno").ok_or(err()));
    let format = try!(map.string_from_textmap("format").ok_or(err()));
    let extent_size = try!(map.u64_from_textmap("extent_size").ok_or(err()));
    let max_lv = try!(map.u64_from_textmap("max_lv").ok_or(err()));
    let max_pv = try!(map.u64_from_textmap("max_pv").ok_or(err()));
    let metadata_copies = try!(map.u64_from_textmap("metadata_copies").ok_or(err()));

    let status: Vec<_> = try!(map.list_from_textmap("status").ok_or(err()))
        .into_iter()
        .filter_map(|item| match item { Entry::String(x) => Some(x), _ => {None}})
        .collect();

    let flags: Vec<_> = try!(map.list_from_textmap("flags").ok_or(err()))
        .into_iter()
        .filter_map(|item| match item { Entry::String(x) => Some(x), _ => {None}})
        .collect();

    let pv_meta = try!(map.textmap_from_textmap("physical_volumes").ok_or(err()));
    let pvs = try!(pvs_from_textmap(pv_meta));

    let lv_meta = try!(map.textmap_from_textmap("logical_volumes").ok_or(err()));
    let lvs = try!(lvs_from_textmap(lv_meta));

    let vg = VG {
        name: name.to_string(),
        id: id,
        seqno: seqno,
        format: format,
        status: status,
        flags: flags,
        extent_size: extent_size,
        max_lv: max_lv,
        max_pv: max_pv,
        metadata_copies: metadata_copies,
        pvs: pvs,
        lvs: lvs,
    };

    Ok(vg)
}
