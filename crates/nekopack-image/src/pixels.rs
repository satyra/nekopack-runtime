use crate::{Metadata, Mng};
use std::fmt;
use std::io::{Cursor, Write};

#[derive(Debug)]
pub enum PixelError {
    Unsupported(&'static str),
    Budget,
    ImageOrdinal(usize),
    Decode(png::DecodingError),
    Encode(png::EncodingError),
}
impl fmt::Display for PixelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported(s) => write!(f, "unsupported pixel feature: {s}"),
            Self::Budget => write!(f, "pixel allocation budget exceeded"),
            Self::ImageOrdinal(n) => write!(f, "embedded image ordinal {n} out of range"),
            Self::Decode(e) => write!(f, "PNG decode: {e}"),
            Self::Encode(e) => write!(f, "PNG encode: {e}"),
        }
    }
}
impl std::error::Error for PixelError {}
impl From<png::DecodingError> for PixelError {
    fn from(e: png::DecodingError) -> Self {
        Self::Decode(e)
    }
}
impl From<png::EncodingError> for PixelError {
    fn from(e: png::EncodingError) -> Self {
        Self::Encode(e)
    }
}

/// Row-major, straight/unpremultiplied RGBA8 samples. No gamma/ICC conversion.
/// Pixels remain at their embedded image dimensions, not the MNG canvas size.
#[derive(Debug, PartialEq, Eq)]
pub struct RgbaImage {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) pixels: Vec<u8>,
}
impl RgbaImage {
    pub fn from_rgba(
        width: u32,
        height: u32,
        pixels: Vec<u8>,
        budget: usize,
    ) -> Result<Self, PixelError> {
        let expected = u64::from(width)
            .checked_mul(u64::from(height))
            .and_then(|n| n.checked_mul(4))
            .ok_or(PixelError::Budget)?;
        if width == 0 || height == 0 || expected > budget as u64 {
            return Err(PixelError::Budget);
        }
        if pixels.len() as u64 != expected {
            return Err(PixelError::Unsupported("RGBA buffer length"));
        }
        Ok(Self {
            width,
            height,
            pixels,
        })
    }
    pub fn width(&self) -> u32 {
        self.width
    }
    pub fn height(&self) -> u32 {
        self.height
    }
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }
    /// Export decoded pixels only, not a replacement for the original MNG metadata.
    pub fn write_png(&self, output: impl Write) -> Result<(), PixelError> {
        let mut encoder = png::Encoder::new(output, self.width, self.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header()?;
        writer.write_image_data(&self.pixels)?;
        writer.finish()?;
        Ok(())
    }
}

