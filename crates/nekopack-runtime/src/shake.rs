//! BRAVE shake options and the shared TD logical-pixel displacement formula.
//! Snapshot blits preserve destination pixels outside the clipped rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shake {
    horizontal: i32,
    vertical: i32,
    duration_ms: u32,
}

pub(crate) struct Frames {
    motion: Shake,
    source: nekopack_image::RgbaImage,
    pixels: Vec<u8>,
    elapsed: Option<u64>,
}

impl Frames {
    pub(crate) fn new(
        motion: Shake,
        source: nekopack_image::RgbaImage,
        old: nekopack_image::RgbaImage,
    ) -> Result<Self, String> {
        if (source.width(), source.height()) != (old.width(), old.height()) {
            return Err("shake image dimensions mismatch".into());
        }
        if source.pixels().len() + old.pixels().len() > 64 * 1024 * 1024 {
            return Err("shake retained pixel budget exceeded".into());
        }
        // The current Scene renderer supplies an opaque flattened snapshot.
        // Original translucent row blits use a different alpha kernel.
        if source
            .pixels()
            .as_chunks::<4>()
            .0
            .iter()
            .any(|p| p[3] != 255)
        {
            return Err("unsupported translucent shake snapshot".into());
        }
        Ok(Self {
            motion,
            source,
            pixels: old.pixels().to_vec(),
            elapsed: None,
        })
    }
    pub(crate) fn frame(&mut self, elapsed: u64) -> Result<&[u8], String> {
        if self.elapsed.is_some_and(|last| elapsed < last) {
            return Err("shake frame time moved backwards".into());
        }
        if self.elapsed != Some(elapsed) {
            if let Some([dx, dy]) = self.motion.displacement(elapsed) {
                let (w, h) = (
                    i64::from(self.source.width()),
                    i64::from(self.source.height()),
                );
                let (dx, dy) = (i64::from(dx), i64::from(dy));
                let (left, right) = (dx.max(0), (w + dx).min(w));
                let (top, bottom) = (dy.max(0), (h + dy).min(h));
                if left < right && top < bottom {
                    let bytes = ((right - left) * 4) as usize;
                    for y in top..bottom {
                        let target = ((y * w + left) * 4) as usize;
                        let source = (((y - dy) * w + left - dx) * 4) as usize;
                        self.pixels[target..target + bytes]
                            .copy_from_slice(&self.source.pixels()[source..source + bytes]);
                    }
                }
            }
            self.elapsed = Some(elapsed);
        }
        Ok(&self.pixels)
    }
    pub(crate) fn finished(&self) -> bool {
        self.elapsed
            .is_some_and(|t| t >= u64::from(self.motion.duration_ms()))
    }
    pub(crate) fn dimensions(&self) -> (u32, u32) {
        (self.source.width(), self.source.height())
    }
    pub(crate) fn into_png(self) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let mut png = Vec::new();
        nekopack_image::RgbaImage::from_rgba(
            self.source.width(),
            self.source.height(),
            self.pixels,
            64 * 1024 * 1024,
        )?
        .write_png(&mut png)?;
        Ok(png)
    }
}

