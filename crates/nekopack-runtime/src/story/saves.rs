//! Runtime-owned saves; intentionally incompatible with original .sav files.
use super::*;
use nekopack_script::checkpoint::{digest, Reader, Writer, LIMIT};

pub(crate) struct Saved {
    machine: Machine,
    scene: Scene,
    labels: StoryLabels,
    speaker: String,
    voice: String,
    motions: Vec<crate::motion::Queue>,
    elapsed: u64,
    audio: BTreeMap<String, (String, bool)>,
    audio_effects: std::collections::VecDeque<HostEffect>,
    page_effects: Vec<String>,
}
impl<R: Read + Seek> StorySession<R> {
    pub fn side_menu(&mut self, quick: Option<&[u8]>) -> Result<Vec<u8>, Box<dyn Error>> {
        let available = quick.is_some_and(|b| {
            Saved::decode(b, &mut self.store, self.scene.dimensions, false).is_ok()
        });
        self.system.eval("side-options", "return Config.Popup")?;
        let popup = self.system.result().trim() != "0";
        self.side_menu = Some(crate::side_menu::SideMenu::open(
            &mut self.store,
            available,
            popup,
        )?);
        self.side_menu.as_ref().unwrap().packet("")
    }
    pub fn side_pointer(&mut self, kind: i32, x: i32, y: i32) -> Result<Vec<u8>, Box<dyn Error>> {
        self.side_menu
            .as_mut()
            .ok_or("side menu is not initialized")?
            .pointer(kind, x, y)
    }
    pub fn begin_save(&mut self, sequence: u64) -> Result<Vec<u8>, Box<dyn Error>> {
        if self.failed
            || self.return_title_requested
            || self.menu.is_some()
            || self.settings.is_some()
            || self.dialogue_hidden
            || self.pending_effect.is_some()
            || self.page.as_ref().map(|p| p.sequence) != Some(sequence)
            || self.text_reveal.as_ref().is_none_or(|r| r.advance)
        {
            return Err("stale save/load request".into());
        }
        self.save_modal = true;
        self.auto_mode = false;
        self.skip_mode = false;
        Ok(b"{\"ready\":true}".to_vec())
    }
    pub fn open_save_ui(&mut self, saving: bool) -> Result<Vec<u8>, Box<dyn Error>> {
        self.save_boundary()?;
        self.save_ui = Some(crate::save_ui::SaveUi::open(
            &mut self.store,
            saving,
            false,
        )?);
        self.save_ui_packet(0, true)
    }
    pub fn save_ui_pointer(
        &mut self,
        kind: i32,
        x: i32,
        y: i32,
    ) -> Result<Vec<u8>, Box<dyn Error>> {
        let (action, refresh) = self
            .save_ui
            .as_mut()
            .ok_or("save UI is not open")?
            .pointer(kind, x, y)?;
        self.save_ui_packet(action, refresh)
    }
    pub fn save_ui_packet(&self, action: i32, refresh: bool) -> Result<Vec<u8>, Box<dyn Error>> {
        self.save_ui.as_ref().ok_or("save UI is not open")?.packet(
            self.preview_font
                .as_ref()
                .ok_or("save UI font not installed")?,
            action,
            refresh,
        )
    }
    pub fn save_ui_slot(
        &mut self,
        id: usize,
        bytes: Option<&[u8]>,
        stamp: String,
    ) -> Result<(), Box<dyn Error>> {
        let value = bytes
            .map(|b| {
                Saved::decode(b, &mut self.store, self.scene.dimensions, false)
                    .and_then(|s| s.preview(stamp))
            })
            .transpose()
            .map_err(|e| e.to_string());
        self.save_ui
            .as_mut()
            .ok_or("save UI is not open")?
            .set_slot(id, value)
    }
    fn save_boundary(&self) -> Result<(), Box<dyn Error>> {
        if !self.save_modal
            || self.failed
            || self.return_title_requested
            || self.pending_effect.is_some()
            || self.page.is_none()
            || self.text_reveal.as_ref().is_none_or(|r| r.advance)
        {
            return Err("save/load requires an unadvanced dialogue modal".into());
        }
        Ok(())
    }
    pub fn cancel_save(&mut self) -> Result<Vec<u8>, Box<dyn Error>> {
        self.save_boundary()?;
        self.save_ui = None;
        self.prepared_save = None;
        self.save_modal = false;
        Ok(b"{\"closed\":true}".to_vec())
    }
    pub fn export_save(&self) -> Result<Vec<u8>, Box<dyn Error>> {
        self.save_boundary()?;
        if !self.labels.in_route {
            return Err("save unavailable outside a route".into());
        }
        let mut w = Writer::default();
        w.number(self.store.save_identity());
        w.bytes(&self.machine.save_dialogue()?);
        w.text(&self.labels.title);
        w.text(&self.labels.route);
        w.text(&self.labels.caption);
        w.text(&self.speaker);
        w.text(&self.voice);
        let effects = &self.page.as_ref().unwrap().effects;
        w.number(effects.len() as u64);
        for effect in effects {
            w.text(effect);
        }
        self.scene.save(&mut w)?;
        w.number(self.motion_elapsed);
        w.number(self.motions.len() as u64);
        for m in &self.motions {
            m.save(&mut w);
        }
        w.number(self.persistent_audio.len() as u64);
        for (kind, (name, looping)) in &self.persistent_audio {
            w.text(kind);
            w.text(name);
            w.number(u64::from(*looping));
        }
        if w.0.len() > LIMIT - 13 {
            return Err("runtime save exceeds limit".into());
        }
        let mut out = b"NKSV\x01".to_vec();
        out.extend(digest(&w.0).to_le_bytes());
        out.extend(w.0);
        Ok(out)
    }
    /// Validate and materialize everything before the host releases the current presentation.
    /// A failed preparation leaves the live VM, dialogue and audio untouched.
    pub fn prepare_save(&mut self, bytes: &[u8]) -> Result<Vec<u8>, Box<dyn Error>> {
        self.save_boundary()?;
        self.prepared_save = None;
        let saved = Saved::decode(bytes, &mut self.store, self.scene.dimensions, true)?;
        let (source, line) = saved.machine.location().unwrap();
        let result = format!(
            "{{\"prepared\":true,\"source\":{},\"line\":{line},\"depth\":{}}}",
            quote(source)?,
            saved.machine.depth()
        )
        .into_bytes();
        self.prepared_save = Some(saved);
        Ok(result)
    }
    pub fn commit_save(&mut self) -> Result<Vec<u8>, Box<dyn Error>> {
        self.save_boundary()?;
        let saved = self.prepared_save.take().ok_or("no prepared save")?;
        self.apply_saved(saved)
    }
    pub(crate) fn apply_saved(&mut self, saved: Saved) -> Result<Vec<u8>, Box<dyn Error>> {
        self.save_ui = None;
        self.system.eval(
            "restore-labels",
            &format!(
                "StoryTitle={}\nStoryRoute={}\nHost.caption={}",
                quote(saved.labels.title.as_bytes())?,
                quote(saved.labels.route.as_bytes())?,
                quote(saved.labels.caption.as_bytes())?
            ),
        )?;
        self.machine = saved.machine;
        self.scene = saved.scene;
        self.labels = saved.labels;
        self.speaker = saved.speaker;
        self.voice = saved.voice;
        self.motions = saved.motions;
        self.motion_elapsed = saved.elapsed;
        self.motion_restore_base = saved.elapsed;
        self.persistent_audio = saved.audio;
        self.restore_audio = saved.audio_effects;
        self.page = None;
        self.text_reveal = None;
        self.effects = saved.page_effects;
        self.save_modal = false;
        self.auto_mode = false;
        self.skip_mode = false;
        self.dialogue_hidden = false;
        self.wipe = None;
        self.shake = None;
        self.flash = None;
        // Materialize the pending text without completing it or executing preceding commands.
        self.advance()?;
        for effect in &mut self.restore_audio {
            self.host_sequence += 1;
            effect.token = self.host_sequence;
        }
        self.pending_effect = self.restore_audio.pop_front();
        self.restoring = self.pending_effect.is_some();
        self.poll_packet()
    }
}

