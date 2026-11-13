//! Runtime-owned saves; intentionally incompatible with original .sav files.
use super::*;
use nekopack_script::checkpoint::{digest, Reader, Writer, LIMIT};

pub(crate) struct Saved {
    machine: Machine,
    selection: bool,
    scene: Scene,
    labels: StoryLabels,
    speaker: String,
    voice: String,
    motions: Vec<crate::motion::Queue>,
    elapsed: u64,
    audio: BTreeMap<String, (String, bool)>,
    bgv_tags: BTreeMap<String, String>,
    audio_effects: std::collections::VecDeque<HostEffect>,
    page_effects: Vec<String>,
    history: Option<crate::message_log::History>,
    last_wipe_mask: Option<String>,
    delayed_scene: Option<system_commands::Delay>,
    system_state: BTreeMap<String, i64>,
    native_messages: Vec<super::native_load::NativeMessage>,
    navigation_back: Vec<Vec<u8>>,
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
        let keep_icon = crate::preferences::enabled(&mut self.system, "KeepIcon")?;
        let (next, back) = self.shortcut_navigation_state()?;
        self.side_menu = Some(crate::side_menu::SideMenu::open(
            &mut self.store,
            &mut self.system,
            &mut self.machine.variables,
            available,
            popup,
            keep_icon,
            self.labels.in_route,
            self.message_history
                .entries
                .iter()
                .any(|entry| !entry.voice.is_empty()),
            self.skip_mode,
            self.auto_mode,
            next,
            back,
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
            && self.system_command.is_none()
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
        self.serialize_story_save(false)
    }

    pub(super) fn serialize_selection_save(&self) -> Result<Vec<u8>, Box<dyn Error>> {
        self.serialize_story_save(true)
    }

    fn serialize_story_save(&self, selection: bool) -> Result<Vec<u8>, Box<dyn Error>> {
        if self.system_command.is_some() { return Err("cannot save a suspended system command".into()); }
        if !self.labels.in_route {
            return Err("save unavailable outside a route".into());
        }
        let mut w = Writer::default();
        w.number(self.store.save_identity());
        w.bytes(&if selection {
            self.machine.save_selection()?
        } else {
            self.machine.save_dialogue()?
        });
        w.text(&self.labels.title);
        w.text(&self.labels.route);
        w.text(&self.labels.caption);
        w.text(&self.speaker);
        w.text(&self.voice);
        let effects = if selection {
            &self.effects
        } else {
            &self.page.as_ref().unwrap().effects
        };
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
        w.number(self.persistent_bgv_tags.len() as u64);
        for (kind, tag) in &self.persistent_bgv_tags {
            w.text(kind);
            w.text(tag);
        }
        w.number(u64::from(self.delayed_scene.is_some()));
        if let Some(delay) = &self.delayed_scene { delay.save(&mut w, self.motion_elapsed)?; }
        w.number(self.system_save_state.len() as u64);
        for (name, value) in &self.system_save_state { w.text(name); w.number(*value as u64); }
        w.number(self.native_load.messages.len() as u64);
        for entry in &self.native_load.messages { entry.save(&mut w); }
        w.number(if selection { 0 } else { self.navigation_back.len() as u64 });
        if !selection { for checkpoint in &self.navigation_back { w.bytes(checkpoint); } }
        if w.0.len() > LIMIT - 13 {
            return Err("runtime save exceeds limit".into());
        }
        let mut out = if selection {
            b"NKSC\x09".to_vec()
        } else {
            b"NKSV\x09".to_vec()
        };
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
        let mut saved = saved;
        let native_back = saved.native_back_expression(&mut self.store, self.scene.dimensions)?;
        self.system.eval("restore-native-back", &format!("SkipBackData <- {native_back}"))?;
        for (name, value) in &saved.system_state {
            self.system.eval("restore-original-message-state", &format!("getroottable()[{}]={value}", quote(name.as_bytes())?))?;
        }
        self.system_save_state = std::mem::take(&mut saved.system_state);
        self.message_overlay_snapshot = None;
        for effect in &mut saved.audio_effects {
            if !effect.voice_tag.is_empty() {
                effect.audio_volume = super::bgv::role_volume(&mut self.system, &effect.voice_tag)?;
            }
        }
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
        let selection = saved.selection;
        if !selection {
            self.navigation_back = std::mem::take(&mut saved.navigation_back);
        }
        self.native_load.messages = std::mem::take(&mut saved.native_messages);
        let native_log = self.native_load.messages.iter().enumerate().map(|(index,entry)| entry.expression(saved.history.as_ref()
            .map(|history| history.entries[index].face.as_str()).unwrap_or_default())).collect::<Result<Vec<_>,_>>()?.join(",");
        self.system.eval("restore-native-message-log", &format!("MessageLog <- [{native_log}]"))?;
        self.system
            .eval("restore-navigation", "MegaSkip <- false")?;
        self.rebuilding_page = !selection && saved.history.is_some();
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
        self.system_command = None;
        self.last_message_overlay = None;
        self.system_command_sounds.clear();
        self.delayed_scene = saved.delayed_scene;
        self.dialogue_scene_png = None;
        self.last_wipe_mask = saved.last_wipe_mask;
        self.labels = saved.labels;
        self.speaker = saved.speaker;
        self.voice = saved.voice;
        self.motions = saved.motions;
        self.motion_elapsed = saved.elapsed;
        self.motion_restore_base = saved.elapsed;
        self.persistent_audio = saved.audio;
        self.persistent_bgv_tags = saved.bgv_tags;
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
        // Selection resumes after restored audio; no branch is completed during BACK.
        self.pending_effect = None;
        self.next_skip = false;
        if !selection {
            self.advance()?;
        }
        self.rebuilding_page = false;
        if selection {
            let payload = self.render()?;
            self.restore_audio.push_back(HostEffect {
                token: 0,
                kind: "fade".into(),
                name: "0".into(),
                duration_ms: 0,
                movie_left_skip: false,
                audio_loop: false,
                audio_volume: 100,
                voice_tag: String::new(),
                payload,
            });
        }
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
        Self::decode_kind(bytes, store, dimensions, with_audio, false)
    }
    pub(super) fn decode_selection<R: Read + Seek>(
        bytes: &[u8],
        store: &mut DatGame<R>,
        dimensions: [u32; 2],
    ) -> Result<Self, Box<dyn Error>> {
        Self::decode_kind(bytes, store, dimensions, true, true)
    }
    fn decode_kind<R: Read + Seek>(
        bytes: &[u8],
        store: &mut DatGame<R>,
        dimensions: [u32; 2],
        with_audio: bool,
        selection: bool,
    ) -> Result<Self, Box<dyn Error>> {
        if bytes.len() > LIMIT
            || bytes.len() < 13
            || &bytes[..4] != if selection { b"NKSC" } else { b"NKSV" }
            || !matches!(bytes[4], 1..=9)
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
        let machine = if selection {
            Machine::load_selection(r.bytes()?, store, super::STORY_STEP_BUDGET, 64)?
        } else {
            Machine::load_dialogue(r.bytes()?, store, super::STORY_STEP_BUDGET, 64)?
        };
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
        for _ in 0..r.count(if bytes[4] >= 6 { 5 } else { 3 })? {
            let kind = r.text()?;
            let name = r.text()?;
            if !(matches!(kind.as_str(), "bgm" | "env" | "se")
                || bytes[4] >= 6 && matches!(kind.as_str(), "bgv1" | "bgv2"))
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
        let mut bgv_tags = BTreeMap::new();
        if bytes[4] >= 6 {
            for _ in 0..r.count(2)? {
                let kind = r.text()?;
                let tag = r.text()?;
                let profile = store.bgv_profile.as_ref().ok_or("saved BGV requires native engine contract")?;
                if !audio.contains_key(&kind) || !profile.channels.iter().any(|channel|
                    super::bgv::kind(channel.channel).is_ok_and(|value| value == kind)
                    && channel.tag.eq_ignore_ascii_case(&tag))
                    || bgv_tags.insert(kind, tag).is_some()
                {
                    return Err("invalid saved BGV role tag".into());
                }
            }
            if audio.keys().filter(|kind| kind.starts_with("bgv")).any(|kind| !bgv_tags.contains_key(kind)) {
                return Err("saved BGV has no role tag".into());
            }
        }
        let delayed_scene = if bytes[4] >= 7 {
            match r.number()? {
                0 => None,
                1 => Some(system_commands::Delay::load(&mut r, dimensions)?),
                _ => return Err("invalid saved delayed scene flag".into()),
            }
        } else { None };
        let mut system_state = BTreeMap::new();
        if bytes[4] >= 8 {
            for _ in 0..r.count(1)? {
                let name = r.text()?;
                if name != "ClimaxCount" { return Err("unknown saved original message state".into()); }
                system_state.insert(name, r.number()? as i64);
            }
        }
        let mut native_messages = Vec::new();
        let mut navigation_back = Vec::new();
        if bytes[4] >= 9 {
            for _ in 0..r.count(32768)? { native_messages.push(super::native_load::NativeMessage::load(&mut r)?); }
            for _ in 0..r.count(32768)? { navigation_back.push(r.bytes()?.to_vec()); }
            if selection && !navigation_back.is_empty() { return Err("selection checkpoint contains nested navigation".into()); }
        }
        // Old NKSV did not save original size/mask/climax fields or BACK snapshots.
        // Keep its actual text/voice history, and migrate absent presentation fields
        // to the original handler's initial values. No return point is invented.
        if native_messages.is_empty() {
            if let Some(history) = &history {
                native_messages = history.entries.iter().map(|entry| super::native_load::NativeMessage {
                    size: 30, name: entry.name.clone(), voice: entry.voice.clone(), mask: String::new(),
                    text: entry.text.clone(), climax: None,
                }).collect();
            }
        }
        if let Some(history) = &history {
            if native_messages.len() != history.entries.len() || native_messages.iter().zip(&history.entries)
                .any(|(native, entry)| native.name != entry.name || native.voice != entry.voice || native.text != entry.text) {
                return Err("original message history differs from runtime history".into());
            }
        }
        if let Some(h) = history.as_ref().filter(|_| !selection) {
            let last = h.entries.last().ok_or("empty saved message log")?;
            if last.name != speaker
                || last.voice != voice
                || Some(last.text.as_bytes()) != machine.pending_text()
            {
                return Err("saved log does not match dialogue".into());
            }
        }
        r.finish()?;
        for checkpoint in &navigation_back {
            Self::decode_kind(checkpoint, store, dimensions, with_audio, true)?;
        }
        let mut audio_effects = std::collections::VecDeque::new();
        let mut sounds = audio.clone();
        if !selection && !voice.is_empty() {
            sounds.insert("voice".into(), (voice.clone(), false));
        }
        for (kind, (name, looping)) in sounds.into_iter().filter(|_| with_audio) {
            let directory = match kind.as_str() {
                "bgm" => "sound/bgm",
                "env" => "sound/env",
                "se" => "sound/se",
                "bgv1" | "bgv2" => "sound/bgv",
                _ => "voice",
            };
            let payload = store.read(directory, &format!("{name}.ogg"))?;
            if !payload.starts_with(b"OggS") {
                return Err("invalid saved audio resource".into());
            }
            audio_effects.push_back(HostEffect {
                token: 0,
                audio_volume: if matches!(kind.as_str(), "bgm" | "env") { 50 } else { 100 },
                voice_tag: bgv_tags.get(&kind).cloned().unwrap_or_default(),
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
            selection,
            history,
            last_wipe_mask,
            delayed_scene,
            system_state,
            native_messages,
            navigation_back,
            page_effects,
            machine,
            scene,
            labels,
            speaker,
            voice,
            motions,
            elapsed,
            audio,
            bgv_tags,
            audio_effects,
        };
        saved.scene.render()?;
        Ok(saved)
    }
    pub(super) fn native_values<R: Read + Seek>(&self, date: &str, time: &str,
        store: &mut DatGame<R>, dimensions: [u32; 2]) -> Result<String, Box<dyn Error>> {
        let log = self.native_messages.iter().enumerate().map(|(index,entry)| entry.expression(self.history.as_ref()
            .map(|history| history.entries[index].face.as_str()).unwrap_or_default())).collect::<Result<Vec<_>, _>>()?.join(",");
        let back = self.native_back_expression(store, dimensions)?;
        Ok(format!("{{fields={{sdate={},stime={},route={},title={},text={},stamp={}}},globals={{MessageLog=[{}],ClimaxCount={},SkipBackData={}}}}}", quote(date.as_bytes())?, quote(time.as_bytes())?,
            quote(self.labels.route.as_bytes())?, quote(self.labels.title.as_bytes())?,
            quote(self.native_messages.last().map(|entry| entry.text.as_bytes()).unwrap_or_default())?,
            quote(format!("{date} {time}").as_bytes())?,
            log, self.system_state.get("ClimaxCount").copied().unwrap_or(0), back))
    }
    fn native_back_expression<R: Read + Seek>(&self, store: &mut DatGame<R>, dimensions: [u32; 2]) -> Result<String, Box<dyn Error>> {
        let mut entries = Vec::new();
        for bytes in &self.navigation_back {
            let checkpoint = Self::decode_kind(bytes, store, dimensions, false, true)?;
            let mut data = String::from("NKSC:");
            for byte in bytes { use std::fmt::Write; write!(&mut data, "{byte:02x}")?; }
            entries.push(format!("{{pos={},data={}}}", checkpoint.native_messages.len(), quote(data.as_bytes())?));
        }
        Ok(format!("[{}]", entries.join(",")))
    }
    pub(crate) fn preview(self, stamp: String) -> Result<crate::save_ui::Slot, Box<dyn Error>> {
        self.preview_size(stamp, [128,72])
    }
    pub(super) fn preview_size(mut self, stamp: String, size: [u32;2]) -> Result<crate::save_ui::Slot, Box<dyn Error>> {
        let mut motions = self.motions;
        let positions = motions
            .iter_mut()
            .map(|m| (m.slot, m.position(self.elapsed)))
            .collect::<Vec<_>>();
        let pixels = if let Some(delay) = self.delayed_scene.filter(|delay| delay.pending()) {
            delay.into_before()
        } else {
            self.scene.render_pixels(&positions)?
        };
        Ok(crate::save_ui::Slot {
            title: self.labels.title,
            route: self.labels.route,
            stamp,
            text: self.history.as_ref()
                .and_then(|history| history.entries.last())
                .map(|entry| entry.text.clone())
                .unwrap_or_default(),
            thumbnail: if size == [128,72] { crate::save_ui::thumbnail(&pixels)? }
                else { super::native_load::thumbnail(&pixels, size)? },
        })
    }
}
