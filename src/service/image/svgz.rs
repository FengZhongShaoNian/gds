//! Bounded gzip/DEFLATE decompression (`.svgz`) and table-driven IEEE CRC-32 helpers.
//!
//! `usvg` delegates `.svgz` decompression to an unbounded in-memory gunzip call; pre-decompressing
//! with [`decompress_svgz_bounded`] caps the decoded XML size so malicious compressed SVGs fail
//! deterministically with [`Error::TooLarge`] instead of exhausting host memory.

use super::Error;

const CRC32_TABLE: [u32; 256] = {
    let mut table = [0u32; 256];
    let mut index = 0usize;
    while index < 256 {
        let mut crc = index as u32;
        let mut bit = 0;
        while bit < 8 {
            crc = if (crc & 1) != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
            bit += 1;
        }
        table[index] = crc;
        index += 1;
    }
    table
};

/// Computes the IEEE 802.3 CRC-32 checksum over `bytes`.
pub(crate) fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in bytes {
        let idx = ((crc ^ u32::from(byte)) & 0xFF) as usize;
        crc = (crc >> 8) ^ CRC32_TABLE[idx];
    }
    !crc
}

/// Returns `true` when `bytes` begins with the RFC 1952 gzip magic header (`1F 8B`).
pub(crate) fn is_gzip_magic(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0x1F, 0x8B])
}

/// Decompresses a gzip-wrapped `.svgz` byte stream, enforcing a strict output cap of `max_bytes`.
///
/// # Errors
/// Returns [`Error::TooLarge`] when the uncompressed XML exceeds `max_bytes`, or an SVG parse error
/// when the gzip container or DEFLATE stream is malformed.
pub(crate) fn decompress_svgz_bounded(data: &[u8], max_bytes: usize) -> Result<Vec<u8>, Error> {
    let mut output = Vec::new();
    let mut offset = 0usize;

    loop {
        let member_start = offset;
        let payload_start = parse_gzip_header(&data[member_start..])?;
        let mut reader = BitReader::new(&data[member_start + payload_start..]);
        let member_out_start = output.len();
        inflate_stream(&mut reader, &mut output, member_out_start, max_bytes)?;
        let deflated_end = member_start + payload_start + reader.finish_byte_offset();

        let trailer = data
            .get(deflated_end..deflated_end + 8)
            .ok_or_else(malformed_gzip)?;
        let expected_crc = u32::from_le_bytes([trailer[0], trailer[1], trailer[2], trailer[3]]);
        let expected_isize = u32::from_le_bytes([trailer[4], trailer[5], trailer[6], trailer[7]]);
        let member_bytes = &output[member_out_start..];
        let actual_crc = crc32(member_bytes);
        let actual_isize = (member_bytes.len() as u64 & 0xFFFF_FFFF) as u32;
        if actual_crc != expected_crc || actual_isize != expected_isize {
            return Err(malformed_gzip());
        }

        offset = deflated_end + 8;
        if offset == data.len() {
            return Ok(output);
        }
        if !is_gzip_magic(&data[offset..]) {
            return Err(malformed_gzip());
        }
    }
}

fn malformed_gzip() -> Error {
    Error::Svg(resvg::usvg::Error::MalformedGZip)
}

fn too_large(actual_bytes: usize, max_bytes: usize) -> Error {
    Error::TooLarge {
        limit_bytes: max_bytes as u64,
        actual_bytes: actual_bytes as u64,
    }
}

