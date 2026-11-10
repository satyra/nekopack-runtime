use super::*;
impl<R: Read + Seek> StorySession<R> {
    pub fn open_log(&mut self, sequence: u64) -> Result<Vec<u8>, Box<dyn Error>> {
        self.begin_save(sequence)?;
        let mut motions = self.motions.clone();
        let positions = motions
            .iter_mut()
            .map(|m| (m.slot, m.position(self.motion_elapsed)))
            .collect::<Vec<_>>();
        let base = self.scene.render_pixels(&positions)?;
        self.log_ui = Some(crate::message_log::LogUi::open(
            &mut self.store,
            base,
            self.message_history.entries.len(),
        )?);
        self.log_packet(0)
    }
    fn log_packet(&self, replay: usize) -> Result<Vec<u8>, Box<dyn Error>> {
        self.log_ui.as_ref().ok_or("log is not open")?.packet(
            &self.message_history,
            self.preview_font
                .as_ref()
                .ok_or("log font is not installed")?,
            replay,
        )
    }
    pub fn log_pointer(&mut self, kind: i32, x: i32, y: i32) -> Result<Vec<u8>, Box<dyn Error>> {
        let replay = self.log_ui.as_mut().ok_or("log is not open")?.pointer(
            &self.message_history,
            kind,
            x,
            y,
        )?;
        self.log_packet(replay)
    }
    pub fn close_log(&mut self, sequence: u64) -> Result<Vec<u8>, Box<dyn Error>> {
        if self.page.as_ref().map(|p| p.sequence) != Some(sequence) || self.log_ui.is_none() {
            return Err("stale log close".into());
        }
        self.log_ui = None;
        self.dialogue_modal = false;
        Ok(format!("{{\"kind\":\"log_closed\",\"sequence\":{sequence}}}").into_bytes())
    }
    pub fn log_voice(&mut self, index: usize) -> Result<Vec<u8>, Box<dyn Error>> {
        if self.log_ui.is_none() {
            return Err("log is not open".into());
        }
        let e = self
            .message_history
            .entries
            .get(index.checked_sub(1).ok_or("invalid log index")?)
            .ok_or("invalid log index")?;
        if e.name.is_empty() || e.voice.is_empty() {
            return Err("log entry has no replayable voice".into());
        }
        crate::message_log::audio(&mut self.store, &mut self.system, Some(&e.voice))
    }
    pub fn star_sound(&mut self) -> Result<Vec<u8>, Box<dyn Error>> {
        crate::message_log::audio(&mut self.store, &mut self.system, None)
    }
}
