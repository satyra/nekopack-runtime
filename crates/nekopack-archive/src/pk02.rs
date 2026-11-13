//! PK02 tree archives. Names are byte strings: the format has no encoding tag.
use crate::{invalid, Result};
use std::collections::{HashMap, HashSet};
use std::io::{BufReader, Read, Seek, SeekFrom};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub path: Vec<u8>,
    pub size: u64,
    pub offset: u64,
    /// 0 is stored; 1 is a raw LZ4 block, bounded by the decoded size.
    pub compression: u8,
}

pub struct Pk02<R> {
    reader: R,
    entries: Vec<Entry>,
    lookup: HashMap<Vec<u8>, usize>,
}

struct Index<'a, R> {
    reader: &'a mut R,
    length: u64,
    remaining: u64,
}
struct Directory {
    children: u64,
    files: u64,
    child_offset: u64,
    file_offset: u64,
    name: Vec<u8>,
    end: u64,
}
impl<R: Read + Seek> Index<'_, R> {
    fn bytes<const N: usize>(&mut self) -> Result<[u8; N]> {
        let pos = self.reader.stream_position()?;
        self.remaining = self
            .remaining
            .checked_sub(N as u64)
            .ok_or_else(|| invalid(pos, "PK02 index exceeds limit"))?;
        let mut b = [0; N];
        self.reader.read_exact(&mut b)?;
        Ok(b)
    }
    fn seek(&mut self, offset: u64) -> Result<()> {
        if offset >= self.length {
            return Err(invalid(offset, "PK02 index offset outside archive"));
        }
        self.reader.seek(SeekFrom::Start(offset))?;
        Ok(())
    }
    fn name(&mut self) -> Result<Vec<u8>> {
        let mut name = Vec::new();
        loop {
            let b = self.bytes::<1>()?[0];
            if b == 0 {
                return Ok(name);
            }
            name.push(b);
        }
    }
    fn directory(&mut self, offset: u64) -> Result<Directory> {
        self.seek(offset)?;
        let h = self.bytes::<32>()?;
        let name = self.name()?;
        Ok(Directory {
            children: q(&h[0..8]),
            files: q(&h[8..16]),
            child_offset: q(&h[16..24]),
            file_offset: q(&h[24..32]),
            name,
            end: self.reader.stream_position()?,
        })
    }
}
fn q(b: &[u8]) -> u64 {
    u64::from_le_bytes(b.try_into().unwrap())
}
fn join(parent: &[u8], name: &[u8]) -> Vec<u8> {
    let mut path = parent.to_vec();
    if !path.is_empty() {
        path.push(b'/');
    }
    path.extend_from_slice(name);
    path
}