pub fn decode_png(data: &[u8], max_rgba_bytes: usize) -> Result<RgbaImage, PixelError> {
    let mut decoder = png::Decoder::new(Cursor::new(data));
    decoder.set_limits(png::Limits {
        bytes: max_rgba_bytes,
    });
    decoder.set_transformations(png::Transformations::EXPAND);
    decoder.set_ignore_text_chunk(true);
    decoder.set_ignore_iccp_chunk(true);
    let mut reader = decoder.read_info()?;
    let info = reader.info();
    if info.animation_control.is_some() {
        return Err(PixelError::Unsupported("APNG"));
    }
    if info.bit_depth == png::BitDepth::Sixteen {
        return Err(PixelError::Unsupported(
            "16-bit samples; no lossy truncation",
        ));
    }
    let width = info.width;
    let height = info.height;
    let rgba_len = u64::from(width)
        .checked_mul(u64::from(height))
        .and_then(|n| n.checked_mul(4))
        .ok_or(PixelError::Budget)?;
    if rgba_len > max_rgba_bytes as u64 {
        return Err(PixelError::Budget);
    }
    let length = reader.output_buffer_size().ok_or(PixelError::Budget)?;
    if length > max_rgba_bytes {
        return Err(PixelError::Budget);
    }
    let mut decoded = vec![0; length];
    let frame = reader.next_frame(&mut decoded)?;
    reader.finish()?; // includes the trailing chunks/IEND and their checks
    decoded.truncate(frame.buffer_size());
    let pixels = match frame.color_type {
        png::ColorType::Rgba => decoded,
        color => {
            let mut output = Vec::with_capacity(rgba_len as usize);
            match color {
                png::ColorType::Rgb => {
                    for p in decoded.as_chunks::<3>().0 {
                        output.extend([p[0], p[1], p[2], 255]);
                    }
                }
                png::ColorType::Grayscale => {
                    for v in decoded {
                        output.extend([v, v, v, 255]);
                    }
                }
                png::ColorType::GrayscaleAlpha => {
                    for p in decoded.as_chunks::<2>().0 {
                        output.extend([p[0], p[0], p[0], p[1]]);
                    }
                }
                _ => return Err(PixelError::Unsupported("unexpanded PNG color type")),
            }
            output
        }
    };
    Ok(RgbaImage {
        width,
        height,
        pixels,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmbeddedImage {
    pub ordinal: usize,
    pub first_chunk: usize,
    pub last_chunk: usize,
    /// Most recent preceding control chunk, NOT an evaluated animation state.
    pub preceding_fram: Option<usize>,
    pub preceding_defi: Option<usize>,
}
impl<'a> Mng<'a> {
    /// Validate the independent-PNG subset before exposing any images.
    /// Unsupported global pixel dependencies must never be silently discarded.
    pub fn embedded_images(&self) -> Result<Vec<EmbeddedImage>, PixelError> {
        let mut images = Vec::new();
        let mut start = None;
        let mut fram = None;
        let mut defi = None;
        for (i, c) in self.chunks.iter().enumerate() {
            match &c.metadata {
                Metadata::ImageHeader { filter, .. } => {
                    if *filter != 0 {
                        return Err(PixelError::Unsupported("MNG intrapixel filter"));
                    }
                    start = Some(i);
                }
                Metadata::ImageEnd => {
                    let first_chunk = start
                        .take()
                        .ok_or(PixelError::Unsupported("non-PNG embedded image"))?;
                    images.push(EmbeddedImage {
                        ordinal: images.len(),
                        first_chunk,
                        last_chunk: i,
                        preceding_fram: fram,
                        preceding_defi: defi,
                    });
                }
                Metadata::Frame(_) => {
                    if start.is_some() {
                        return Err(PixelError::Unsupported("FRAM inside PNG"));
                    }
                    fram = Some(i);
                }
                Metadata::Definition(_) => {
                    if start.is_some() {
                        return Err(PixelError::Unsupported("DEFI inside PNG"));
                    }
                    defi = Some(i);
                }
                _ => {
                    if [*b"JHDR", *b"BASI", *b"DHDR"].contains(&c.kind) {
                        return Err(PixelError::Unsupported("JNG/BASI/delta-PNG"));
                    }
                    if [*b"PLTE", *b"tRNS"].contains(&c.kind)
                        && (start.is_none() || c.data.is_empty())
                    {
                        return Err(PixelError::Unsupported(
                            "global/inherited palette or transparency",
                        ));
                    }
                    if let Metadata::Unknown { critical: true } = c.metadata {
                        if !(start.is_some() && c.kind == *b"PLTE") {
                            return Err(PixelError::Unsupported("unknown critical MNG/PNG chunk"));
                        }
                    }
                }
            }
        }
        Ok(images)
    }

    /// Decode one embedded image. FRAM/DEFI/tEXt stay in the original Mng object.
    pub fn decode_image(
        &self,
        ordinal: usize,
        max_rgba_bytes: usize,
    ) -> Result<RgbaImage, PixelError> {
        let images = self.embedded_images()?;
        let image = images
            .get(ordinal)
            .ok_or(PixelError::ImageOrdinal(ordinal))?;
        let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
        for c in &self.chunks[image.first_chunk..=image.last_chunk] {
            png.extend((c.data.len() as u32).to_be_bytes());
            png.extend(c.kind);
            png.extend(c.data);
            png.extend(c.crc.to_be_bytes());
        }
        decode_png(&png, max_rgba_bytes)
    }
}
