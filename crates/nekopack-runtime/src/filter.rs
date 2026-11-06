//! BLESS actor-load filter, from the original saturated BGRA add/subtract path.
use nekopack_image::RgbaImage;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Filter {
    #[default]
    None,
    Night,
}
impl Filter {
    pub fn parse(args: &[Vec<u8>]) -> Result<Self, String> {
        match args {
            [] => Ok(Self::None),
            [name] if name == "夜".as_bytes() => Ok(Self::Night),
            _ => Err("unsupported actor filter arguments".into()),
        }
    }
    pub fn metadata(self) -> &'static str {
        match self {
            Self::None => "null",
            Self::Night => "\"night\"",
        }
    }
    pub fn apply(self, image: &mut RgbaImage) {
        if self == Self::Night {
            // Original packed BGRA: paddusb 0x00000011; psubusb 0x00111600.
            for p in image.pixels_mut().as_chunks_mut::<4>().0 {
                p[0] = p[0].saturating_sub(17);
                p[1] = p[1].saturating_sub(22);
                p[2] = p[2].saturating_add(17);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn night_clamps_every_channel_and_preserves_alpha() {
        let pixels = (0..=255u8).flat_map(|v| [v, v, v, v]).collect();
        let mut image = RgbaImage::from_rgba(256, 1, pixels, 1024).unwrap();
        Filter::Night.apply(&mut image);
        for (v, p) in image.pixels().as_chunks::<4>().0.iter().enumerate() {
            assert_eq!(
                *p,
                [
                    v.saturating_sub(17) as u8,
                    v.saturating_sub(22) as u8,
                    (v + 17).min(255) as u8,
                    v as u8
                ]
            );
        }
        let before = image.pixels().to_vec();
        Filter::None.apply(&mut image);
        assert_eq!(before, image.pixels());
        assert_eq!(Filter::parse(&[]).unwrap(), Filter::None);
        assert_eq!(
            Filter::parse(&["夜".as_bytes().to_vec()]).unwrap(),
            Filter::Night
        );
        assert!(Filter::parse(&[b"unknown".to_vec()]).is_err());
    }
}