impl<R: Read + Seek> Pk02<R> {
    pub fn open(mut reader: R, max_index_bytes: u64) -> Result<Self> {
        let length = reader.seek(SeekFrom::End(0))?;
        reader.seek(SeekFrom::Start(0))?;
        let mut h = [0; 12];
        reader.read_exact(&mut h)?;
        if &h[..4] != b"PK02" {
            return Err(invalid(0, "not a PK02 archive"));
        }
        let root_offset = q(&h[4..]);
        let mut index = Index {
            reader: &mut reader,
            length,
            remaining: max_index_bytes,
        };
        let root = index.directory(root_offset)?;
        let root_path = if root.name == b"." {
            Vec::new()
        } else {
            root.name.clone()
        };
        let mut pending = vec![(root_offset, root, root_path)];
        let mut visited = HashSet::new();
        let mut entries = Vec::new();
        let mut lookup = HashMap::new();
        while let Some((offset, dir, path)) = pending.pop() {
            if !visited.insert(offset) {
                return Err(invalid(offset, "PK02 repeated directory offset"));
            }
            let mut child_offset = dir.child_offset;
            let mut children = Vec::new();
            for _ in 0..dir.children {
                let child = index.directory(child_offset)?;
                let next = child.end;
                let child_path = join(&path, &child.name);
                children.push((child_offset, child, child_path));
                child_offset = next;
            }
            pending.extend(children.into_iter().rev());
            if dir.files > 0 {
                index.seek(dir.file_offset)?;
            }
            for _ in 0..dir.files {
                let at = index.reader.stream_position()?;
                let h = index.bytes::<17>()?;
                let name = index.name()?;
                let entry = Entry {
                    path: join(&path, &name),
                    size: q(&h[..8]),
                    offset: q(&h[8..16]),
                    compression: h[16],
                };
                if entry.compression > 1 {
                    return Err(invalid(at + 16, "unsupported PK02 compression"));
                }
                if entry.offset > length
                    || (entry.size > 0 && entry.offset == length)
                    || (entry.compression == 0 && entry.size > length - entry.offset)
                {
                    return Err(invalid(at, "PK02 payload outside archive"));
                }
                if let Some(&ordinal) = lookup.get(&entry.path) {
                    entries[ordinal] = entry;
                } else {
                    lookup.insert(entry.path.clone(), entries.len());
                    entries.push(entry);
                }
            }
        }
        Ok(Self {
            reader,
            entries,
            lookup,
        })
    }
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }
    /// Match the original path bytes exactly; directory separators are `/`.
    pub fn entry(&self, path: &[u8]) -> Option<&Entry> {
        self.lookup.get(path).map(|&ordinal| &self.entries[ordinal])
    }
    /// Match the original path bytes exactly; directory separators are `/`.
    pub fn read(&mut self, path: &[u8]) -> Result<Option<Vec<u8>>> {
        let Some(entry) = self.entry(path).cloned() else {
            return Ok(None);
        };
        let size = usize::try_from(entry.size)
            .map_err(|_| invalid(entry.offset, "PK02 output size exceeds platform"))?;
        let mut out = Vec::new();
        out.try_reserve_exact(size)
            .map_err(|_| invalid(entry.offset, "PK02 output allocation failed"))?;
        self.reader.seek(SeekFrom::Start(entry.offset))?;
        let mut reader = BufReader::new(&mut self.reader);
        if entry.compression == 0 {
            out.resize(size, 0);
            reader.read_exact(&mut out)?;
        } else {
            while out.len() < size {
                let token = byte(&mut reader)?;
                let literals = run_length(&mut reader, token >> 4, entry.offset)?;
                if literals > size - out.len() {
                    return Err(invalid(entry.offset, "PK02 LZ4 literals exceed output"));
                }
                let start = out.len();
                out.resize(start + literals, 0);
                reader.read_exact(&mut out[start..])?;
                if out.len() == size {
                    break;
                }
                let mut delta = [0; 2];
                reader.read_exact(&mut delta)?;
                let delta = u16::from_le_bytes(delta) as usize;
                if delta == 0 || delta > out.len() {
                    return Err(invalid(entry.offset, "PK02 LZ4 invalid match offset"));
                }
                let count = run_length(&mut reader, token & 15, entry.offset)?
                    .checked_add(4)
                    .ok_or_else(|| invalid(entry.offset, "PK02 LZ4 length overflow"))?;
                if count > size - out.len() {
                    return Err(invalid(entry.offset, "PK02 LZ4 match exceeds output"));
                }
                for _ in 0..count {
                    out.push(out[out.len() - delta]);
                }
            }
        }
        Ok(Some(out))
    }
}
fn byte(reader: &mut impl Read) -> Result<u8> {
    let mut b = [0];
    reader.read_exact(&mut b)?;
    Ok(b[0])
}
fn run_length(reader: &mut impl Read, base: u8, offset: u64) -> Result<usize> {
    let mut n = base as usize;
    if base == 15 {
        loop {
            let b = byte(reader)?;
            n = n
                .checked_add(b as usize)
                .ok_or_else(|| invalid(offset, "PK02 LZ4 length overflow"))?;
            if b != 255 {
                break;
            }
        }
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    fn fixture(compression: u8, payload: &[u8], size: u64) -> Vec<u8> {
        fixture_named(compression, payload, size, b"test.bin")
    }
    fn fixture_named(compression: u8, payload: &[u8], size: u64, name: &[u8]) -> Vec<u8> {
        let mut b = b"PK02".to_vec();
        b.extend_from_slice(&0u64.to_le_bytes());
        let data = b.len() as u64;
        b.extend_from_slice(payload);
        let table = b.len() as u64;
        b.extend_from_slice(&size.to_le_bytes());
        b.extend_from_slice(&data.to_le_bytes());
        b.push(compression);
        b.extend_from_slice(name);
        b.push(0);
        let root = b.len() as u64;
        for v in [0, 1, 0, table] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        b.extend_from_slice(b".\0");
        b[4..12].copy_from_slice(&root.to_le_bytes());
        b
    }
    #[test]
    fn stored_and_missing() {
        let mut p = Pk02::open(Cursor::new(fixture(0, b"hello", 5)), 4096).unwrap();
        assert_eq!(p.entries()[0].path, b"test.bin");
        assert_eq!(p.read(b"test.bin").unwrap().unwrap(), b"hello");
        assert!(p.read(b"TEST.bin").unwrap().is_none());
    }
    #[test]
    fn lz4_overlapping_match_and_final_literals() {
        let mut p =
            Pk02::open(Cursor::new(fixture(1, b"\x15a\x01\x00\x50hello", 15)), 4096).unwrap();
        assert_eq!(p.read(b"test.bin").unwrap().unwrap(), b"aaaaaaaaaahello");
    }
    #[test]
    fn invalid_index_and_payload() {
        assert!(Pk02::open(Cursor::new(fixture(0, b"hello", 5)), 1).is_err());
        assert!(Pk02::open(Cursor::new(fixture(2, b"hello", 5)), 4096).is_err());
        let mut p = Pk02::open(Cursor::new(fixture(1, b"\x10a\x00\x00", 5)), 4096).unwrap();
        assert!(p.read(b"test.bin").is_err());
    }
    #[test]
    fn nested_paths_use_exact_forward_slashes() {
        let mut b = fixture(0, b"hello", 5);
        let child = q(&b[4..12]);
        b.truncate(b.len() - 2);
        b.extend_from_slice(b"script\0");
        let root = b.len() as u64;
        for v in [1, 0, child, 0] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        b.extend_from_slice(b".\0");
        b[4..12].copy_from_slice(&root.to_le_bytes());
        let mut p = Pk02::open(Cursor::new(b), 4096).unwrap();
        assert!(p.entry(b"script\\test.bin").is_none());
        assert!(p.read(b"script\\test.bin").unwrap().is_none());
        assert_eq!(p.entry(b"script/test.bin").unwrap().size, 5);
        assert_eq!(p.read(b"script/test.bin").unwrap().unwrap(), b"hello");
    }
    #[test]
    fn cp932_trail_backslash_is_part_of_filename() {
        // CP932 encodes ソ as 83 5C: 5C must not become a directory separator.
        let name = b"\x83\x5c.bin";
        let mut p = Pk02::open(Cursor::new(fixture_named(0, b"hello", 5, name)), 4096).unwrap();
        assert_eq!(p.entries()[0].path, name);
        assert_eq!(p.entry(name).unwrap().size, 5);
        assert_eq!(p.read(name).unwrap().unwrap(), b"hello");
        assert!(p.entry(b"\x83/.bin").is_none());
        assert!(p.read(b"\x83/.bin").unwrap().is_none());
    }
    #[test]
    fn index_truncation_offsets_and_sizes() {
        let b = fixture(0, b"hello", 5);
        for end in 0..b.len() {
            assert!(Pk02::open(Cursor::new(&b[..end]), 4096).is_err());
        }
        let mut bad = b.clone();
        bad[4..12].copy_from_slice(&u64::MAX.to_le_bytes());
        assert!(Pk02::open(Cursor::new(bad), 4096).is_err());
        let mut bad = b.clone();
        bad[17..25].copy_from_slice(&u64::MAX.to_le_bytes());
        assert!(Pk02::open(Cursor::new(bad), 4096).is_err());
        let mut bad = b;
        bad[25..33].copy_from_slice(&u64::MAX.to_le_bytes());
        assert!(Pk02::open(Cursor::new(bad), 4096).is_err());
    }
    #[test]
    #[ignore = "set NEKOPACK_PK02 to a locally supplied archive; reads bytes without displaying content"]
    fn local_archive() {
        let path = std::env::var_os("NEKOPACK_PK02").expect("set NEKOPACK_PK02");
        let mut p = Pk02::open(std::fs::File::open(path).unwrap(), 16 * 1024 * 1024).unwrap();
        let entries = p.entries().to_vec();
        assert!(!entries.is_empty());
        for e in entries {
            assert_eq!(p.read(&e.path).unwrap().unwrap().len() as u64, e.size);
        }
    }
}
