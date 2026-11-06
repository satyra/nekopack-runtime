//! Read-only NEKOPACK /1 layout. No engine-version or gameplay compatibility claim.
//! Evidence and algorithm attribution: docs/Sources.md (X2, N2, G1).
//! The source must remain unchanged while an Archive is in use.
use std::fmt;
use std::io::{self, Read, Seek, SeekFrom, Write};
mod names;
pub use names::name_hash;

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Invalid { offset: u64, reason: &'static str },
    EntryOutOfRange(usize),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "I/O: {e}"),
            Self::Invalid { offset, reason } => write!(f, "offset 0x{offset:X}: {reason}"),
            Self::EntryOutOfRange(n) => write!(f, "entry ordinal {n} out of range"),
        }
    }
}
impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            _ => None,
        }
    }
}
impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}
type Result<T> = std::result::Result<T, Error>;

fn invalid(offset: u64, reason: &'static str) -> Error {
    Error::Invalid { offset, reason }
}
fn u32le(bytes: &[u8]) -> u32 {
    u32::from_le_bytes(bytes[..4].try_into().expect("caller supplies four bytes"))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Header {
    pub name_seed: u32,
    pub priority: u32,
    pub index_key: u32,
    pub index_length: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry {
    pub directory_hash: u32,
    pub name_hash: u32,
    /// Offset of the 8-byte block header, not the payload.
    pub block_offset: u64,
    pub length: u32,
    pub key: u32,
}

/// The parity depends on seed and length; it is NOT a content checksum.
pub fn parity(seed: u32, length: u32) -> u32 {
    let t = (length ^ (seed ^ length).wrapping_add(0x5D588B65)).wrapping_sub(0x359D3E2A);
    let v = (length ^ (length ^ t).wrapping_sub(0x70E44324)).wrapping_add(0x6C078965);
    v.rotate_left(t >> 27)
}

fn check_key(seed: u32, length: u32, key: u32, offset: u64) -> Result<()> {
    if key != 0 && key != parity(seed, length) {
        return Err(invalid(
            offset,
            "nonzero block key does not match /1 parity",
        ));
    }
    Ok(())
}

/// Feedback is updated per little-endian u16 lane; the final odd byte is valid.
/// Calls can split at arbitrary byte boundaries without resetting the cipher.
struct Decoder {
    lanes: [u16; 4],
    position: usize,
    low_byte: u8,
    encrypted: bool,
}
impl Decoder {
    fn new(h: u32) -> Self {
        let a = h ^ h.wrapping_add(0x5D588B65);
        let b = a ^ h.wrapping_sub(0x359D3E2A);
        let lo = b ^ a.wrapping_sub(0x70E44324);
        let hi = lo ^ b.wrapping_add(0x6C078965);
        Self {
            lanes: [lo as u16, (lo >> 16) as u16, hi as u16, (hi >> 16) as u16],
            position: 0,
            low_byte: 0,
            encrypted: h != 0,
        }
    }
    fn apply(&mut self, data: &mut [u8]) {
        if !self.encrypted {
            return;
        }
        for byte in data {
            let lane = self.position / 2;
            let high = self.position & 1;
            *byte ^= (self.lanes[lane] >> (high * 8)) as u8;
            if high == 0 {
                self.low_byte = *byte;
            } else {
                let plain = u16::from_le_bytes([self.low_byte, *byte]);
                self.lanes[lane] = self.lanes[lane].wrapping_add(plain);
            }
            self.position = (self.position + 1) & 7;
        }
    }
}

pub struct Archive<R> {
    source: R,
    header: Header,
    index: Vec<u8>,
    entries: Vec<Entry>,
    trailing_bytes: u64,
}

impl<R: Read + Seek> Archive<R> {
    /// Reads the index and every block header, but never loads entire payloads.
    /// max_index_bytes is a caller-selected allocation budget, not a format limit.
    pub fn open(mut source: R, max_index_bytes: usize) -> Result<Self> {
        let size = source.seek(SeekFrom::End(0))?;
        if size < 24 {
            return Err(invalid(0, "truncated archive header"));
        }
        source.seek(SeekFrom::Start(0))?;
        let mut raw = [0; 24];
        source.read_exact(&mut raw)?;
        if &raw[..8] != b"NEKOPACK" {
            return Err(invalid(0, "not NEKOPACK"));
        }
        let header = Header {
            name_seed: u32le(&raw[8..]),
            priority: u32le(&raw[12..]),
            index_key: u32le(&raw[16..]),
            index_length: u32le(&raw[20..]),
        };
        let length = u64::from(header.index_length);
        if length < 8 || length % 8 != 0 || length > size - 24 {
            return Err(invalid(20, "invalid /1 index length"));
        }
        if length > max_index_bytes as u64 {
            return Err(invalid(20, "index exceeds caller allocation budget"));
        }
        check_key(header.name_seed, header.index_length, header.index_key, 16)?;
        let mut index = vec![0; length as usize];
        source.read_exact(&mut index)?;
        Decoder::new(header.index_key).apply(&mut index);
        let mut entries = Vec::new();
        let mut pos = 0;
        let mut offset = 24 + length;
        while pos < index.len() {
            // Each record consumes a multiple of 8; index length was checked above.
            let directory_hash = u32le(&index[pos..]);
            let count = u32le(&index[pos + 4..]) as usize;
            pos += 8;
            if count > (index.len() - pos) / 8 {
                return Err(invalid(
                    24 + pos as u64 - 4,
                    "directory count exceeds index",
                ));
            }
            for _ in 0..count {
                let name_hash = u32le(&index[pos..]);
                let length = u32le(&index[pos + 4..]);
                pos += 8;
                // Subtraction bounds-check avoids overflow even for u64 sources.
                if offset > size || size - offset < 8 + u64::from(length) {
                    return Err(invalid(offset, "resource block exceeds source length"));
                }
                source.seek(SeekFrom::Start(offset))?;
                let mut block = [0; 8];
                source.read_exact(&mut block)?;
                let key = u32le(&block);
                if u32le(&block[4..]) != length {
                    return Err(invalid(offset + 4, "index/block length mismatch"));
                }
                check_key(header.name_seed, length, key, offset)?;
                entries.push(Entry {
                    directory_hash,
                    name_hash,
                    block_offset: offset,
                    length,
                    key,
                });
                offset += 8 + u64::from(length);
            }
        }
        Ok(Self {
            source,
            header,
            index,
            entries,
            trailing_bytes: size - offset,
        })
    }

    pub fn header(&self) -> Header {
        self.header
    }
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }
    pub fn index_bytes(&self) -> &[u8] {
        &self.index
    }
    pub fn trailing_bytes(&self) -> u64 {
        self.trailing_bytes
    }

    fn entry(&self, ordinal: usize) -> Result<Entry> {
        self.entries
            .get(ordinal)
            .copied()
            .ok_or(Error::EntryOutOfRange(ordinal))
    }

    /// Decode only the leading bytes; no assumption that a signature identifies a codec.
    pub fn read_prefix(&mut self, ordinal: usize, limit: usize) -> Result<Vec<u8>> {
        let entry = self.entry(ordinal)?;
        let length = limit.min(entry.length as usize);
        self.source.seek(SeekFrom::Start(entry.block_offset + 8))?;
        let mut data = vec![0; length];
        self.source.read_exact(&mut data)?;
        Decoder::new(entry.key).apply(&mut data);
        Ok(data)
    }

    /// Random access by ordinal preserves duplicate hashes without guessing a winner.
    /// The whole resource is streamed using a 64 KiB buffer, starting at its cipher origin.
    pub fn copy_entry(&mut self, ordinal: usize, output: &mut impl Write) -> Result<u64> {
        let entry = self.entry(ordinal)?;
        self.source.seek(SeekFrom::Start(entry.block_offset + 8))?;
        let mut remaining = u64::from(entry.length);
        let mut buffer = [0; 64 * 1024];
        let mut decoder = Decoder::new(entry.key);
        while remaining != 0 {
            let count = remaining.min(buffer.len() as u64) as usize;
            self.source.read_exact(&mut buffer[..count])?;
            decoder.apply(&mut buffer[..count]);
            output.write_all(&buffer[..count])?;
            remaining -= count as u64;
        }
        Ok(u64::from(entry.length))
    }
}

#[cfg(test)]
mod tests;
