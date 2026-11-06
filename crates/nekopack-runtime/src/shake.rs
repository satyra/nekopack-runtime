//! BRAVE shake options and the shared TD logical-pixel displacement formula.
//! This is not wired into StorySession until snapshot/edge behaviour is verified.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shake {
    horizontal: i32,
    vertical: i32,
    duration_ms: u32,
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
