//! Borrowed MNG metadata and bounded embedded PNG pixel decoding; no animation player.
//! Full raw chunks remain available, including unknown critical chunks.
use std::fmt;
mod pixels;
pub use pixels::{decode_png, EmbeddedImage, PixelError, RgbaImage};

#[derive(Debug, PartialEq, Eq)]
pub struct Error {
    pub offset: usize,
    pub reason: &'static str,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "MNG offset 0x{:X}: {}", self.offset, self.reason)
    }
}
impl std::error::Error for Error {}
type Result<T> = std::result::Result<T, Error>;
fn error(offset: usize, reason: &'static str) -> Error {
    Error { offset, reason }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Frame<'a> {
    pub mode: u8,
    pub name: &'a [u8],
    pub change_flags: Option<[u8; 4]>,
    pub delay: Option<u32>,
    pub timeout: Option<u32>,
    pub clipping: Option<(u8, [i32; 4])>,
    /// None = unchanged; Some(empty) = explicitly empty sync list.
    pub sync_ids: Option<Vec<u32>>,
}
#[derive(Debug, PartialEq, Eq)]
pub struct Definition {
    pub object_id: u16,
    pub do_not_show: Option<u8>,
    pub concrete: Option<u8>,
    pub position: Option<[i32; 2]>,
    pub clipping: Option<[i32; 4]>,
}
#[derive(Debug, PartialEq, Eq)]
pub enum Metadata<'a> {
    Header([u32; 7]),
    Frame(Frame<'a>),
    Definition(Definition),
    Termination {
        action: u8,
        repeat: Option<(u8, u32, u32)>,
    },
    Text {
        keyword: &'a [u8],
        text: &'a [u8],
    },
    ImageHeader {
        width: u32,
        height: u32,
        bit_depth: u8,
        color_type: u8,
        compression: u8,
        filter: u8,
        interlace: u8,
    },
    ImageData,
    ImageEnd,
    End,
    Unknown {
        critical: bool,
    },
}
#[derive(Debug)]
pub struct Chunk<'a> {
    pub offset: usize,
    pub kind: [u8; 4],
    pub data: &'a [u8],
    pub crc: u32,
    pub metadata: Metadata<'a>,
}
#[derive(Debug)]
pub struct Mng<'a> {
    pub header: [u32; 7],
    pub chunks: Vec<Chunk<'a>>,
}

