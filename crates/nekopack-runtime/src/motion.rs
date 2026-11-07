//! Original 0x438640: timed actor operations, including cancelling `[]` components.
pub(crate) const DURATION_MS: u64 = 250;
#[derive(Clone, Debug)]
enum Kind {
    Rightward,
    Dip,
    Stationary,
    HorizontalExcursion(i32),
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
    pub fn stationary(slot: u8, start: [i32; 2], center: [i32; 2]) -> Result<Self, String> {
        let mut motion = Self::new(slot, start, 0, center)?;
        motion.kind = Kind::Stationary;
        Ok(motion)
    }
    pub fn horizontal_excursion(
        slot: u8,
        start: [i32; 2],
        amplitude: i32,
        center: [i32; 2],
    ) -> Result<Self, String> {
        let mut motion = Self::new(slot, start, 0, center)?;
        start[0]
            .checked_add(amplitude)
            .ok_or("actor excursion peak overflow")?;
        (start[0] - center[0])
            .checked_add(amplitude)
            .ok_or("actor excursion relative peak overflow")?;
        motion.kind = Kind::HorizontalExcursion(amplitude);
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
            Kind::Stationary => self.start,
            Kind::HorizontalExcursion(amplitude) => {
                let relative = f64::from(self.start[0] - self.center[0])
                    + f64::from(amplitude) * (t * std::f64::consts::PI).sin();
                [self.center[0] + relative as i32, self.start[1]]
            }
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
            Kind::HorizontalExcursion(amplitude) => format!(",\"kind\":\"horizontal_excursion\",\"start_y\":{},\"end_y\":{},\"center_y\":{},\"amplitude_x\":{amplitude}",self.start[1],self.end[1],self.center[1]),
            Kind::Stationary => format!(",\"kind\":\"stationary\",\"start_y\":{},\"end_y\":{},\"center_y\":{},\"amplitude_x\":0",self.start[1],self.end[1],self.center[1]),
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
// Each actor owns its clock. A late frame starts only the next segment at now.
#[derive(Clone, Debug)]
pub(crate) struct Queue {
    pub slot: u8,
    segments: Vec<Motion>,
    current: usize,
    started: u64,
}
impl Queue {
    pub fn new(slot: u8, segments: Vec<Motion>) -> Self {
        assert!(!segments.is_empty());
        Self {
            slot,
            segments,
            current: 0,
            started: 0,
        }
    }
    pub fn duration(&self) -> u64 {
        self.segments.len() as u64 * DURATION_MS
    }
    pub fn finished(&self) -> bool {
        self.current == self.segments.len()
    }
    pub fn position(&mut self, elapsed: u64) -> [i32; 2] {
        if !self.finished() && elapsed - self.started >= DURATION_MS {
            self.current += 1;
            self.started = elapsed;
        }
        if self.finished() {
            self.segments.last().unwrap().end
        } else {
            self.segments[self.current].position(elapsed - self.started)
        }
    }
    pub fn metadata(&self) -> String {
        if self.segments.len() == 1 {
            return self.segments[0].metadata();
        }
        format!(
            "{{\"slot\":{},\"kind\":\"queue\",\"segments\":[{}]}}",
            self.slot,
            self.segments
                .iter()
                .map(Motion::metadata)
                .collect::<Vec<_>>()
                .join(",")
        )
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn queue_discards_overshoot_and_actors_finish_independently() {
        let center = [640, 360];
        let mut queue = Queue::new(
            2,
            vec![
                Motion::dip(2, center, center).unwrap(),
                Motion::new(2, center, 200, center).unwrap(),
            ],
        );
        let mut other = Queue::new(1, vec![Motion::stationary(1, center, center).unwrap()]);
        assert_eq!(queue.position(125), [640, 410]);
        assert_eq!(queue.position(270), center);
        assert!(!queue.finished());
        assert_eq!(other.position(270), center);
        assert!(other.finished());
        assert_eq!(queue.position(270), center);
        assert_eq!(queue.position(395), [740, 360]);
        assert_eq!(queue.position(519), [839, 360]);
        assert!(!queue.finished());
        assert_eq!(queue.position(520), [840, 360]);
        assert!(queue.finished());
        assert_eq!(queue.position(9999), [840, 360]);
        let mut late = Queue::new(
            2,
            vec![
                Motion::dip(2, center, center).unwrap(),
                Motion::new(2, center, 200, center).unwrap(),
            ],
        );
        assert_eq!(late.position(9000), center);
        assert!(!late.finished());
        assert_eq!(late.position(9000), center);
        assert_eq!(late.position(9250), [840, 360]);
        assert!(late.finished());
    }
    #[test]
    fn left_brackets_return_to_anchor_and_truncate_centered_coordinates() {
        let m = Motion::horizontal_excursion(1, [1120, 360], -200, [640, 360]).unwrap();
        assert_eq!(
            [0, 50, 125, 200, 250, 999].map(|t| m.position(t)),
            [
                [1120, 360],
                [1002, 360],
                [920, 360],
                [1002, 360],
                [1120, 360],
                [1120, 360]
            ]
        );
        let negative = Motion::horizontal_excursion(1, [440, 360], -200, [640, 360]).unwrap();
        assert_eq!(negative.position(50), [323, 360]);
        assert!(Motion::horizontal_excursion(1, [i32::MIN + 199, 0], -200, [0, 0]).is_err());
        assert!(Motion::horizontal_excursion(1, [0, 0], -200, [i32::MAX, 0]).is_err());
    }
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
    fn cancelled_horizontal_components_retain_a_stationary_timed_operation() {
        let motion = Motion::stationary(6, [990, -1], [640, 360]).unwrap();
        for t in [0, 1, 50, 125, 249, 250, 999] {
            assert_eq!(motion.position(t), [990, -1]);
        }
        assert!(motion.metadata().contains("\"kind\":\"stationary\""));
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
