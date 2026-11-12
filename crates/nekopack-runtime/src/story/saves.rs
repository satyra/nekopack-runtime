//! Runtime-owned saves; intentionally incompatible with original .sav files.
use super::*;
use nekopack_script::checkpoint::{LIMIT, Reader, Writer, digest};

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
    history: Option<crate::message_log::History>,
    last_wipe_mask: Option<String>,
}
impl<R: Read + Seek> StorySession<R> {
    pub fn restore_route_preview(&mut self, bytes: &[u8]) -> Result<(), Box<dyn Error>> {
        let saved = Saved::decode(bytes, &mut self.store, self.scene.dimensions, false)?;
        self.apply_saved(saved)?;
        Ok(())
    }

    pub fn side_menu(&mut self, quick: Option<&[u8]>) -> Result<Vec<u8>, Box<dyn Error>> {
        let available = quick.is_some_and(|b| {
            Saved::decode(b, &mut self.store, self.scene.dimensions, false).is_ok()
        });
        let popup = crate::preferences::enabled(&mut self.system, "Popup")?;
        self.side_menu = Some(crate::side_menu::SideMenu::open(
            &mut self.store,
            available,
            popup,
            crate::preferences::enabled(&mut self.system, "KeepIcon")?,
            self.labels.in_route,
            self.message_history
                .entries
                .iter()
                .any(|entry| !entry.voice.is_empty()),
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
        if self.log_ui.is_some()
            || self.failed
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
        self.dialogue_modal = true;
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
        if self.log_ui.is_some()
            || !self.dialogue_modal
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
        self.dialogue_modal = false;
        Ok(b"{\"closed\":true}".to_vec())
    }
    pub fn export_save(&self) -> Result<Vec<u8>, Box<dyn Error>> {
        self.save_boundary()?;
        self.serialize_dialogue_save()
    }

    pub fn quit_autosave_ready(&mut self) -> Result<bool, Box<dyn Error>> {
        Ok(!self.replay
            && self.labels.in_route
            && !self.failed
            && self.pending_effect.is_none()
            && self.page.is_some()
            && self
                .text_reveal
                .as_ref()
                .is_none_or(|reveal| !reveal.advance)
            && crate::preferences::enabled(&mut self.system, "AutoSave")?)
    }

    pub fn export_quit_autosave(&mut self) -> Result<Vec<u8>, Box<dyn Error>> {
        if !self.quit_autosave_ready()? {
            return Err("no dialogue available for exit quicksave".into());
        }
        self.serialize_dialogue_save()
    }

    pub fn export_route_preview(&self) -> Result<Vec<u8>, Box<dyn Error>> {
        if self.host_mode || self.metadata_only || self.page.is_none() {
            return Err("route checkpoint requires a rendered preview dialogue".into());
        }
        self.serialize_dialogue_save()
    }

    fn serialize_dialogue_save(&self) -> Result<Vec<u8>, Box<dyn Error>> {
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
        self.message_history.save(&mut w);
        w.text(self.last_wipe_mask.as_deref().unwrap_or(""));
        w.number(u64::from(self.scene.cinema_frame));
        w.number(self.scene.mapped_actors.len() as u64);
        for (slot, actor) in &self.scene.mapped_actors {
            w.number((*slot).into());
            w.text(&actor.containers.join("#"));
            w.text(&actor.expression);
            w.number(actor.extras.len() as u64);
            for (book, title) in &actor.extras {
                w.text(book);
                w.text(title);
            }
            w.number(u64::from(actor.anchor.is_some()));
            if let Some(anchor) = actor.anchor {
                for coordinate in anchor {
                    w.number(u64::from(coordinate as u32));
                }
            }
        }
        if w.0.len() > LIMIT - 13 {
            return Err("runtime save exceeds limit".into());
        }
        let mut out = b"NKSV\x05".to_vec();
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
        self.rebuilding_page = saved.history.is_some();
        self.message_history = saved.history.unwrap_or_default();
        self.log_face = self
            .message_history
            .entries
            .last()
            .map(|e| e.face.clone())
            .unwrap_or_default();
        self.unlocks.capture(&self.machine.variables);
        self.machine = saved.machine;
        self.unlocks.apply(&mut self.machine.variables);
        if self.host_mode {
            self.machine.set_step_budget(super::STORY_STEP_BUDGET);
        }
        self.scene = saved.scene;
        self.dialogue_scene_png = None;
        self.last_wipe_mask = saved.last_wipe_mask;
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
        self.dialogue_modal = false;
        self.auto_mode = false;
        self.skip_mode = false;
        self.dialogue_hidden = false;
        self.wipe = None;
        self.shake = None;
        self.flash = None;
        // Materialize the pending text without completing it or executing preceding commands.
        self.advance()?;
        self.rebuilding_page = false;
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
        if bytes.len() > LIMIT
            || bytes.len() < 13
            || &bytes[..4] != b"NKSV"
            || !matches!(bytes[4], 1..=5)
        {
            return Err("unknown runtime save format".into());
        }
        if digest(&bytes[13..]) != u64::from_le_bytes(bytes[5..13].try_into()?) {
            return Err("runtime save checksum mismatch".into());
        }
        let mut r = Reader::new(&bytes[13..])?;
        if r.number()? != store.save_identity() {
            return Err("save belongs to a different DAT mount".into());
        }
        let machine = Machine::load_dialogue(r.bytes()?, store, super::STORY_STEP_BUDGET, 64)?;
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
        let mut scene = Scene::load(&mut r, dimensions)?;
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
        let history = if bytes[4] >= 2 {
            Some(crate::message_log::History::load(&mut r)?)
        } else {
            None
        };
        let last_wipe_mask = if bytes[4] >= 3 {
            let mask = r.text()?;
            if mask.is_empty() {
                None
            } else {
                store.read("image/mask", &mask)?;
                Some(mask)
            }
        } else {
            None
        };
        if bytes[4] >= 4 {
            scene.cinema_frame = r.count(1)? == 1;
        }
        if bytes[4] >= 5 {
            for _ in 0..r.count(64)? {
                let slot = u8::try_from(r.number()?)?;
                let containers = r
                    .text()?
                    .split('#')
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned)
                    .collect();
                let expression = r.text()?;
                let mut extras = Vec::new();
                for _ in 0..r.count(32)? {
                    extras.push((r.text()?, r.text()?));
                }
                let anchor = if r.count(1)? == 1 {
                    Some([
                        u32::try_from(r.number()?)? as i32,
                        u32::try_from(r.number()?)? as i32,
                    ])
                } else {
                    None
                };
                if slot == 0
                    || scene
                        .mapped_actors
                        .insert(
                            slot,
                            super::mapped_actor::Actor {
                                containers,
                                expression,
                                extras,
                                anchor,
                            },
                        )
                        .is_some()
                {
                    return Err("invalid mapped actor slot".into());
                }
            }
        }
        if let Some(h) = &history {
            let last = h.entries.last().ok_or("empty saved message log")?;
            if last.name != speaker
                || last.voice != voice
                || Some(last.text.as_bytes()) != machine.pending_text()
            {
                return Err("saved log does not match dialogue".into());
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
            history,
            last_wipe_mask,
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
