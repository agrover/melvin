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

/// A lexer for utf-8 encoded json data
pub struct Lexer<I: IntoIterator<Item=u8>> {
    chars: I::IntoIter,
    next_byte: Option<u8>,
    cursor: usize,
}

#[derive(Debug, PartialEq, Clone)]
pub enum Token {
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
    String(Span),

    Ident(Span),

    /// An unsigned integer number
    Number(Span),

    Comment(Span),

    /// The type of the token could not be identified.
    /// Should be removed if this lexer is ever to be feature complete
    Invalid(u8),
}

impl AsRef<str> for Token {
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

/// A pair of indices into the byte stream returned by our source 
/// iterator.
/// It is an exclusive range.
#[derive(Debug, PartialEq, Clone, Default)]
pub struct Span {
    /// Index of the first the byte
    pub begin: usize,
    /// Index one past the last byte
    pub end: usize,
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
    // tells position where these modes were started
    String(usize),
    Ident(usize),
    Number(usize),
    Comment(usize),
    Main,
}

impl<I> Iterator for Lexer<I> 
    where I: IntoIterator<Item=u8> {
        type Item = Token;

        /// Lex the underlying byte stream to generate tokens
        fn next(&mut self) -> Option<Token> {

            let mut state = Mode::Main;

            while let Some(c) = self.next_byte() {

                match state {
                    Mode::String(first) => {
                        match c {
                            b'"' => {
                                return Some(Token::String({Span {begin: first + 1, end: self.cursor - 1 }}));
                            },
                            _ => {
                                continue;
                            }
                        }
                    },
                    Mode::Ident(first) => {
                        match c {
                            b'a' ... b'z' | b'_' => {
                                continue;
                            }
                            _ => {
                                return Some(Token::Ident({Span {begin: first, end: self.cursor }}));
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
                                return Some(Token::Number({Span {begin: first, end: self.cursor}}));
                            }
                        }
                    }
                    Mode::Comment(first) => {
                        match c {
                            b'\n' => {
                                self.put_back(c);
                                return Some(Token::Comment({Span {begin: first, end: self.cursor}}));
                            }
                            _ => {
                                continue;
                            }
                        }
                    },
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
                            b'a' ... b'z' | b'_' => {
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
                }
            }

            None
        }
    }
