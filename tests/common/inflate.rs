//! A minimal zlib inflater for the tests: meshbox writes PNG and glb but reads neither.
//! A copy of spritebox's decompressor.
#![allow(dead_code)]

#[derive(Debug)]
pub struct Error(pub String);
pub enum ErrorCode {
    InvalidArgument,
}
impl Error {
    fn new(_code: ErrorCode, message: impl Into<String>) -> Self {
        Error(message.into())
    }
}
pub type Result<T> = std::result::Result<T, Error>;

fn adler32(bytes: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &byte in bytes {
        a = (a + byte as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

const LENGTH_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
const LENGTH_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
const DIST_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const DIST_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];
fn reverse_bits(code: u32, len: u32) -> u32 {
    code.reverse_bits() >> (32 - len)
}

/// The order in which code-length code lengths are stored.
const CL_ORDER: [usize; 19] = [
    16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
];
const END_OF_BLOCK: usize = 256;
fn fixed_literal_lengths() -> [u8; 288] {
    let mut lengths = [8u8; 288];
    lengths[144..256].fill(9);
    lengths[256..280].fill(7);
    lengths
}
fn corrupt(message: &str) -> Error {
    Error::new(
        ErrorCode::InvalidArgument,
        format!("Malformed zlib data: {message}"),
    )
}

/// Reads bits least significant first, as DEFLATE stores them.
struct BitReader<'a> {
    data: &'a [u8],
    pos: usize,
    bits: u64,
    count: u32,
    /// Zero bits appended past the end of `data`, still in `bits`.
    padding: u32,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            pos: 0,
            bits: 0,
            count: 0,
            padding: 0,
        }
    }
    fn refill(&mut self) {
        while self.count <= 56 {
            let byte = match self.data.get(self.pos) {
                Some(&b) => b,
                None => {
                    self.padding += 8;
                    0
                }
            };
            self.pos += 1;
            self.bits |= (byte as u64) << self.count;
            self.count += 8;
        }
    }
    fn peek(&mut self, n: u32) -> u32 {
        if self.count < n {
            self.refill();
        }
        (self.bits & ((1u64 << n) - 1)) as u32
    }
    fn consume(&mut self, n: u32) -> Result<()> {
        self.bits >>= n;
        self.count -= n;
        if self.padding > self.count {
            return Err(corrupt("the stream ends early"));
        }
        Ok(())
    }
    fn bits(&mut self, n: u32) -> Result<u32> {
        let value = self.peek(n);
        self.consume(n)?;
        Ok(value)
    }
    fn align(&mut self) -> Result<()> {
        self.consume(self.count % 8)
    }
    /// Bytes of input consumed so far, once aligned.
    fn byte_position(&self) -> usize {
        self.pos - (self.count / 8) as usize
    }
    /// Copies `n` bytes after aligning to a byte.
    fn copy_bytes(&mut self, mut n: usize, out: &mut Vec<u8>) -> Result<()> {
        while n > 0 && self.count >= 8 {
            out.push(self.bits(8)? as u8);
            n -= 1;
        }
        if n > self.data.len().saturating_sub(self.pos) {
            return Err(corrupt("a stored block ends early"));
        }
        out.extend_from_slice(&self.data[self.pos..self.pos + n]);
        self.pos += n;
        Ok(())
    }
}

/// A canonical Huffman code decoded with one table lookup.
struct Decoder {
    /// Indexed by the next `max_len` bits: `symbol << 4 | length`, or 0 for no code.
    table: Vec<u16>,
    max_len: u32,
}

impl Decoder {
    fn new(lengths: &[u8]) -> Result<Self> {
        let mut count = [0u32; 16];
        for &len in lengths {
            count[len as usize] += 1;
        }
        count[0] = 0;
        let mut left = 1i64;
        for &c in &count[1..] {
            left = (left << 1) - c as i64;
            if left < 0 {
                return Err(corrupt("a Huffman code is over-subscribed"));
            }
        }
        let max_len = (1..16).rev().find(|&l| count[l] > 0).unwrap_or(1) as u32;
        let mut next = [0u32; 16];
        let mut code = 0u32;
        for len in 1..16 {
            code = (code + count[len - 1]) << 1;
            next[len] = code;
        }
        let mut table = vec![0u16; 1 << max_len];
        for (symbol, &len) in lengths.iter().enumerate() {
            if len == 0 {
                continue;
            }
            let len = len as u32;
            let reversed = reverse_bits(next[len as usize], len) as usize;
            next[len as usize] += 1;
            let entry = (symbol as u16) << 4 | len as u16;
            for slot in (reversed..table.len()).step_by(1 << len) {
                table[slot] = entry;
            }
        }
        Ok(Self { table, max_len })
    }
    fn decode(&self, reader: &mut BitReader) -> Result<usize> {
        let entry = self.table[reader.peek(self.max_len) as usize];
        if entry == 0 {
            return Err(corrupt("an invalid Huffman code"));
        }
        reader.consume((entry & 15) as u32)?;
        Ok((entry >> 4) as usize)
    }
}

