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
use std::io::Error;
use std::io::ErrorKind::Other;
use std::str::FromStr;

use std::collections::btree_map::BTreeMap;

use lv::{LV, Segment};
use vg::VG;
use pv::{PV, Device};

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
    Number(i64),

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
    next_is_ident: bool,
}

impl<'a> Lexer<'a> {
    /// Returns a new Lexer from a given byte iterator.
    pub fn new(chars: &'a[u8]) -> Lexer<'a> {
        Lexer {
            chars: chars,
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
                            self.next_is_ident = true;
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
                        b'0' ... b'9' | b'-' => {
                            if self.next_is_ident {
                                state = Mode::Ident(self.cursor - 1);
                            } else {
                                state = Mode::Number(self.cursor - 1);
                            }
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
                            self.next_is_ident = false;
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
                            let s = String::from_utf8_lossy(
                                &self.chars[first..self.cursor]).into_owned();
                            return Some(Token::Number(s.parse().unwrap()));
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
    Number(i64),
    String(String),
    TextMap(Box<LvmTextMap>),
    List(Box<Vec<Entry>>),
}

pub trait TextMapOps {
    fn i64_from_textmap(&self, name: &str) -> Option<i64>;
    fn string_from_textmap(&self, name: &str) -> Option<&str>;
    fn textmap_from_textmap(&self, name: &str) -> Option<&LvmTextMap>;
    fn list_from_textmap(&self, name: &str) -> Option<&Vec<Entry>>;
}

impl TextMapOps for LvmTextMap {
    fn i64_from_textmap(&self, name: &str) -> Option<i64> {
        match self.get(name) {
            Some(&Entry::Number(ref x)) => Some(x.clone()),
            _ => None
        }
    }
    fn string_from_textmap(&self, name: &str) -> Option<&str> {
        match self.get(name) {
            Some(&Entry::String(ref x)) => Some(x),
            _ => None
        }
    }
    fn textmap_from_textmap(&self, name: &str) -> Option<&LvmTextMap> {
        match self.get(name) {
            Some(&Entry::TextMap(ref x)) => Some(x),
            _ => None
        }
    }
    fn list_from_textmap(&self, name: &str) -> Option<&Vec<Entry>> {
        match self.get(name) {
            Some(&Entry::List(ref x)) => Some(x),
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

// TODO: More appropriate error type than io::Result
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

pub fn buf_to_textmap(buf: &[u8]) -> io::Result<LvmTextMap> {

    let mut tokens: Vec<Token> = Vec::new();

    // LVM vsn1 is implicitly a map at the top level, so add
    // the appropriate tokens
    tokens.push(Token::CurlyOpen);
    tokens.extend(&mut Lexer::new(&buf));
    tokens.push(Token::CurlyClose);

    get_textmap(&tokens)
}

// status may be either a string or a list of strings
fn status_from_textmap(map: &LvmTextMap) -> io::Result<Vec<String>> {
    match map.get("status") {
        Some(&Entry::String(ref x)) => Ok(vec!(x.clone())),
        Some(&Entry::List(ref x)) =>
            Ok({x.iter()
                .filter_map(|item| match item {
                    &Entry::String(ref x) => Some(x.clone()),
                    _ => {None}
                })
                .collect()
            }),
        _ => Err(Error::new(Other, "status textmap parsing error")),
    }
}

fn device_from_textmap(map: &LvmTextMap) -> io::Result<Device> {
    match map.get("device") {
        Some(&Entry::String(ref x)) => {
            match Device::from_str(x) {
                Ok(x) => Ok(x),
                Err(_) => Err(Error::new(Other, "could not parse string"))
            }
        },
        Some(&Entry::Number(x)) => Ok(Device::from(x)),
        _ => Err(Error::new(Other, "device textmap parsing error")),
    }
}

fn pvs_from_textmap(map: &LvmTextMap) -> io::Result<BTreeMap<String, PV>> {
    let err = || Error::new(Other, "pv textmap parsing error");

    let mut ret_vec = BTreeMap::new();

    for (key, value) in map {
        let pv_dict = match value {
            &Entry::TextMap(ref x) => x,
            _ => return Err(
                Error::new(Other, "expected textmap when parsing PV")),
        };

        let id = try!(pv_dict.string_from_textmap("id").ok_or(err()));
        let device = try!(device_from_textmap(pv_dict));
        let dev_size = try!(pv_dict.i64_from_textmap("dev_size").ok_or(err()));
        let pe_start = try!(pv_dict.i64_from_textmap("pe_start").ok_or(err()));
        let pe_count = try!(pv_dict.i64_from_textmap("pe_count").ok_or(err()));

        let status = try!(status_from_textmap(pv_dict));

        let flags: Vec<_> = try!(pv_dict.list_from_textmap("flags").ok_or(err()))
            .into_iter()
            .filter_map(|item| match item {
                &Entry::String(ref x) => Some(x.clone()),
                _ => {None}
            })
            .collect();

        // If textmap came from lvmetad, it may also include sections
        // for data area (da0) and metadata area (mda0). These are not
        // in the on-disk text metadata, but in the binary PV header.
        // Don't know if we need them, omitting for now.

        ret_vec.insert(key.clone(), PV {
            name: key.clone(),
            id: id.to_string(),
            device: device,
            status: status,
            flags: flags,
            dev_size: dev_size as u64,
            pe_start: pe_start as u64,
            pe_count: pe_count as u64,
            });
    }

    Ok(ret_vec)
}

fn segments_from_textmap(segment_count: u64, map: &LvmTextMap) ->io::Result<Vec<Segment>> {
    let err = || Error::new(Other, "segment textmap parsing error");

    let mut segments = Vec::new();
    for i in 0..segment_count {
        let name = format!("segment{}", i+1);
        let seg_dict = try!(map.textmap_from_textmap(&name).ok_or(err()));
        let stripe_list = try!(seg_dict.list_from_textmap("stripes").ok_or(err()));

        let mut stripes: Vec<_> = Vec::new();
        for slc in stripe_list.chunks(2) {
            let name = match &slc[0] {
                &Entry::String(ref x) => x.clone(), _ => return Err(err())
            };
            let val = match slc[1] {
                Entry::Number(x) => x, _ => return Err(err())
            };
            stripes.push((name, val as u64));
        }

        segments.push(Segment{
            name: name,
            start_extent: try!(
                seg_dict.i64_from_textmap("start_extent").ok_or(err())) as u64,
            extent_count: try!(
                seg_dict.i64_from_textmap("extent_count").ok_or(err())) as u64,
            ty: try!(
                seg_dict.string_from_textmap("type").ok_or(err())).to_string(),
            stripes: stripes,
        });
    }

    Ok(segments)
}

fn lvs_from_textmap(map: &LvmTextMap) -> io::Result<BTreeMap<String, LV>> {
    let err = || Error::new(Other, "lv textmap parsing error");

    let mut ret_vec = BTreeMap::new();

    for (key, value) in map {
        let lv_dict = match value {
            &Entry::TextMap(ref x) => x,
            _ => return Err(
                Error::new(Other,"expected textmap when parsing LV")),
        };

        let id = try!(lv_dict.string_from_textmap("id").ok_or(err()));
        let creation_host = try!(lv_dict.string_from_textmap("creation_host")
                                 .ok_or(err()));
        let creation_time = try!(lv_dict.i64_from_textmap("creation_time")
                                 .ok_or(err()));
        let segment_count = try!(lv_dict.i64_from_textmap("segment_count")
                                 .ok_or(err()));

        let segments = try!(segments_from_textmap(segment_count as u64, &lv_dict));

        let status = try!(status_from_textmap(lv_dict));

        let flags: Vec<_> = try!(lv_dict.list_from_textmap("flags").ok_or(err()))
            .into_iter()
            .filter_map(|item| match item { &Entry::String(ref x) => Some(x.clone()), _ => {None}})
            .collect();

        ret_vec.insert(key.clone(), LV {
            name: key.clone(),
            id: id.to_string(),
            status: status,
            flags: flags,
            creation_host: creation_host.to_string(),
            creation_time: creation_time,
            segments: segments,
            });
    }

    Ok(ret_vec)
}

pub fn vg_from_textmap(name: &str, map: &LvmTextMap) -> io::Result<VG> {

    let err = || Error::new(Other, "vg textmap parsing error");

    let id = try!(map.string_from_textmap("id").ok_or(err()));
    let seqno = try!(map.i64_from_textmap("seqno").ok_or(err()));
    let format = try!(map.string_from_textmap("format").ok_or(err()));
    let extent_size = try!(map.i64_from_textmap("extent_size").ok_or(err()));
    let max_lv = try!(map.i64_from_textmap("max_lv").ok_or(err()));
    let max_pv = try!(map.i64_from_textmap("max_pv").ok_or(err()));
    let metadata_copies = try!(map.i64_from_textmap("metadata_copies").ok_or(err()));

    let status = try!(status_from_textmap(map));

    let flags: Vec<_> = try!(map.list_from_textmap("flags").ok_or(err()))
        .into_iter()
        .filter_map(|item| match item { &Entry::String(ref x) => Some(x.clone()), _ => {None}})
        .collect();

    let pvs = try!(map.textmap_from_textmap("physical_volumes").ok_or(err())
                   .and_then(|tm| pvs_from_textmap(tm)));

    let lvs = match map.textmap_from_textmap("logical_volumes") {
        Some(ref x) => try!(lvs_from_textmap(x)),
        None => BTreeMap::new(),
    };

    let vg = VG {
        name: name.to_string(),
        id: id.to_string(),
        seqno: seqno as u64,
        format: format.to_string(),
        status: status,
        flags: flags,
        extent_size: extent_size as u64,
        max_lv: max_lv as u64,
        max_pv: max_pv as u64,
        metadata_copies: metadata_copies as u64,
        pvs: pvs,
        lvs: lvs,
    };

    Ok(vg)
}

pub fn textmap_to_buf(tm: &LvmTextMap) -> Vec<u8> {
    let mut vec = Vec::new();

    for (k, v) in tm {
        match v {
            &Entry::String(ref x) => {
                vec.extend(k.as_bytes());
                vec.extend(b" = \"");
                vec.extend(x.as_bytes());
                vec.extend(b"\"\n");
            },
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
                    .map(|x| {
                        match x {
                            &Entry::String(ref x) => format!("\"{}\"", x),
                            &Entry::Number(ref x) => format!("{}", x),
                            _ => panic!("should not be in lists"),
                        }})
                    .collect();
                vec.extend(z.connect(", ").as_bytes());
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