impl Saved {
    pub(crate) fn decode<R: Read + Seek>(
        bytes: &[u8],
        store: &mut DatGame<R>,
        dimensions: [u32; 2],
        with_audio: bool,
    ) -> Result<Self, Box<dyn Error>> {
        if bytes.len() > LIMIT || bytes.len() < 13 || &bytes[..5] != b"NKSV\x01" {
            return Err("unknown runtime save format".into());
        }
        if digest(&bytes[13..]) != u64::from_le_bytes(bytes[5..13].try_into()?) {
            return Err("runtime save checksum mismatch".into());
        }
        let mut r = Reader::new(&bytes[13..])?;
        if r.number()? != store.save_identity() {
            return Err("save belongs to a different DAT mount".into());
        }
        let machine = Machine::load_dialogue(r.bytes()?, store, 10000, 64)?;
        let title = r.text()?;
        let route = r.text()?;
        let caption = r.text()?;
        if route.is_empty()
            || [&title, &route, &caption]
                .iter()
                .any(|s| s.chars().any(char::is_control))
        {
            return Err("invalid saved route labels".into());
        }
        let labels = StoryLabels {
            title,
            route,
            caption,
            in_route: true,
        };
        let speaker = r.text()?;
        let voice = r.text()?;
        let mut page_effects = Vec::new();
        for _ in 0..r.count(10000)? {
            page_effects.push(r.text()?);
        }
        let scene = Scene::load(&mut r, dimensions)?;
        let elapsed = r.number()?;
        let mut motions = Vec::new();
        for _ in 0..r.count(9)? {
            let m = crate::motion::Queue::load(&mut r, elapsed)?;
            if !scene.actors.iter().any(|a| a.slot == m.slot)
                || motions
                    .iter()
                    .any(|old: &crate::motion::Queue| old.slot == m.slot)
            {
                return Err("invalid saved motion owner".into());
            }
            motions.push(m);
        }
        let mut audio = BTreeMap::new();
        for _ in 0..r.count(3)? {
            let kind = r.text()?;
            let name = r.text()?;
            if !matches!(kind.as_str(), "bgm" | "env" | "se")
                || name.is_empty()
                || r.number()? != 1
                || audio.insert(kind, (name, true)).is_some()
            {
                return Err("invalid saved audio".into());
            }
        }
        r.finish()?;
        let mut audio_effects = std::collections::VecDeque::new();
        let mut sounds = audio.clone();
        if !voice.is_empty() {
            sounds.insert("voice".into(), (voice.clone(), false));
        }
        for (kind, (name, looping)) in sounds.into_iter().filter(|_| with_audio) {
            let directory = match kind.as_str() {
                "bgm" => "sound/bgm",
                "env" => "sound/env",
                "se" => "sound/se",
                _ => "voice",
            };
            let payload = store.read(directory, &format!("{name}.ogg"))?;
            if !payload.starts_with(b"OggS") {
                return Err("invalid saved audio resource".into());
            }
            audio_effects.push_back(HostEffect {
                token: 0,
                kind,
                name,
                duration_ms: 0,
                movie_left_skip: false,
                audio_loop: looping,
                payload,
            });
        }
        // Validate rendering before the destructive commit, including saved positions.
        let mut saved = Saved {
            page_effects,
            machine,
            scene,
            labels,
            speaker,
            voice,
            motions,
            elapsed,
            audio,
            audio_effects,
        };
        saved.scene.render()?;
        Ok(saved)
    }
    pub(crate) fn preview(mut self, stamp: String) -> Result<crate::save_ui::Slot, Box<dyn Error>> {
        let mut motions = self.motions;
        let positions = motions
            .iter_mut()
            .map(|m| (m.slot, m.position(self.elapsed)))
            .collect::<Vec<_>>();
        let pixels = self.scene.render_pixels(&positions)?;
        Ok(crate::save_ui::Slot {
            title: self.labels.title,
            route: self.labels.route,
            stamp,
            thumbnail: crate::save_ui::thumbnail(&pixels)?,
        })
    }
}
