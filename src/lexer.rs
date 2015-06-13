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

use std::io;
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
    Dict(Box<LvmTextMap>),
    List(Box<Vec<Entry>>),
}

pub trait MapFromMeta {
    fn u64_from_meta(&mut self, name: &str) -> Option<u64>;
    fn string_from_meta(&mut self, name: &str) -> Option<String>;
    fn dict_from_meta(&mut self, name: &str) -> Option<BTreeMap<String, Entry>>;
    fn list_from_meta(&mut self, name: &str) -> Option<Vec<Entry>>;
}

impl MapFromMeta for LvmTextMap {
    fn u64_from_meta(&mut self, name: &str) -> Option<u64> {
        match self.remove(name) {
            Some(Entry::Number(x)) => Some(x),
            _ => None
        }
    }
    fn string_from_meta(&mut self, name: &str) -> Option<String> {
        match self.remove(name) {
            Some(Entry::String(x)) => Some(x),
            _ => None
        }
    }
    fn dict_from_meta(&mut self, name: &str) -> Option<LvmTextMap> {
        match self.remove(name) {
            Some(Entry::Dict(x)) => Some(*x),
            _ => None
        }
    }
    fn list_from_meta(&mut self, name: &str) -> Option<Vec<Entry>> {
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

fn get_hash<'a>(tokens: &[Token<'a>]) -> io::Result<BTreeMap<String, Entry>> {
    let mut ret: BTreeMap<String, Entry> = BTreeMap::new();

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
                ret.insert(ident, Entry::Dict(
                    Box::new(try!(get_hash(&slc)))));
                cur += slc.len();
            }
            _ => return Err(Error::new(
                Other, format!("Unexpected {:?} after an ident", tokens[cur])))
        };
    }

    Ok(ret)
}

pub fn lex_and_structize(buf: &[u8]) -> io::Result<BTreeMap<String, Entry>> {

    let mut tokens: Vec<Token> = Vec::new();

    // LVM vsn1 is implicitly a map at the top level, so add
    // the appropriate tokens
    tokens.push(Token::CurlyOpen);
    tokens.append(&mut Lexer::new(&buf).collect());
    tokens.push(Token::CurlyClose);

    get_hash(&tokens)
}