fn parse_gzip_header(member: &[u8]) -> Result<usize, Error> {
    if member.len() < 10 || member[0] != 0x1F || member[1] != 0x8B || member[2] != 8 {
        return Err(malformed_gzip());
    }
    let flags = member[3];
    if (flags & 0xE0) != 0 {
        return Err(malformed_gzip());
    }
    let mut pos = 10usize;

    if (flags & 0x04) != 0 {
        let len_bytes = member.get(pos..pos + 2).ok_or_else(malformed_gzip)?;
        let xlen = usize::from(u16::from_le_bytes([len_bytes[0], len_bytes[1]]));
        pos = pos.checked_add(2 + xlen).ok_or_else(malformed_gzip)?;
        if pos > member.len() {
            return Err(malformed_gzip());
        }
    }
    if (flags & 0x08) != 0 {
        pos = skip_c_string(member, pos)?;
    }
    if (flags & 0x10) != 0 {
        pos = skip_c_string(member, pos)?;
    }
    if (flags & 0x02) != 0 {
        let expected = member.get(pos..pos + 2).ok_or_else(malformed_gzip)?;
        let hcrc = u16::from_le_bytes([expected[0], expected[1]]);
        let computed = (crc32(&member[..pos]) & 0xFFFF) as u16;
        if hcrc != computed {
            return Err(malformed_gzip());
        }
        pos += 2;
    }

    Ok(pos)
}

fn skip_c_string(bytes: &[u8], start: usize) -> Result<usize, Error> {
    let rel = bytes
        .get(start..)
        .and_then(|tail| tail.iter().position(|&byte| byte == 0))
        .ok_or_else(malformed_gzip)?;
    Ok(start + rel + 1)
}

struct BitReader<'a> {
    data: &'a [u8],
    byte_pos: usize,
    bit_buf: u64,
    bit_count: u8,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            byte_pos: 0,
            bit_buf: 0,
            bit_count: 0,
        }
    }

    fn read_bits(&mut self, count: u8) -> Result<u32, Error> {
        while self.bit_count < count {
            let Some(&next) = self.data.get(self.byte_pos) else {
                return Err(malformed_gzip());
            };
            self.byte_pos += 1;
            self.bit_buf |= u64::from(next) << self.bit_count;
            self.bit_count += 8;
        }
        let mask = (1u64 << count) - 1;
        let value = (self.bit_buf & mask) as u32;
        self.bit_buf >>= count;
        self.bit_count -= count;
        Ok(value)
    }

    fn align_to_byte(&mut self) {
        let discard = self.bit_count % 8;
        self.bit_buf >>= discard;
        self.bit_count -= discard;
        let buffered_bytes = usize::from(self.bit_count / 8);
        self.byte_pos -= buffered_bytes;
        self.bit_buf = 0;
        self.bit_count = 0;
    }

    fn read_aligned_slice(&mut self, len: usize) -> Result<&'a [u8], Error> {
        self.align_to_byte();
        let end = self.byte_pos.checked_add(len).ok_or_else(malformed_gzip)?;
        let slice = self
            .data
            .get(self.byte_pos..end)
            .ok_or_else(malformed_gzip)?;
        self.byte_pos = end;
        Ok(slice)
    }

    fn finish_byte_offset(&mut self) -> usize {
        self.align_to_byte();
        self.byte_pos
    }
}

const MAX_HUFF_BITS: usize = 15;
const MAX_HUFF_SYMBOLS: usize = 288;

struct HuffmanTable {
    counts: [u16; MAX_HUFF_BITS + 1],
    symbols: [u16; MAX_HUFF_SYMBOLS],
    symbol_count: u16,
}

