//! TD flash alternates solid RGB and a scene snapshot, waiting 30 ms per phase.
use nekopack_image::RgbaImage;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Flash {
    rgb: [u8; 3],
    count: u32,
}
impl Flash {
    pub fn parse<'a>(options: impl IntoIterator<Item = &'a str>) -> Result<Self, String> {
        let mut result = Self {
            rgb: [255; 3],
            count: 1,
        };
        for option in options {
            if !option.is_empty() && option.bytes().all(|c| c.is_ascii_digit()) {
                result.count = option.parse().map_err(|_| "flash count overflow")?;
            } else {
                result.rgb = match option {
                    "white" | "白" => [255, 255, 255],
                    "赤" => [255, 0, 0],
                    "青" => [0, 0, 255],
                    "黄" => [255, 255, 0],
                    "緑" => [0, 255, 0],
                    "紫" => [255, 0, 255],
                    "水" => [0, 255, 255],
                    _ => return Err(format!("unsupported flash option {option}")),
                };
            }
        }
        // Bound native presentation work independently of script instruction count.
        if !(1..=128).contains(&result.count) {
            return Err("unsupported flash count; expected 1..=128".into());
        }
        Ok(result)
    }
    pub fn rgb(self) -> [u8; 3] {
        self.rgb
    }
    pub fn count(self) -> u32 {
        self.count
    }
    pub fn duration_ms(self) -> u64 {
        u64::from(self.count) * 60
    }
}
pub(crate) struct Frames {
    spec: Flash,
    source: RgbaImage,
    solid: Vec<u8>,
    next: u64,
}
impl Frames {
    pub(crate) fn new(spec: Flash, source: RgbaImage) -> Result<Self, String> {
        if source.pixels().len() > 32 * 1024 * 1024 {
            return Err("flash retained pixel budget exceeded".into());
        }
        if source
            .pixels()
            .as_chunks::<4>()
            .0
            .iter()
            .any(|p| p[3] != 255)
        {
            return Err("unsupported translucent flash snapshot".into());
        }
        let mut solid = vec![0; source.pixels().len()];
        for pixel in solid.as_chunks_mut::<4>().0 {
            pixel.copy_from_slice(&[spec.rgb[0], spec.rgb[1], spec.rgb[2], 255]);
        }
        Ok(Self {
            spec,
            source,
            solid,
            next: 0,
        })
    }
    pub(crate) fn dimensions(&self) -> (u32, u32) {
        (self.source.width(), self.source.height())
    }
    pub(crate) fn frame(&mut self, phase: u64) -> Result<&[u8], String> {
        if phase >= u64::from(self.spec.count) * 2
            || (phase != self.next && self.next.checked_sub(1) != Some(phase))
        {
            return Err("out-of-order flash phase".into());
        }
        if phase == self.next {
            self.next += 1;
        }
        Ok(if phase.is_multiple_of(2) {
            &self.solid
        } else {
            self.source.pixels()
        })
    }
    pub(crate) fn finished(&self) -> bool {
        self.next == u64::from(self.spec.count) * 2
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn options_are_color_and_count_not_seconds() {
        assert_eq!(
            Flash::parse(["青", "5"]).unwrap(),
            Flash::parse(["5", "青"]).unwrap()
        );
        assert_eq!(Flash::parse(["青", "5"]).unwrap().duration_ms(), 300);
        assert_eq!(Flash::parse([]).unwrap().rgb, [255; 3]);
        for bad in ["0", "129", "-1", "999999999999", "0.5", "blue"] {
            assert!(Flash::parse([bad]).is_err());
        }
    }
    #[test]
    fn every_phase_is_required_and_last_restores_snapshot() {
        let source = RgbaImage::filled(1, 1, [12, 34, 56, 255], 4).unwrap();
        let mut f = Frames::new(Flash::parse(["青", "2"]).unwrap(), source).unwrap();
        assert!(f.frame(1).is_err());
        assert_eq!(f.frame(0).unwrap(), [0, 0, 255, 255]);
        assert_eq!(f.frame(0).unwrap(), [0, 0, 255, 255]);
        assert_eq!(f.frame(1).unwrap(), [12, 34, 56, 255]);
        assert!(f.frame(0).is_err());
        assert!(!f.finished());
        f.frame(2).unwrap();
        assert_eq!(f.frame(3).unwrap(), [12, 34, 56, 255]);
        assert!(f.finished());
        assert!(f.frame(4).is_err());
    }
}
