//! TD mask kernel: white mask samples reveal the target first. Integer 0..256 phase.
use nekopack_image::RgbaImage;
use std::rc::Rc;
pub struct Wipe {
    old: RgbaImage,
    target: RgbaImage,
    mask: Rc<RgbaImage>,
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
        Self::with_mask(old, target, Rc::new(mask), soft, duration_ms)
    }
    pub(crate) fn with_mask(
        old: RgbaImage, target: RgbaImage, mask: Rc<RgbaImage>, soft: bool, duration_ms: u64,
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
        let mode=if self.soft {crate::system_command::mask::Mode::Fade} else {crate::system_command::mask::Mode::Hard};
        crate::system_command::mask::render(&self.old,&self.target,&self.mask,q,mode).map_err(|e|e.to_string())
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
