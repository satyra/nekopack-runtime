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
    pub fn complete(&self) -> bool {
        self.shown == self.total
    }
    pub fn visible(&self) -> &str {
        &self.text[..self.end]
    }
    pub fn count(&self) -> usize {
        self.shown
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
