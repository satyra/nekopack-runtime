//! BLESS original 0x438640: 250 ms, sin((t - .5) * pi), truncation in centered coordinates.
pub(crate) const DURATION_MS: u64 = 250;
#[derive(Clone, Debug)]
pub(crate) struct Motion {
    pub slot: u8,
    pub start: i32,
    pub end: i32,
    pub center: i32,
}
impl Motion {
    pub fn new(slot: u8, start: i32, delta: i32, center: i32) -> Result<Self, String> {
        let end = start
            .checked_add(delta)
            .ok_or("actor motion target overflow")?;
        start
            .checked_sub(center)
            .ok_or("actor motion origin overflow")?;
        end.checked_sub(center)
            .ok_or("actor motion target overflow")?;
        Ok(Self {
            slot,
            start,
            end,
            center,
        })
    }
    pub fn x(&self, elapsed: u64) -> i32 {
        if elapsed == 0 {
            return self.start;
        }
        if elapsed >= DURATION_MS {
            return self.end;
        }
        let t = elapsed as f64 / DURATION_MS as f64;
        let weight = (((t - 0.5) * std::f64::consts::PI).sin() + 1.0) * 0.5;
        let relative = f64::from(self.start - self.center)
            + (f64::from(self.end) - f64::from(self.start)) * weight;
        self.center + relative as i32
    }
    pub fn metadata(&self) -> String {
        format!(
            "{{\"slot\":{},\"start_x\":{},\"end_x\":{},\"center_x\":{}}}",
            self.slot, self.start, self.end, self.center
        )
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_curve_centered_truncation_and_endpoints() {
        let m = Motion::new(6, 640, 350, 640).unwrap();
        assert_eq!(
            [0, 50, 125, 200, 250, 999].map(|t| m.x(t)),
            [640, 673, 815, 956, 990, 990]
        );
        let m = Motion::new(2, 440, 350, 640).unwrap();
        assert_eq!(m.x(50), 474); // trunc(-166.57), not trunc(473.42)
        assert!(Motion::new(1, i32::MAX, 50, 0).is_err());
    }
}
