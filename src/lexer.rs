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

/// A lexer for utf-8 encoded json data
pub struct Lexer<I: IntoIterator<Item=u8>> {
    chars: I::IntoIter,
    next_byte: Option<u8>,
    cursor: u64,
}

#[derive(Debug, PartialEq, Clone)]
pub enum TokenType {
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
    String,

    Ident,

    /// An unsigned integer number
    Number,

    /// The type of the token could not be identified.
    /// Should be removed if this lexer is ever to be feature complete
    Invalid,
}

impl AsRef<str> for TokenType {
    fn as_ref(&self) -> &str {
        match *self {
            TokenType::CurlyOpen => "{",
            TokenType::CurlyClose => "}",
            TokenType::BracketOpen => "[",
            TokenType::BracketClose => "]",
            TokenType::Equals => "=",
            TokenType::Comma => ",",

            TokenType::Invalid => panic!("Cannot convert invalid TokenType"),
            _ => panic!("Cannot convert variant TokenTypes"),
        }
    }
}

/// A pair of indices into the byte stream returned by our source 
/// iterator.
/// It is an exclusive range.
#[derive(Debug, PartialEq, Clone, Default)]
pub struct Span {
    /// Index of the first the byte
    pub first: u64,
    /// Index one past the last byte
    pub end: u64,
}

/// A lexical token, identifying its kind and span.
#[derive(Debug, PartialEq, Clone)]
pub struct Token {
    /// The exact type of the token
    pub kind: TokenType,

    /// A buffer representing the bytes of this Token. 
    pub buf: Span
}

impl<I> Lexer<I> where I: IntoIterator<Item=u8> {
    /// Returns a new Lexer from a given byte iterator.
    pub fn new(chars: I) -> Lexer<I> {
        Lexer {
            chars: chars.into_iter(),
            next_byte: None,
            cursor: 0,
        }
    }

    pub fn into_inner(self) -> I::IntoIter {
        self.chars
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
                let res = self.chars.next();
                match res {
                    None => None,
                    Some(_) => {
                        self.cursor += 1;
                        res
                    }
                }
            }
        }
    }
}

// Identifies the state of the lexer
enum Mode {
    // String parse mode: bool = ignore_next
    String(bool),
    Ident,
    Number,
    SlowPath,
}

impl<I> Iterator for Lexer<I> 
                    where I: IntoIterator<Item=u8> {
    type Item = Token;

    /// Lex the underlying byte stream to generate tokens
    fn next(&mut self) -> Option<Token> {
        let mut t: Option<TokenType> = None;

        let mut first = 0;
        let mut state = Mode::SlowPath;
        let last_cursor = self.cursor;

        while let Some(c) = self.next_byte() {
            let mut set_cursor = |cursor| {
                first = cursor - 1;
            };

            match state {
                Mode::String(ref mut ign_next) => {
                    if *ign_next && (c == b'"' || c == b'\\') {
                        *ign_next = false;
                        continue;
                    }
                    match c {
                        b'"' => {
                            t = Some(TokenType::String);
                            break;
                        },
                        b'\\' => {
                            *ign_next = true;
                            continue;
                        },
                        _ => {
                            continue;
                        }
                    }
                },
                Mode::Ident => {
                    match c {
                        b'a' ... b'z' | b'_' => {
                            continue;
                        }
                        _ => {
                            t = Some(TokenType::Ident);
                            break;
                        }
                    }
                },
                Mode::Number => {
                    match c {
                         b'0' ... b'9'
                        |b'-'
                        |b'.' => {
                            continue;
                        },
                        _ => {
                            t = Some(TokenType::Number);
                            self.put_back(c);
                            break;
                        }
                    }
                }
                Mode::SlowPath => {
                    match c {
                        b'{' => { t = Some(TokenType::CurlyOpen); set_cursor(self.cursor); break; },
                        b'}' => { t = Some(TokenType::CurlyClose); set_cursor(self.cursor); break; },
                        b'"' => {
                            state = Mode::String(false);
                            set_cursor(self.cursor);
                            // it starts at invalid, and once we know it closes, it's a string
                            t = Some(TokenType::Invalid);
                        },
                        b'a' ... b'z' | b'_' => {
                            state = Mode::Ident;
                            set_cursor(self.cursor);
                        },
                        b'0' ... b'9' => {
                            state = Mode::Number;
                            set_cursor(self.cursor);
                        },
                        b'[' => { t = Some(TokenType::BracketOpen); set_cursor(self.cursor); break; },
                        b']' => { t = Some(TokenType::BracketClose); set_cursor(self.cursor); break; },
                        b'=' => { t = Some(TokenType::Equals); set_cursor(self.cursor); break; },
                        b',' => { t = Some(TokenType::Comma); set_cursor(self.cursor); break; },
                        b'\\' => {
                            // invalid
                            t = Some(TokenType::Invalid);
                            set_cursor(self.cursor);
                            break
                        }
                        _ => {

                        },
                    }// end single byte match
                }// end case SlowPath
            }// end match state
        }// end for each byte

        match t {
            None => None,
            Some(t) => {
                if self.cursor == last_cursor {
                    None
                } else {
                    Some(Token {
                        kind: t,
                        buf : Span {first: first, end: self.cursor }
                    })
                }
            }
        }
    }
}