impl HuffmanTable {
    fn from_lengths(lengths: &[u8]) -> Result<Self, Error> {
        if lengths.len() > MAX_HUFF_SYMBOLS {
            return Err(malformed_gzip());
        }
        let mut counts = [0u16; MAX_HUFF_BITS + 1];
        for &len in lengths {
            let idx = usize::from(len);
            if idx > MAX_HUFF_BITS {
                return Err(malformed_gzip());
            }
            if idx > 0 {
                counts[idx] += 1;
            }
        }

        let mut left = 1i32;
        for &count in &counts[1..] {
            left = (left << 1) - i32::from(count);
            if left < 0 {
                return Err(malformed_gzip());
            }
        }

        let mut offsets = [0u16; MAX_HUFF_BITS + 1];
        let mut total = 0u16;
        for bit_len in 1..MAX_HUFF_BITS {
            total += counts[bit_len];
            offsets[bit_len + 1] = total;
        }
        let symbol_count = total + counts[MAX_HUFF_BITS];
        if usize::from(symbol_count) > MAX_HUFF_SYMBOLS {
            return Err(malformed_gzip());
        }
        let mut symbols = [0u16; MAX_HUFF_SYMBOLS];

        for (symbol, &len) in lengths.iter().enumerate() {
            if len > 0 {
                let slot = &mut offsets[usize::from(len)];
                symbols[usize::from(*slot)] = symbol as u16;
                *slot += 1;
            }
        }

        Ok(Self {
            counts,
            symbols,
            symbol_count,
        })
    }

    fn decode(&self, reader: &mut BitReader<'_>) -> Result<u16, Error> {
        let mut code = 0i32;
        let mut first = 0i32;
        let mut index = 0usize;
        for &count in &self.counts[1..] {
            code |= reader.read_bits(1)? as i32;
            let count_i32 = i32::from(count);
            if code - count_i32 < first {
                let pos = index + (code - first) as usize;
                if pos >= usize::from(self.symbol_count) {
                    return Err(malformed_gzip());
                }
                return Ok(self.symbols[pos]);
            }
            index += usize::from(count);
            first = (first + count_i32) << 1;
            code <<= 1;
        }
        Err(malformed_gzip())
    }
}

fn fixed_tables() -> Result<(HuffmanTable, HuffmanTable), Error> {
    let mut lit_lens = [8u8; 288];
    lit_lens[144..256].fill(9);
    lit_lens[256..280].fill(7);
    let dist_lens = [5u8; 32];
    Ok((
        HuffmanTable::from_lengths(&lit_lens)?,
        HuffmanTable::from_lengths(&dist_lens)?,
    ))
}

fn read_dynamic_tables(reader: &mut BitReader<'_>) -> Result<(HuffmanTable, HuffmanTable), Error> {
    const CODELEN_ORDER: [usize; 19] = [
        16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
    ];

    let hlit = reader.read_bits(5)? as usize + 257;
    let hdist = reader.read_bits(5)? as usize + 1;
    let hclen = reader.read_bits(4)? as usize + 4;
    if hlit > 286 || hdist > 32 {
        return Err(malformed_gzip());
    }

    let mut codelen_lens = [0u8; 19];
    for &idx in CODELEN_ORDER.iter().take(hclen) {
        codelen_lens[idx] = reader.read_bits(3)? as u8;
    }
    let codelen_table = HuffmanTable::from_lengths(&codelen_lens)?;

    let total = hlit + hdist;
    let mut lengths = [0u8; 318];
    let mut len = 0usize;
    while len < total {
        let sym = codelen_table.decode(reader)?;
        match sym {
            0..=15 => {
                lengths[len] = sym as u8;
                len += 1;
            }
            16 => {
                if len == 0 {
                    return Err(malformed_gzip());
                }
                let prev = lengths[len - 1];
                let repeat = reader.read_bits(2)? as usize + 3;
                if len + repeat > total {
                    return Err(malformed_gzip());
                }
                lengths[len..len + repeat].fill(prev);
                len += repeat;
            }
            17 => {
                let repeat = reader.read_bits(3)? as usize + 3;
                if len + repeat > total {
                    return Err(malformed_gzip());
                }
                lengths[len..len + repeat].fill(0);
                len += repeat;
            }
            18 => {
                let repeat = reader.read_bits(7)? as usize + 11;
                if len + repeat > total {
                    return Err(malformed_gzip());
                }
                lengths[len..len + repeat].fill(0);
                len += repeat;
            }
            _ => return Err(malformed_gzip()),
        }
    }

    if lengths[256] == 0 {
        return Err(malformed_gzip());
    }
    Ok((
        HuffmanTable::from_lengths(&lengths[..hlit])?,
        HuffmanTable::from_lengths(&lengths[hlit..total])?,
    ))
}

