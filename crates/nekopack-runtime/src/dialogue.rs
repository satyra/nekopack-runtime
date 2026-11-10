//! Plain-text presentation state. Time is supplied by the host's active clock.
pub(crate) struct Reveal {
    text: String,
    total: usize,
    end: usize,
    speed: u64,
    pub keep_voice: bool,
    elapsed: u64,
    shown: usize,
    pub advance: bool,
    completed_at: Option<u64>,
    auto_delay: u64,
}
impl Reveal {
    pub fn new(text: String, speed: u64, keep_voice: u64) -> Result<Self, String> {
        if speed > 100 || keep_voice > 1 {
            return Err(
                "unsupported TextSpeed/KeepVoice; supported ranges are 0..100 and 0..1".into(),
            );
        }
        let total = text.chars().count();
        Ok(Self {
            text,
            total,
            end: 0,
            speed,
            keep_voice: keep_voice == 1,
            elapsed: 0,
            shown: 0,
            advance: false,
            completed_at: None,
            auto_delay: 1750,
        })
    }
    pub fn update(&mut self, elapsed: u64, click: bool) -> Result<bool, String> {
        if elapsed < self.elapsed {
            return Err("dialogue clock moved backwards".into());
        }
        if click && self.advance {
            return Err("dialogue advance already granted".into());
        }
        self.elapsed = elapsed;
        let count = if self.speed == 100 {
            self.total
        } else {
            ((u128::from(elapsed) * u128::from(self.speed + 10) / 1000).min(self.total as u128))
                as usize
        };
        if count > self.shown {
            let (offset, character) = self.text[self.end..]
                .char_indices()
                .nth(count - self.shown - 1)
                .unwrap();
            self.end += offset + character.len_utf8();
            self.shown = count;
        }
        if self.complete() && self.completed_at.is_none() {
            self.completed_at = Some(elapsed);
        }
        if click {
            if self.complete() {
                self.advance = true;
                return Ok(true);
            }
            self.shown = self.total;
            self.end = self.text.len();
        }
        Ok(false)
    }
    pub fn set_auto_speed(&mut self, speed: u64) -> Result<(), String> {
        if speed > 100 {
            return Err("invalid AutoSpeed".into());
        }
        self.auto_delay = 3000 - speed * 25;
        Ok(())
    }
    pub fn auto_frame(
        &mut self,
        elapsed: u64,
        click: bool,
        enabled: bool,
        voiced: bool,
        voice_finished: bool,
    ) -> Result<bool, String> {
        if self.advance {
            return Err("dialogue advance already granted".into());
        }
        let manual = self.update(elapsed, click)?;
        if manual || click || !enabled {
            return Ok(manual);
        }
        let next = if voiced {
            voice_finished
        } else {
            self.completed_at
                .is_some_and(|at| elapsed.saturating_sub(at) > self.auto_delay + 1)
        };
        if next {
            return self.update(elapsed, true);
        }
        Ok(false)
    }
    pub fn complete(&self) -> bool {
        self.shown == self.total
    }
    pub fn visible(&self) -> &str {
        &self.text[..self.end]
    }
    pub fn elapsed(&self) -> u64 {
        self.elapsed
    }
    pub fn count(&self) -> usize {
        self.shown
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn auto_unvoiced_delay_disable_and_single_ack() {
        for (speed, delay) in [(0, 3000), (50, 1750), (100, 500)] {
            let mut r = Reveal::new("a".into(), 100, 1).unwrap();
            r.set_auto_speed(speed).unwrap();
            assert!(!r.auto_frame(0, false, true, false, true).unwrap());
            assert!(!r.auto_frame(delay + 1, false, true, false, true).unwrap());
            assert!(!r.auto_frame(delay + 2, false, false, false, true).unwrap());
            assert!(r.auto_frame(delay + 2, false, true, false, true).unwrap());
            assert!(r.auto_frame(delay + 3, false, true, false, true).is_err());
        }
    }
    #[test]
    fn auto_voice_completion_reveals_then_advances_and_pause_is_not_completion() {
        let mut r = Reveal::new("long voice text".into(), 0, 1).unwrap();
        assert!(!r.auto_frame(0, false, true, true, false).unwrap());
        assert!(!r.auto_frame(1, false, true, true, true).unwrap());
        assert!(r.complete());
        assert!(!r.auto_frame(1, false, true, true, false).unwrap());
        assert!(r.auto_frame(2, false, true, true, true).unwrap());
        let mut r = Reveal::new("text".into(), 100, 1).unwrap();
        assert!(!r.auto_frame(0, false, true, true, false).unwrap());
        assert!(!r.auto_frame(10000, false, true, true, false).unwrap());
        assert!(r.auto_frame(10001, false, true, true, true).unwrap());
    }
    #[test]
    fn two_clicks_unicode_and_monotonic_time() {
        let mut r = Reveal::new("中😀文abc".into(), 0, 1).unwrap();
        assert!(!r.update(200, false).unwrap());
        assert_eq!(r.visible(), "中😀");
        assert!(!r.update(200, true).unwrap());
        assert!(r.complete());
        assert!(!r.advance);
        assert!(r.update(200, true).unwrap());
        assert!(r.keep_voice);
        assert!(r.update(250, true).is_err());
        assert!(r.update(200, false).is_ok());
        assert!(r.update(199, false).is_err());
    }
    #[test]
    fn natural_completion_fast_text_and_invalid_config() {
        let mut r = Reveal::new("abcdef".into(), 50, 0).unwrap();
        r.update(99, false).unwrap();
        assert!(!r.complete());
        assert!(r.update(100, true).unwrap());
        assert!(!r.keep_voice);
        let mut fast = Reveal::new("abc".into(), 100, 1).unwrap();
        fast.update(0, false).unwrap();
        assert!(fast.complete());
        assert!(Reveal::new("".into(), 101, 0).is_err());
        assert!(Reveal::new("".into(), 50, 2).is_err());
        let mut long = Reveal::new("x".into(), 0, 1).unwrap();
        long.update(u64::MAX, false).unwrap();
        assert!(long.complete());
    }
}