/// Decompresses a zlib stream whose decompressed size is at most `max_output` bytes.
pub fn zlib_decompress(data: &[u8], max_output: usize) -> Result<Vec<u8>> {
    if data.len() < 2 {
        return Err(corrupt("the stream ends early"));
    }
    let (cmf, flg) = (data[0], data[1]);
    if cmf & 0x0f != 8 || cmf >> 4 > 7 || (cmf as u16 * 256 + flg as u16) % 31 != 0 {
        return Err(corrupt("the header is not a DEFLATE stream"));
    }
    if flg & 0x20 != 0 {
        return Err(Error::new(
            ErrorCode::InvalidArgument,
            "zlib streams with a preset dictionary are not supported",
        ));
    }
    let (out, used) = inflate(&data[2..], max_output)?;
    let trailer = data
        .get(2 + used..2 + used + 4)
        .ok_or_else(|| corrupt("the Adler-32 checksum is missing"))?;
    let expected = u32::from_be_bytes([trailer[0], trailer[1], trailer[2], trailer[3]]);
    if adler32(&out) != expected {
        return Err(corrupt("the Adler-32 checksum does not match"));
    }
    Ok(out)
}

/// Decompresses raw DEFLATE data; returns the output and the bytes of input used.
fn inflate(data: &[u8], max_output: usize) -> Result<(Vec<u8>, usize)> {
    let mut reader = BitReader::new(data);
    let mut out = Vec::new();
    let mut fixed: Option<(Decoder, Decoder)> = None;
    loop {
        let last = reader.bits(1)? == 1;
        match reader.bits(2)? {
            0 => {
                reader.align()?;
                let len = reader.bits(16)?;
                let nlen = reader.bits(16)?;
                if len != !nlen & 0xffff {
                    return Err(corrupt("a stored block's length check fails"));
                }
                if out.len() + len as usize > max_output {
                    return Err(corrupt("more data than the image needs"));
                }
                reader.copy_bytes(len as usize, &mut out)?;
            }
            1 => {
                if fixed.is_none() {
                    fixed = Some((
                        Decoder::new(&fixed_literal_lengths())?,
                        Decoder::new(&[5; 30])?,
                    ));
                }
                let (literals, distances) = fixed.as_ref().unwrap_or_else(|| unreachable!());
                inflate_block(&mut reader, literals, distances, &mut out, max_output)?;
            }
            2 => {
                let (literals, distances) = read_dynamic_codes(&mut reader)?;
                inflate_block(&mut reader, &literals, &distances, &mut out, max_output)?;
            }
            _ => return Err(corrupt("a block of reserved type 3")),
        }
        if last {
            break;
        }
    }
    reader.align()?;
    Ok((out, reader.byte_position()))
}

fn read_dynamic_codes(reader: &mut BitReader) -> Result<(Decoder, Decoder)> {
    let hlit = reader.bits(5)? as usize + 257;
    let hdist = reader.bits(5)? as usize + 1;
    let hclen = reader.bits(4)? as usize + 4;
    if hlit > 286 || hdist > 30 {
        return Err(corrupt("too many codes in a dynamic block"));
    }
    let mut cl_lengths = [0u8; 19];
    for &i in &CL_ORDER[..hclen] {
        cl_lengths[i] = reader.bits(3)? as u8;
    }
    let cl = Decoder::new(&cl_lengths)?;
    let mut lengths = vec![0u8; hlit + hdist];
    let mut i = 0;
    while i < lengths.len() {
        let symbol = cl.decode(reader)?;
        let (value, repeat) = match symbol {
            0..=15 => (symbol as u8, 1),
            16 => {
                let previous = *i
                    .checked_sub(1)
                    .and_then(|p| lengths.get(p))
                    .ok_or_else(|| corrupt("a repeat with no previous length"))?;
                (previous, 3 + reader.bits(2)? as usize)
            }
            17 => (0, 3 + reader.bits(3)? as usize),
            _ => (0, 11 + reader.bits(7)? as usize),
        };
        if i + repeat > lengths.len() {
            return Err(corrupt("code lengths overflow their table"));
        }
        lengths[i..i + repeat].fill(value);
        i += repeat;
    }
    if lengths[END_OF_BLOCK] == 0 {
        return Err(corrupt("a dynamic block has no end-of-block code"));
    }
    Ok((
        Decoder::new(&lengths[..hlit])?,
        Decoder::new(&lengths[hlit..])?,
    ))
}

fn inflate_block(
    reader: &mut BitReader,
    literals: &Decoder,
    distances: &Decoder,
    out: &mut Vec<u8>,
    max_output: usize,
) -> Result<()> {
    loop {
        let symbol = literals.decode(reader)?;
        if symbol < 256 {
            if out.len() >= max_output {
                return Err(corrupt("more data than the image needs"));
            }
            out.push(symbol as u8);
            continue;
        }
        if symbol == END_OF_BLOCK {
            return Ok(());
        }
        let index = symbol - 257;
        if index >= 29 {
            return Err(corrupt("an invalid length code"));
        }
        let length =
            LENGTH_BASE[index] as usize + reader.bits(LENGTH_EXTRA[index] as u32)? as usize;
        let d = distances.decode(reader)?;
        if d >= 30 {
            return Err(corrupt("an invalid distance code"));
        }
        let distance = DIST_BASE[d] as usize + reader.bits(DIST_EXTRA[d] as u32)? as usize;
        if distance > out.len() {
            return Err(corrupt("a distance before the start of the data"));
        }
        if out.len() + length > max_output {
            return Err(corrupt("more data than the image needs"));
        }
        let start = out.len() - distance;
        for k in 0..length {
            out.push(out[start + k]);
        }
    }
}
