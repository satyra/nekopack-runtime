//! Explicit sprite composition, not a MNG animation scheduler.
use crate::{Metadata, Mng, PixelError, RgbaImage};

#[derive(Debug, PartialEq, Eq)]
pub struct Sprite<'a> {
    pub ordinal: usize,
    pub name: &'a [u8],
    pub position: [i32; 2],
}

impl Mng<'_> {
    /// The currently observed sprite profile: object zero, visible, abstract,
    /// mode-two FRAM without changed timing/clipping. Unsupported controls fail.
    /// Names are raw bytes and duplicates remain separate sprites.
    pub fn sprites(&self) -> Result<Vec<Sprite<'_>>, PixelError> {
        self.embedded_images()?;
        let mut position = [0, 0];
        let mut name = &b""[..];
        let mut result = Vec::new();
        for chunk in &self.chunks {
            match &chunk.metadata {
                Metadata::Definition(d) => {
                    if d.object_id != 0
                        || d.do_not_show != Some(0)
                        || d.concrete != Some(0)
                        || d.position.is_none()
                        || d.clipping.is_some()
                    {
                        return Err(PixelError::Unsupported("sprite DEFI profile"));
                    }
                    position = d.position.unwrap();
                }
                Metadata::Frame(f) => {
                    if f.mode != 2 || f.change_flags.is_some_and(|flags| flags != [0; 4]) {
                        return Err(PixelError::Unsupported("sprite FRAM profile"));
                    }
                    name = f.name;
                }
                Metadata::ImageHeader { .. } => result.push(Sprite {
                    ordinal: result.len(),
                    name,
                    position,
                }),
                _ => {}
            }
        }
        Ok(result)
    }
}

impl RgbaImage {
    pub fn transparent(width: u32, height: u32, budget: usize) -> Result<Self, PixelError> {
        let len = u64::from(width)
            .checked_mul(u64::from(height))
            .and_then(|v| v.checked_mul(4))
            .ok_or(PixelError::Budget)?;
        if width == 0 || height == 0 || len > budget as u64 {
            return Err(PixelError::Budget);
        }
        Ok(Self {
            width,
            height,
            pixels: vec![0; len as usize],
        })
    }

    /// Source-over in raw RGBA sample space. Clip to the canvas; round each
    /// output channel to nearest integer (ties up). No gamma correction.
    pub fn draw(&mut self, source: &RgbaImage, position: [i32; 2]) {
        let [x, y] = position.map(i64::from);
        let left = x.max(0);
        let top = y.max(0);
        let right = (x + i64::from(source.width)).min(i64::from(self.width));
        let bottom = (y + i64::from(source.height)).min(i64::from(self.height));
        for dy in top..bottom {
            for dx in left..right {
                let si = (((dy - y) as usize * source.width as usize) + (dx - x) as usize) * 4;
                let di = ((dy as usize * self.width as usize) + dx as usize) * 4;
                let s = &source.pixels[si..si + 4];
                let d = &mut self.pixels[di..di + 4];
                let sa = u32::from(s[3]);
                let da = u32::from(d[3]);
                let alpha = sa * 255 + da * (255 - sa);
                if alpha == 0 {
                    continue;
                }
                for channel in 0..3 {
                    let value =
                        u32::from(s[channel]) * sa * 255 + u32::from(d[channel]) * da * (255 - sa);
                    d[channel] = ((value + alpha / 2) / alpha) as u8;
                }
                d[3] = ((alpha + 127) / 255) as u8;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn alpha_and_signed_clipping() {
        let source = RgbaImage {
            width: 2,
            height: 1,
            pixels: vec![255, 0, 0, 128, 0, 255, 0, 255],
        };
        let mut canvas = RgbaImage::transparent(2, 1, 8).unwrap();
        canvas.draw(&source, [0, 0]);
        assert_eq!(canvas.pixels(), source.pixels());
        canvas.draw(&source, [0, 0]);
        assert_eq!(canvas.pixels(), &[255, 0, 0, 192, 0, 255, 0, 255]);
        canvas.draw(&source, [-1, 0]);
        assert_eq!(&canvas.pixels()[..4], &[0, 255, 0, 255]);
        let before = canvas.pixels().to_vec();
        canvas.draw(&source, [i32::MAX, i32::MIN]);
        assert_eq!(canvas.pixels(), before);
        assert!(RgbaImage::transparent(u32::MAX, u32::MAX, usize::MAX).is_err());
        assert!(RgbaImage::transparent(2, 2, 15).is_err());
    }
    #[test]
    fn mixed_translucent_colors() {
        let mut dst = RgbaImage {
            width: 1,
            height: 1,
            pixels: vec![0, 0, 255, 128],
        };
        let src = RgbaImage {
            width: 1,
            height: 1,
            pixels: vec![255, 0, 0, 128],
        };
        dst.draw(&src, [0, 0]);
        assert_eq!(dst.pixels(), &[170, 0, 85, 192]);
    }
}