struct Fields<'a> {
    data: &'a [u8],
    pos: usize,
    base: usize,
}
impl<'a> Fields<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        if n > self.data.len() - self.pos {
            return Err(error(self.base + self.pos, "truncated metadata fields"));
        }
        let data = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(data)
    }
    fn byte(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn word(&mut self) -> Result<u32> {
        Ok(u32::from_be_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn signed<const N: usize>(&mut self) -> Result<[i32; N]> {
        let mut result = [0; N];
        for value in &mut result {
            *value = self.word()? as i32;
        }
        Ok(result)
    }
}

fn metadata<'a>(kind: &[u8; 4], data: &'a [u8], base: usize) -> Result<Metadata<'a>> {
    let mut r = Fields { data, pos: 0, base };
    let bad = || error(base, "invalid metadata length or value");
    Ok(match kind {
        b"MHDR" => {
            if data.len() != 28 {
                return Err(bad());
            }
            let mut h = [0; 7];
            for v in &mut h {
                *v = r.word()?;
            }
            if h[0] == 0 || h[1] == 0 || h.iter().any(|v| *v > 0x7fff_ffff) {
                return Err(bad());
            }
            Metadata::Header(h)
        }
        b"FRAM" => {
            let mode = if data.is_empty() { 0 } else { r.byte()? };
            if mode > 4 {
                return Err(bad());
            }
            let tail = &data[r.pos..];
            let name_end = tail.iter().position(|b| *b == 0);
            let name = r.take(name_end.unwrap_or(tail.len()))?;
            let mut f = Frame {
                mode,
                name,
                change_flags: None,
                delay: None,
                timeout: None,
                clipping: None,
                sync_ids: None,
            };
            if name_end.is_some() {
                r.byte()?;
                let flags: [u8; 4] = r.take(4)?.try_into().unwrap();
                if flags[0] > 2 || flags[1] > 8 || flags[2] > 2 || flags[3] > 2 {
                    return Err(bad());
                }
                f.change_flags = Some(flags);
                if flags[0] != 0 {
                    f.delay = Some(r.word()?);
                }
                if flags[1] != 0 {
                    f.timeout = Some(r.word()?);
                }
                if flags[2] != 0 {
                    let delta = r.byte()?;
                    if delta > 1 {
                        return Err(bad());
                    }
                    f.clipping = Some((delta, r.signed()?));
                }
                if flags[3] != 0 {
                    let mut ids = Vec::new();
                    while r.pos < data.len() {
                        ids.push(r.word()?);
                    }
                    if ids.iter().any(|v| *v > 0x7fff_ffff) {
                        return Err(bad());
                    }
                    f.sync_ids = Some(ids);
                }
                if r.pos != data.len()
                    || f.delay.is_some_and(|v| v > 0x7fff_ffff)
                    || f.timeout.is_some_and(|v| v > 0x7fff_ffff)
                {
                    return Err(bad());
                }
            }
            Metadata::Frame(f)
        }
        b"DEFI" => {
            if ![2, 3, 4, 12, 28].contains(&data.len()) {
                return Err(bad());
            }
            let object_id = u16::from_be_bytes(r.take(2)?.try_into().unwrap());
            let do_not_show = if data.len() >= 3 {
                Some(r.byte()?)
            } else {
                None
            };
            let concrete = if data.len() >= 4 {
                Some(r.byte()?)
            } else {
                None
            };
            if do_not_show.is_some_and(|v| v > 1) || concrete.is_some_and(|v| v > 1) {
                return Err(bad());
            }
            let position = if data.len() >= 12 {
                Some(r.signed()?)
            } else {
                None
            };
            let clipping = if data.len() == 28 {
                Some(r.signed()?)
            } else {
                None
            };
            Metadata::Definition(Definition {
                object_id,
                do_not_show,
                concrete,
                position,
                clipping,
            })
        }
        b"TERM" => {
            if ![1, 10].contains(&data.len()) {
                return Err(bad());
            }
            let action = r.byte()?;
            if action > 3 || (action == 3) != (data.len() == 10) {
                return Err(bad());
            }
            let repeat = if data.len() == 10 {
                let after = r.byte()?;
                let delay = r.word()?;
                let count = r.word()?;
                if after > 2 || delay > 0x7fff_ffff || count > 0x7fff_ffff {
                    return Err(bad());
                }
                Some((after, delay, count))
            } else {
                None
            };
            Metadata::Termination { action, repeat }
        }
        b"tEXt" => {
            let separator = data.iter().position(|b| *b == 0).ok_or_else(bad)?;
            if separator == 0 || separator > 79 {
                return Err(bad());
            }
            Metadata::Text {
                keyword: &data[..separator],
                text: &data[separator + 1..],
            }
        }
        b"IHDR" => {
            if data.len() != 13 {
                return Err(bad());
            }
            let width = r.word()?;
            let height = r.word()?;
            if width == 0 || height == 0 || width > 0x7fff_ffff || height > 0x7fff_ffff {
                return Err(bad());
            }
            Metadata::ImageHeader {
                width,
                height,
                bit_depth: r.byte()?,
                color_type: r.byte()?,
                compression: r.byte()?,
                filter: r.byte()?,
                interlace: r.byte()?,
            }
        }
        b"IDAT" => Metadata::ImageData,
        b"IEND" | b"MEND" => {
            if !data.is_empty() {
                return Err(bad());
            }
            if kind == b"IEND" {
                Metadata::ImageEnd
            } else {
                Metadata::End
            }
        }
        _ => Metadata::Unknown {
            critical: kind[0].is_ascii_uppercase(),
        },
    })
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xEDB88320 & 0u32.wrapping_sub(crc & 1));
        }
    }
    !crc
}
impl<'a> Mng<'a> {
    /// Checks framing, CRC, supported metadata lengths and basic image boundaries.
    /// Not a full MNG/PNG semantic validator; no pixel data is decompressed.
    pub fn parse(data: &'a [u8], max_chunks: usize) -> Result<Self> {
        if !data.starts_with(b"\x8aMNG\r\n\x1a\n") {
            return Err(error(0, "invalid signature"));
        }
        let mut chunks = Vec::new();
        let mut pos = 8;
        let mut image_open = false;
        let mut header = None;
        while pos < data.len() {
            if chunks.len() >= max_chunks {
                return Err(error(pos, "chunk budget exceeded"));
            }
            if data.len() - pos < 12 {
                return Err(error(pos, "truncated chunk"));
            }
            let length = u32::from_be_bytes(data[pos..pos + 4].try_into().unwrap()) as usize;
            if length > 0x7fff_ffff || length > data.len() - pos - 12 {
                return Err(error(pos, "invalid chunk length"));
            }
            let end = pos + length + 12;
            let kind: [u8; 4] = data[pos + 4..pos + 8].try_into().unwrap();
            if !kind.iter().all(u8::is_ascii_alphabetic) {
                return Err(error(pos + 4, "invalid chunk type"));
            }
            let crc = u32::from_be_bytes(data[end - 4..end].try_into().unwrap());
            if crc32(&data[pos + 4..end - 4]) != crc {
                return Err(error(pos, "CRC mismatch"));
            }
            if header.is_none() && kind != *b"MHDR" {
                return Err(error(pos, "MHDR must be first"));
            }
            let metadata = metadata(&kind, &data[pos + 8..end - 4], pos + 8)?;
            match metadata {
                Metadata::Header(h) => {
                    if header.replace(h).is_some() {
                        return Err(error(pos, "duplicate MHDR"));
                    }
                }
                Metadata::ImageHeader { .. } => {
                    if image_open {
                        return Err(error(pos, "nested image"));
                    }
                    image_open = true;
                }
                Metadata::ImageData if !image_open => return Err(error(pos, "IDAT outside image")),
                Metadata::ImageEnd => {
                    if !image_open {
                        return Err(error(pos, "IEND outside image"));
                    }
                    image_open = false;
                }
                Metadata::End => {
                    if image_open || end != data.len() {
                        return Err(error(pos, "unfinished image or bytes after MEND"));
                    }
                }
                _ => {
                    // Preserve unsupported JNG/BASI/delta-image streams as unknown metadata.
                    if [*b"JHDR", *b"BASI", *b"DHDR"].contains(&kind) {
                        if image_open {
                            return Err(error(pos, "nested image"));
                        }
                        image_open = true;
                    }
                }
            }
            chunks.push(Chunk {
                offset: pos,
                kind,
                data: &data[pos + 8..end - 4],
                crc,
                metadata,
            });
            pos = end;
        }
        if !chunks.last().is_some_and(|c| c.kind == *b"MEND") {
            return Err(error(pos, "missing MEND"));
        }
        Ok(Self {
            header: header.unwrap(),
            chunks,
        })
    }
}

#[cfg(test)]
mod tests;