fn inflate_stream(
    reader: &mut BitReader<'_>,
    output: &mut Vec<u8>,
    member_out_start: usize,
    max_bytes: usize,
) -> Result<(), Error> {
    loop {
        let bfinal = reader.read_bits(1)?;
        let btype = reader.read_bits(2)?;
        match btype {
            0 => inflate_stored_block(reader, output, max_bytes)?,
            1 => {
                let (lit_table, dist_table) = fixed_tables()?;
                inflate_huffman_block(
                    reader,
                    &lit_table,
                    &dist_table,
                    output,
                    member_out_start,
                    max_bytes,
                )?;
            }
            2 => {
                let (lit_table, dist_table) = read_dynamic_tables(reader)?;
                inflate_huffman_block(
                    reader,
                    &lit_table,
                    &dist_table,
                    output,
                    member_out_start,
                    max_bytes,
                )?;
            }
            _ => return Err(malformed_gzip()),
        }
        if bfinal == 1 {
            return Ok(());
        }
    }
}

fn inflate_stored_block(
    reader: &mut BitReader<'_>,
    output: &mut Vec<u8>,
    max_bytes: usize,
) -> Result<(), Error> {
    let header = reader.read_aligned_slice(4)?;
    let len = u16::from_le_bytes([header[0], header[1]]);
    let nlen = u16::from_le_bytes([header[2], header[3]]);
    if len ^ 0xFFFF != nlen {
        return Err(malformed_gzip());
    }
    let len_usize = usize::from(len);
    let next_total = output.len().saturating_add(len_usize);
    if next_total > max_bytes {
        return Err(too_large(next_total, max_bytes));
    }
    let payload = reader.read_aligned_slice(len_usize)?;
    output.extend_from_slice(payload);
    Ok(())
}

fn inflate_huffman_block(
    reader: &mut BitReader<'_>,
    lit_table: &HuffmanTable,
    dist_table: &HuffmanTable,
    output: &mut Vec<u8>,
    member_out_start: usize,
    max_bytes: usize,
) -> Result<(), Error> {
    const LENGTH_BASE: [u16; 29] = [
        3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115,
        131, 163, 195, 227, 258,
    ];
    const LENGTH_EXTRA: [u8; 29] = [
        0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
    ];
    const DIST_BASE: [u16; 30] = [
        1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
        2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
    ];
    const DIST_EXTRA: [u8; 30] = [
        0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12,
        13, 13,
    ];

    loop {
        let sym = lit_table.decode(reader)?;
        match sym {
            0..=255 => {
                if output.len() >= max_bytes {
                    return Err(too_large(output.len() + 1, max_bytes));
                }
                output.push(sym as u8);
            }
            256 => return Ok(()),
            257..=285 => {
                let len_idx = usize::from(sym - 257);
                let match_len = usize::from(LENGTH_BASE[len_idx])
                    + reader.read_bits(LENGTH_EXTRA[len_idx])? as usize;
                let dist_sym = dist_table.decode(reader)? as usize;
                if dist_sym >= 30 {
                    return Err(malformed_gzip());
                }
                let distance = usize::from(DIST_BASE[dist_sym])
                    + reader.read_bits(DIST_EXTRA[dist_sym])? as usize;
                let member_len = output.len().saturating_sub(member_out_start);
                if distance == 0 || distance > member_len {
                    return Err(malformed_gzip());
                }
                let next_total = output.len().saturating_add(match_len);
                if next_total > max_bytes {
                    return Err(too_large(next_total, max_bytes));
                }
                for _ in 0..match_len {
                    let byte = output[output.len() - distance];
                    output.push(byte);
                }
            }
            _ => return Err(malformed_gzip()),
        }
    }
}
