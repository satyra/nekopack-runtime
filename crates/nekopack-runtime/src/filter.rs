//! BLESS actor-load filter, from the original saturated BGRA add/subtract path.
use nekopack_image::RgbaImage;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Filter {
    #[default]
    None,
    Night,
    Sunset,
}
impl Filter {
    pub fn parse(args: &[Vec<u8>]) -> Result<Self, String> {
        match args {
            [] => Ok(Self::None),
            [name] if name == "夜".as_bytes() => Ok(Self::Night),
            [name] if name == "夕".as_bytes() => Ok(Self::Sunset),
            _ => Err("unsupported actor filter arguments".into()),
        }
    }
    pub fn metadata(self) -> &'static str {
        match self {
            Self::None => "null",
            Self::Night => "\"night\"",
            Self::Sunset => "\"sunset\"",
        }
    }
    pub fn apply(self, image: &mut RgbaImage) {
        // Convert the original packed BGRA operands to RGB channel order.
        let (add, subtract) = match self {
            Self::None => return,
            Self::Night => ([0, 0, 17], [17, 22, 0]),
            Self::Sunset => ([34, 0, 0], [0, 16, 34]),
        };
        for pixel in image.pixels_mut().as_chunks_mut::<4>().0 {
            for channel in 0..3 {
                pixel[channel] = pixel[channel]
                    .saturating_add(add[channel])
                    .saturating_sub(subtract[channel]);
            }
        }
    }
    pub fn save_id(self) -> u64 {
        match self {
            Self::None => 0,
            Self::Night => 1,
            Self::Sunset => 2,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sunset_uses_original_rgb_operands_and_preserves_alpha() {
        let mut image =
            RgbaImage::from_rgba(2, 1, vec![240, 64, 100, 19, 10, 12, 28, 0], 8).unwrap();
        let filter = Filter::parse(&["夕".as_bytes().to_vec()]).unwrap();
        assert_eq!(filter, Filter::Sunset);
        filter.apply(&mut image);
        assert_eq!(image.pixels(), &[255, 48, 66, 19, 44, 0, 0, 0]);
    }
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
