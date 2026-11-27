//! A small JSON reader for the tests of the glb and metadata writers.

use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<Json>),
    Object(BTreeMap<String, Json>),
}

impl Json {
    pub fn get(&self, key: &str) -> &Json {
        match self {
            Json::Object(map) => map.get(key).unwrap_or(&Json::Null),
            _ => &Json::Null,
        }
    }
    pub fn at(&self, i: usize) -> &Json {
        match self {
            Json::Array(items) => items.get(i).unwrap_or(&Json::Null),
            _ => &Json::Null,
        }
    }
    pub fn len(&self) -> usize {
        match self {
            Json::Array(items) => items.len(),
            Json::Object(map) => map.len(),
            _ => 0,
        }
    }
    pub fn num(&self) -> f64 {
        match self {
            Json::Number(n) => *n,
            other => panic!("not a number: {other:?}"),
        }
    }
    pub fn usize(&self) -> usize {
        self.num() as usize
    }
    pub fn str(&self) -> &str {
        match self {
            Json::String(s) => s,
            other => panic!("not a string: {other:?}"),
        }
    }
    pub fn is_null(&self) -> bool {
        *self == Json::Null
    }
}

pub fn parse(text: &str) -> Json {
    let mut p = Parser {
        bytes: text.as_bytes(),
        pos: 0,
    };
    let value = p.value();
    p.space();
    assert_eq!(p.pos, p.bytes.len(), "trailing data in JSON");
    value
}

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl Parser<'_> {
    fn space(&mut self) {
        while self.pos < self.bytes.len() && b" \t\r\n".contains(&self.bytes[self.pos]) {
            self.pos += 1;
        }
    }
    fn eat(&mut self, b: u8) {
        self.space();
        assert_eq!(self.bytes[self.pos], b, "at {}", self.pos);
        self.pos += 1;
    }
    fn peek(&mut self) -> u8 {
        self.space();
        self.bytes[self.pos]
    }
    fn value(&mut self) -> Json {
        match self.peek() {
            b'{' => {
                self.eat(b'{');
                let mut map = BTreeMap::new();
                if self.peek() == b'}' {
                    self.pos += 1;
                    return Json::Object(map);
                }
                loop {
                    let key = match self.value() {
                        Json::String(s) => s,
                        other => panic!("key {other:?}"),
                    };
                    self.eat(b':');
                    let value = self.value();
                    assert!(map.insert(key, value).is_none(), "duplicate key");
                    match self.peek() {
                        b',' => self.pos += 1,
                        _ => {
                            self.eat(b'}');
                            return Json::Object(map);
                        }
                    }
                }
            }
            b'[' => {
                self.eat(b'[');
                let mut items = Vec::new();
                if self.peek() == b']' {
                    self.pos += 1;
                    return Json::Array(items);
                }
                loop {
                    items.push(self.value());
                    match self.peek() {
                        b',' => self.pos += 1,
                        _ => {
                            self.eat(b']');
                            return Json::Array(items);
                        }
                    }
                }
            }
            b'"' => {
                self.pos += 1;
                let mut out = String::new();
                loop {
                    let c = self.bytes[self.pos];
                    self.pos += 1;
                    match c {
                        b'"' => return Json::String(out),
                        b'\\' => {
                            let e = self.bytes[self.pos];
                            self.pos += 1;
                            match e {
                                b'n' => out.push('\n'),
                                b't' => out.push('\t'),
                                b'r' => out.push('\r'),
                                b'u' => {
                                    let hex =
                                        std::str::from_utf8(&self.bytes[self.pos..self.pos + 4])
                                            .unwrap();
                                    out.push(
                                        char::from_u32(u32::from_str_radix(hex, 16).unwrap())
                                            .unwrap(),
                                    );
                                    self.pos += 4;
                                }
                                other => out.push(other as char),
                            }
                        }
                        _ => {
                            // Copy a whole UTF-8 sequence.
                            let start = self.pos - 1;
                            let len = match c {
                                0..=0x7f => 1,
                                0xc0..=0xdf => 2,
                                0xe0..=0xef => 3,
                                _ => 4,
                            };
                            out.push_str(
                                std::str::from_utf8(&self.bytes[start..start + len]).unwrap(),
                            );
                            self.pos = start + len;
                        }
                    }
                }
            }
            b't' => {
                self.pos += 4;
                Json::Bool(true)
            }
            b'f' => {
                self.pos += 5;
                Json::Bool(false)
            }
            b'n' => {
                self.pos += 4;
                Json::Null
            }
            _ => {
                let start = self.pos;
                while self.pos < self.bytes.len()
                    && b"+-0123456789.eE".contains(&self.bytes[self.pos])
                {
                    self.pos += 1;
                }
                let text = std::str::from_utf8(&self.bytes[start..self.pos]).unwrap();
                Json::Number(text.parse().unwrap_or_else(|_| panic!("number {text}")))
            }
        }
    }
}
