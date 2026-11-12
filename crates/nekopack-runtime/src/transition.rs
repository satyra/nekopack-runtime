//! TD mask kernel: white mask samples reveal the target first. Integer 0..256 phase.
use nekopack_image::RgbaImage;
pub struct Wipe {
    old: RgbaImage,
    target: RgbaImage,
    mask: RgbaImage,
    soft: bool,
    duration_ms: u64,
}
impl Wipe {
    pub fn new(
        old: RgbaImage,
        target: RgbaImage,
        mask: RgbaImage,
        soft: bool,
        duration_ms: u64,
    ) -> Result<Self, String> {
        if duration_ms > i32::MAX as u64 {
            return Err("wipe duration overflow".into());
        }
        if old.pixels().len() + target.pixels().len() + mask.pixels().len() > 64 * 1024 * 1024 {
            return Err("wipe retained pixel budget exceeded".into());
        }
        let size = (target.width(), target.height());
        if (old.width(), old.height()) != size || (mask.width(), mask.height()) != size {
            return Err("wipe image dimensions mismatch".into());
        }
        Ok(Self {
            old,
            target,
            mask,
            soft,
            duration_ms,
        })
    }
    pub fn frame(&self, elapsed_ms: u64) -> Result<RgbaImage, String> {
        let q = if self.duration_ms == 0 || elapsed_ms >= self.duration_ms {
            256
        } else {
            (elapsed_ms * 256 / self.duration_ms) as i32
        };
        let mut pixels = self.old.pixels().to_vec();
        for ((out, target), mask) in pixels
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(self.target.pixels().as_chunks::<4>().0)
            .zip(self.mask.pixels().as_chunks::<4>().0)
        {
            let threshold = i32::from(mask[0]);
            if q >= 255 || (q > 0 && !self.soft && threshold + q >= 255) {
                *out = *target;
            } else if q > 0 && self.soft {
                let alpha = (threshold + 2 * q - 255).clamp(0, 255);
                for (sample, next) in out.iter_mut().zip(target) {
                    *sample = (i32::from(*sample)
                        + ((i32::from(*next) - i32::from(*sample)) * alpha).div_euclid(256))
                        as u8;
                }
            }
        }
        RgbaImage::from_rgba(
            self.target.width(),
            self.target.height(),
            pixels,
            64 * 1024 * 1024,
        )
        .map_err(|e| e.to_string())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn image(pixels: Vec<u8>) -> RgbaImage {
        RgbaImage::from_rgba(2, 1, pixels, 8).unwrap()
    }
    #[test]
    fn threshold_direction_endpoints_and_soft_rounding() {
        let mut w = Wipe::new(
            image(vec![200, 200, 200, 255, 20, 20, 20, 255]),
            image(vec![20, 20, 20, 255, 200, 200, 200, 255]),
            image(vec![0, 0, 0, 255, 255, 255, 255, 255]),
            false,
            100,
        )
        .unwrap();
        assert_eq!(w.frame(0).unwrap(), w.old);
        assert_eq!(w.frame(100).unwrap(), w.target);
        assert_eq!(
            w.frame(50).unwrap().pixels(),
            &[200, 200, 200, 255, 200, 200, 200, 255]
        );
        w.soft = true;
        assert_eq!(
            w.frame(50).unwrap().pixels(),
            &[199, 199, 199, 255, 199, 199, 199, 255]
        );
    }
    #[test]
    fn malformed_frame_buffer_is_rejected() {
        assert!(RgbaImage::from_rgba(2, 1, vec![0; 4], 8).is_err());
    }
}
