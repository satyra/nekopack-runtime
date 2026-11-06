//! Original 0x438640: rightward ease-in/out and `_` vertical sine excursion.
pub(crate) const DURATION_MS: u64 = 250;
#[derive(Clone, Debug)]
enum Kind {
    Rightward,
    Dip,
}
#[derive(Clone, Debug)]
pub(crate) struct Motion {
    pub slot: u8,
    pub start: [i32; 2],
    pub end: [i32; 2],
    pub center: [i32; 2],
    kind: Kind,
}
impl Motion {
    pub fn new(slot: u8, start: [i32; 2], delta: i32, center: [i32; 2]) -> Result<Self, String> {
        let end = [
            start[0]
                .checked_add(delta)
                .ok_or("actor motion target overflow")?,
            start[1],
        ];
        for axis in 0..2 {
            start[axis]
                .checked_sub(center[axis])
                .ok_or("actor motion origin overflow")?;
            end[axis]
                .checked_sub(center[axis])
                .ok_or("actor motion target overflow")?;
        }
        Ok(Self {
            slot,
            start,
            end,
            center,
            kind: Kind::Rightward,
        })
    }
    pub fn dip(slot: u8, start: [i32; 2], center: [i32; 2]) -> Result<Self, String> {
        let mut motion = Self::new(slot, start, 0, center)?;
        start[1].checked_add(50).ok_or("actor dip peak overflow")?;
        (start[1] - center[1])
            .checked_add(50)
            .ok_or("actor dip relative peak overflow")?;
        motion.kind = Kind::Dip;
        Ok(motion)
    }
    pub fn position(&self, elapsed: u64) -> [i32; 2] {
        if elapsed == 0 {
            return self.start;
        }
        if elapsed >= DURATION_MS {
            return self.end;
        }
        let t = elapsed as f64 / DURATION_MS as f64;
        match self.kind {
            Kind::Rightward => {
                let weight = (((t - 0.5) * std::f64::consts::PI).sin() + 1.0) * 0.5;
                let relative = f64::from(self.start[0] - self.center[0])
                    + (f64::from(self.end[0]) - f64::from(self.start[0])) * weight;
                [self.center[0] + relative as i32, self.start[1]]
            }
            Kind::Dip => {
                let relative = f64::from(self.start[1] - self.center[1])
                    + 50.0 * (t * std::f64::consts::PI).sin();
                [self.start[0], self.center[1] + relative as i32]
            }
        }
    }
    pub fn metadata(&self) -> String {
        let vertical = match self.kind {
            Kind::Rightward => String::new(),
            Kind::Dip => format!(
                ",\"kind\":\"dip\",\"start_y\":{},\"end_y\":{},\"center_y\":{},\"amplitude_y\":50",
                self.start[1], self.end[1], self.center[1]
            ),
        };
        format!(
            "{{\"slot\":{},\"start_x\":{},\"end_x\":{},\"center_x\":{}{vertical}}}",
            self.slot, self.start[0], self.end[0], self.center[0]
        )
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_curve_centered_truncation_and_endpoints() {
        let m = Motion::new(6, [640, 360], 350, [640, 360]).unwrap();
        assert_eq!(
            [0, 50, 125, 200, 250, 999].map(|t| m.position(t)[0]),
            [640, 673, 815, 956, 990, 990]
        );
        let m = Motion::new(2, [440, 360], 350, [640, 360]).unwrap();
        assert_eq!(m.position(50), [474, 360]);
        assert!(Motion::new(1, [i32::MAX, 0], 50, [0, 0]).is_err());
    }
    #[test]
    fn dip_is_positive_y_sine_with_no_persistent_displacement() {
        let m = Motion::dip(6, [990, 360], [640, 360]).unwrap();
        assert_eq!(
            [0, 50, 125, 200, 250, 999].map(|t| m.position(t)),
            [
                [990, 360],
                [990, 389],
                [990, 410],
                [990, 389],
                [990, 360],
                [990, 360]
            ]
        );
        let negative = Motion::dip(2, [20, 50], [20, 100]).unwrap();
        assert_eq!(negative.position(50), [20, 80]); // centered truncation, not absolute 79
        assert!(Motion::dip(1, [0, i32::MAX - 49], [0, 0]).is_err());
        assert!(Motion::dip(1, [0, i32::MIN], [0, 1]).is_err());
    }
}