impl Shake {
    pub fn parse<'a>(options: impl IntoIterator<Item = &'a str>) -> Result<Self, String> {
        let mut value = Self {
            horizontal: 8,
            vertical: 8,
            duration_ms: 250,
        };
        let mut once = false;
        for option in options {
            match option {
                "once" => once = true,
                "short" => value.duration_ms /= 2,
                "long" => {
                    value.duration_ms = value
                        .duration_ms
                        .checked_mul(2)
                        .filter(|v| *v <= i32::MAX as u32)
                        .ok_or("shake duration overflow")?
                }
                "honly" => value.vertical = 0,
                "vonly" => value.horizontal = 0,
                "big" => {
                    value.horizontal = value
                        .horizontal
                        .checked_mul(2)
                        .ok_or("shake amplitude overflow")?;
                    value.vertical = value
                        .vertical
                        .checked_mul(2)
                        .ok_or("shake amplitude overflow")?;
                }
                "small" => {
                    value.horizontal /= 2;
                    value.vertical /= 2;
                }
                _ => return Err(format!("unsupported shake option {option}")),
            }
        }
        if once {
            value.duration_ms = 50;
        }
        Ok(value)
    }

    pub fn duration_ms(self) -> u32 {
        self.duration_ms
    }

    /// None means the original loop no longer renders a shake frame. It does
    /// not imply a final zero-offset blit or prescribe how edges are filled.
    pub fn displacement(self, elapsed_ms: u64) -> Option<[i32; 2]> {
        if elapsed_ms >= u64::from(self.duration_ms) {
            return None;
        }
        let t = elapsed_ms as f64;
        Some([
            ((t / 7.0).sin() * f64::from(self.horizontal)) as i32,
            ((t / 8.0).sin() * f64::from(self.vertical)) as i32,
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clipped_frames_preserve_edges_and_last_frame_at_expiry() {
        use nekopack_image::RgbaImage;
        let source = RgbaImage::filled(12, 12, [90, 80, 70, 255], 4096).unwrap();
        let old = RgbaImage::filled(12, 12, [1, 2, 3, 255], 4096).unwrap();
        let mut frames = Frames::new(Shake::parse(["small"]).unwrap(), source, old).unwrap();
        let first = frames.frame(7).unwrap().to_vec(); // offset (3,3)
        assert_eq!(&first[..4], &[1, 2, 3, 255]);
        assert_eq!(
            &first[(3 * 12 + 3) * 4..(3 * 12 + 4) * 4],
            &[90, 80, 70, 255]
        );
        assert_eq!(frames.frame(7).unwrap(), first);
        let second = frames.frame(28).unwrap().to_vec(); // offset (-3,-1)
        assert_eq!(&second[..4], &[90, 80, 70, 255]);
        assert_eq!(&second[11 * 4..12 * 4], &[1, 2, 3, 255]);
        assert_eq!(&second[(11 * 12 + 11) * 4..], &[90, 80, 70, 255]);
        assert!(frames.frame(27).is_err());
        assert_eq!(frames.frame(250).unwrap(), second);
        assert!(frames.finished());
        assert_eq!(frames.frame(u64::MAX).unwrap(), second);
    }
    #[test]
    fn snapshot_contract_rejects_transparency_and_size_mismatch() {
        use nekopack_image::RgbaImage;
        let image = |w, alpha| RgbaImage::filled(w, 1, [10, 20, 30, alpha], 64).unwrap();
        assert!(Frames::new(Shake::parse([]).unwrap(), image(1, 128), image(1, 255)).is_err());
        assert!(Frames::new(Shake::parse([]).unwrap(), image(2, 255), image(1, 255)).is_err());
    }
    #[test]
    fn original_phase_and_truncation() {
        let shake = Shake::parse([]).unwrap();
        assert_eq!(shake.duration_ms(), 250);
        for (t, expected) in [
            (0, [0, 0]),
            (7, [6, 6]),
            (14, [7, 7]),
            (28, [-6, -2]),
            (50, [6, 0]),
        ] {
            assert_eq!(shake.displacement(t), Some(expected));
        }
        assert!(shake.displacement(249).is_some());
        assert_eq!(shake.displacement(250), None);
        assert_eq!(shake.displacement(u64::MAX), None);
    }
    #[test]
    fn ordered_options_and_once_override() {
        assert_eq!(Shake::parse(["short", "long"]).unwrap().duration_ms(), 250);
        assert_eq!(
            Shake::parse(["once", "long", "long"])
                .unwrap()
                .duration_ms(),
            50
        );
        assert_eq!(
            Shake::parse(["long", "once", "short"])
                .unwrap()
                .duration_ms(),
            50
        );
        assert_eq!(
            Shake::parse(["honly", "big"]).unwrap().displacement(7),
            Some([13, 0])
        );
        assert_eq!(
            Shake::parse(["vonly", "small"]).unwrap().displacement(7),
            Some([0, 3])
        );
        assert_eq!(
            Shake::parse(["honly", "vonly"]).unwrap().displacement(7),
            Some([0, 0])
        );
        assert_eq!(Shake::parse(["short"; 8]).unwrap().displacement(0), None);
    }
    #[test]
    fn reject_unknown_and_overflow_without_wrapping() {
        assert!(Shake::parse(["100"]).is_err());
        assert!(Shake::parse(["big"; 28]).is_err());
        assert!(Shake::parse(["long"; 24]).is_err());
    }
}
