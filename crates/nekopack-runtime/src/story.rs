//! Cooperative story presentation. Explicit host mode waits for effect acknowledgements.
use crate::{
    array, bootstrap,
    dat::DatGame,
    quote,
    scene::{plan, Layer, Scene},
    GameResources,
};
use nekopack_image::{decode_png, Mng};
use nekopack_script::{
    runtime::{Machine, Request, Step},
    system::SystemVm,
};
use std::collections::BTreeMap;
use std::{
    error::Error,
    io::{Read, Seek},
};
const PIXELS: usize = 64 * 1024 * 1024;
const AUDIO_CONFIG_FIELDS: [(&str, &str); 5] = [
    ("bgm", "VolBGM"),
    ("voice", "VolVO"),
    ("se", "VolSE"),
    ("env", "VolENV"),
    ("system", "VolSYS"),
];
/// Save-list labels owned by the original system script, not scenario variables.
/// Metadata only: this is not a resumable VM/scene save file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StoryLabels {
    pub title: String,
    pub route: String,
    pub caption: String,
    pub in_route: bool,
}
impl StoryLabels {
    fn capture(system: &mut SystemVm) -> Result<Self, String> {
        fn read(system: &mut SystemVm, expression: &str) -> Result<String, String> {
            system.eval("story-labels", &format!("return {expression}"))?;
            Ok(system.result().to_owned())
        }
        Ok(Self {
            title: read(system, "StoryTitle")?,
            route: read(system, "StoryRoute")?,
            caption: read(system, "Host.caption")?,
            in_route: read(system, "InRoute() ? \"1\" : \"0\"")? == "1",
        })
    }
    fn metadata(&self) -> Result<String, String> {
        Ok(format!(
            "{{\"title\":{},\"route\":{},\"caption\":{},\"in_route\":{}}}",
            quote(self.title.as_bytes())?,
            quote(self.route.as_bytes())?,
            quote(self.caption.as_bytes())?,
            self.in_route
        ))
    }
}
pub struct Page {
    pub motion: Option<String>,
    pub labels: StoryLabels,
    pub sequence: u64,
    pub source: String,
    pub line: usize,
    pub speaker: String,
    pub voice: String,
    pub text: String,
    pub effects: Vec<String>,
    pub png: Vec<u8>,
    pub scene: String,
    pub depth: usize,
    pub steps: usize,
    pub slots: Vec<(u8, u32, i32)>,
}
impl Page {
    pub fn metadata(&self) -> Result<String, String> {
        let slots = self
            .slots
            .iter()
            .map(|(b, i, v)| format!("[{}, {i}, {v}]", b))
            .collect::<Vec<_>>()
            .join(",");
        let motion = self
            .motion
            .as_ref()
            .map(|m| format!(",\"motion\":{m}"))
            .unwrap_or_default();
        Ok(format!("{{\"sequence\":{},\"source\":{},\"line\":{},\"speaker\":{},\"voice\":{},\"text\":{},\"effects\":{},\"scene\":{},\"depth\":{},\"steps\":{},\"slots\":[{}],\"labels\":{}{motion}}}",self.sequence,quote(self.source.as_bytes())?,self.line,quote(self.speaker.as_bytes())?,quote(self.voice.as_bytes())?,quote(self.text.as_bytes())?,array(&self.effects.iter().map(|s|s.as_bytes().to_vec()).collect::<Vec<_>>())?,self.scene,self.depth,self.steps,slots,self.labels.metadata()?))
    }
    pub fn packet(&self) -> Result<Vec<u8>, String> {
        let mut bytes = self.metadata()?.into_bytes();
        bytes.push(b'\n');
        bytes.extend(&self.png);
        Ok(bytes)
    }
}

pub struct HostEffect {
    pub token: u64,
    pub kind: String,
    pub name: String,
    pub duration_ms: u64,
    pub movie_left_skip: bool,
    pub audio_loop: bool,
    pub payload: Vec<u8>,
}
impl HostEffect {
    pub fn packet(&self) -> Result<Vec<u8>, String> {
        self.packet_checkpoint(None)
    }
    fn packet_checkpoint(&self, checkpoint: Option<&str>) -> Result<Vec<u8>, String> {
        let mut extra = checkpoint
            .map(|c| format!(",\"checkpoint\":{c}"))
            .unwrap_or_default();
        if matches!(self.kind.as_str(), "bgm" | "env") {
            extra.push_str(&format!(
                ",\"loop\":{},\"volume_percent\":50",
                self.audio_loop
            ));
        }
        if self.kind == "se" {
            extra.push_str(&format!(
                ",\"loop\":{},\"volume_percent\":100",
                self.audio_loop
            ));
        }
        if self.kind == "movie" {
            extra.push_str(&format!(",\"movie_left_skip\":{}", self.movie_left_skip));
        }
        let mut result = format!(
            "{{\"host_effect\":{},\"token\":{},\"name\":{},\"duration_ms\":{}{extra}}}\n",
            quote(self.kind.as_bytes())?,
            self.token,
            quote(self.name.as_bytes())?,
            self.duration_ms
        )
        .into_bytes();
        result.extend(&self.payload);
        Ok(result)
    }
}

fn duration_ms(text: &str) -> Result<u64, String> {
    let (whole, fraction) = text.split_once('.').unwrap_or((text, ""));
    if whole.is_empty()
        || !whole.bytes().all(|b| b.is_ascii_digit())
        || fraction.len() > 3
        || !fraction.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(
            "invalid duration; expected nonnegative seconds with at most 3 decimal places".into(),
        );
    }
    let whole = whole.parse::<u64>().map_err(|_| "duration overflow")?;
    let fraction = if fraction.is_empty() {
        0
    } else {
        fraction.parse::<u64>().unwrap() * 10u64.pow(3 - fraction.len() as u32)
    };
    whole
        .checked_mul(1000)
        .and_then(|v| v.checked_add(fraction))
        .filter(|v| *v <= i32::MAX as u64)
        .ok_or("duration overflow".into())
}
pub struct StorySession<R: Read + Seek> {
    store: DatGame<R>,
    machine: Machine,
    system: SystemVm,
    labels: StoryLabels,
    scene: Scene,
    motions: Vec<crate::motion::Queue>,
    motion_elapsed: u64,
    presented: Option<Vec<u8>>,
    wipe: Option<crate::transition::Wipe>,
    shake: Option<crate::shake::Frames>,
    flash: Option<crate::flash::Frames>,
    image_macros: BTreeMap<String, Vec<String>>,
    page: Option<Page>,
    speaker: String,
    voice: String,
    sequence: u64,
    failed: bool,
    host_mode: bool,
    interactive_text: bool,
    text_reveal: Option<crate::dialogue::Reveal>,
    pending_effect: Option<HostEffect>,
    host_sequence: u64,
    effects: Vec<String>,
}
impl<R: Read + Seek> StorySession<R> {
    /// Read the original system Config; the VM remains the single source of truth.
    pub fn audio_config(&mut self) -> Result<Vec<u8>, Box<dyn Error>> {
        if self.failed {
            return Err("story session stopped after error".into());
        }
        let mut fields = Vec::new();
        for (key, field) in AUDIO_CONFIG_FIELDS {
            self.system
                .eval("audio-config", &format!("if(typeof Config.{field} != \"integer\")throw \"invalid audio configuration type\"\nreturn Config.{field}.tostring()"))?;
            let value: u8 = self.system.result().parse()?;
            if value > 100 {
                return Err(format!("invalid Config.{field}: {value}").into());
            }
            fields.push(format!("\"{key}\":{value}"));
        }
        Ok(format!("{{{}}}", fields.join(",")).into_bytes())
    }

    /// Session-local settings; this does not implement the original config.sav format.
    pub fn set_audio_config(
        &mut self,
        channel: u64,
        percent: u64,
    ) -> Result<Vec<u8>, Box<dyn Error>> {
        let (_, field) = AUDIO_CONFIG_FIELDS
            .get(usize::try_from(channel)?)
            .ok_or("unknown audio configuration channel")?;
        if percent > 100 {
            return Err("audio configuration percent exceeds 100".into());
        }
        self.audio_config()?;
        self.system.eval(
            "set-audio-config",
            &format!("Config.{field} = {percent}\nRestoreVolume()"),
        )?;
        self.audio_config()
    }

    pub fn open(mut store: DatGame<R>, choice: i32) -> Result<Self, Box<dyn Error>> {
        let (mut machine, mut system) = bootstrap(&mut store, choice, &mut std::io::sink())?;
        machine.enable_dialogue();
        let labels = StoryLabels::capture(&mut system)?;
        let title = store.read("system", "title.mng")?;
        let mng = Mng::parse(&title, 100000)?;
        let mut image_macros = BTreeMap::new();
        let table = String::from_utf8(store.system(b"imagemap.txt")?)?;
        for line in table
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with(';'))
        {
            let (key, value) = line.split_once('=').ok_or("invalid imagemap row")?;
            let key = key.trim().to_owned();
            let value = value.split_whitespace().map(str::to_owned).collect();
            if image_macros.insert(key, value).is_some() {
                return Err("duplicate imagemap macro".into());
            }
        }
        Ok(Self {
            store,
            machine,
            system,
            labels,
            scene: Scene::new([mng.header[0], mng.header[1]]),
            motions: Vec::new(),
            motion_elapsed: 0,
            presented: None,
            wipe: None,
            shake: None,
            flash: None,
            image_macros,
            page: None,
            speaker: String::new(),
            voice: String::new(),
            sequence: 0,
            failed: false,
            host_mode: false,
            interactive_text: false,
            text_reveal: None,
            pending_effect: None,
            host_sequence: 0,
            effects: Vec::new(),
        })
    }
    /// Polling retains the exact pending text. Only a matching sequence acknowledges it.
    pub fn page(&mut self) -> Result<&Page, Box<dyn Error>> {
        if self.failed {
            return Err("story session stopped after error".into());
        }
        if self.page.is_none() && self.pending_effect.is_none() {
            if let Err(e) = self.advance() {
                return Err(self.stop(e));
            }
        }
        self.page
            .as_ref()
            .ok_or_else(|| "host effect requires acknowledgement".into())
    }
    pub fn next(&mut self, sequence: u64) -> Result<&Page, Box<dyn Error>> {
        if self.failed {
            return Err("story session stopped after error".into());
        }
        if self.page.as_ref().map(|p| p.sequence) != Some(sequence) {
            return Err("stale or missing dialogue acknowledgement".into());
        }
        if self.interactive_text && !self.text_reveal.as_ref().is_some_and(|r| r.advance) {
            return Err("dialogue advance was not granted".into());
        }
        self.text_reveal = None;
        self.machine.complete(None)?;
        self.page = None;
        self.motions.clear();
        self.motion_elapsed = 0;
        self.speaker.clear();
        self.voice.clear();
        self.page()
    }

    /// Enable once before polling; headless fixtures retain their explicit static profile.
    pub fn enable_host_effects(&mut self) {
        self.host_mode = true;
    }
    pub fn enable_interactive_text(&mut self) {
        self.interactive_text = true;
    }
    pub fn text_config(&mut self) -> Result<Vec<u8>, Box<dyn Error>> {
        let (speed, keep) = self.read_text_config()?;
        Ok(format!("{{\"speed\":{speed},\"keep_voice\":{keep}}}").into_bytes())
    }
    fn read_text_config(&mut self) -> Result<(u64, u64), Box<dyn Error>> {
        if self.failed {
            return Err("story session stopped after error".into());
        }
        let mut values = Vec::new();
        for field in ["TextSpeed", "KeepVoice"] {
            self.system.eval("text-config",&format!("if(typeof Config.{field} != \"integer\")throw \"invalid text configuration type\"\nreturn Config.{field}.tostring()"))?;
            values.push(self.system.result().parse::<u64>()?);
        }
        if values[0] > 100 || values[1] > 1 {
            return Err(
                "unsupported TextSpeed/KeepVoice; supported ranges are 0..100 and 0..1".into(),
            );
        }
        Ok((values[0], values[1]))
    }
    pub fn set_text_config(&mut self, field: u64, value: u64) -> Result<Vec<u8>, Box<dyn Error>> {
        let name = match field {
            0 if value <= 100 => "TextSpeed",
            1 if value <= 1 => "KeepVoice",
            _ => return Err("invalid text configuration update".into()),
        };
        self.read_text_config()?;
        self.system
            .eval("set-text-config", &format!("Config.{name} = {value}"))?;
        self.text_config()
    }
    /// Poll/click never advances the script. A completed-page click grants one ACK.
    pub fn text_frame(
        &mut self,
        sequence: u64,
        elapsed: u64,
        click: bool,
    ) -> Result<Vec<u8>, Box<dyn Error>> {
        if self.failed
            || !self.interactive_text
            || self.page.as_ref().map(|p| p.sequence) != Some(sequence)
        {
            return Err("stale or missing interactive dialogue".into());
        }
        if self.text_reveal.is_none() {
            if elapsed != 0 || click {
                return Err("dialogue must begin with a zero-time frame".into());
            }
            let (speed, keep) = self.read_text_config()?;
            self.text_reveal = Some(crate::dialogue::Reveal::new(
                self.page.as_ref().unwrap().text.clone(),
                speed,
                keep,
            )?);
        }
        // The original reads KeepVoice at NEXT, while TextSpeed is captured on entry.
        let keep = if click {
            Some(self.read_text_config()?.1 == 1)
        } else {
            None
        };
        let state = self.text_reveal.as_mut().unwrap();
        if let Some(keep) = keep {
            state.keep_voice = keep;
        }
        let advance = state.update(elapsed, click)?;
        Ok(format!("{{\"sequence\":{sequence},\"elapsed_ms\":{elapsed},\"visible\":{},\"text\":{},\"complete\":{},\"advance\":{advance},\"stop_voice\":{}}}",state.count(),quote(state.visible().as_bytes())?,state.complete(),advance&&!state.keep_voice&&!self.page.as_ref().unwrap().voice.is_empty()).into_bytes())
    }
    pub fn poll_packet(&mut self) -> Result<Vec<u8>, Box<dyn Error>> {
        if self.failed {
            return Err("story session stopped after error".into());
        }
        if self.page.is_none() && self.pending_effect.is_none() {
            if let Err(e) = self.advance() {
                return Err(self.stop(e));
            }
        }
        if let Some(effect) = &self.pending_effect {
            let checkpoint = if matches!(effect.kind.as_str(), "shake" | "wipe" | "fade" | "flash")
            {
                Some(self.checkpoint()?)
            } else {
                None
            };
            Ok(effect.packet_checkpoint(checkpoint.as_deref())?)
        } else {
            Ok(self
                .page
                .as_ref()
                .ok_or("no pending story event")?
                .packet()?)
        }
    }
    pub fn acknowledge_effect(&mut self, token: u64) -> Result<Vec<u8>, Box<dyn Error>> {
        if self
            .pending_effect
            .as_ref()
            .is_some_and(|e| e.kind == "movie")
        {
            return Err("movie requires completion or explicit skip reason".into());
        }
        self.complete_effect(token)
    }
    pub fn acknowledge_movie(
        &mut self,
        token: u64,
        reason: u64,
    ) -> Result<Vec<u8>, Box<dyn Error>> {
        let e = self.pending_effect.as_ref().ok_or("no movie request")?;
        if e.kind != "movie"
            || e.token != token
            || reason > 2
            || (reason == 1 && !e.movie_left_skip)
        {
            return Err("invalid movie completion or disabled primary skip".into());
        }
        self.complete_effect(token)
    }
    fn complete_effect(&mut self, token: u64) -> Result<Vec<u8>, Box<dyn Error>> {
        if self.failed {
            return Err("story session stopped after error".into());
        }
        if self.pending_effect.as_ref().map(|e| e.token) != Some(token) {
            return Err("stale or missing host acknowledgement".into());
        }
        if self.shake.as_ref().is_some_and(|s| !s.finished()) {
            return Err("shake acknowledged before final frame".into());
        }
        if self.flash.as_ref().is_some_and(|f| !f.finished()) {
            return Err("flash acknowledged before final phase".into());
        }
        self.machine.complete(None)?;
        let effect = self.pending_effect.take().unwrap();
        self.flash = None;
        if let Some(shake) = self.shake.take() {
            self.presented = Some(shake.into_png()?);
        }
        if matches!(effect.kind.as_str(), "fade" | "wipe" | "flash") {
            self.presented = Some(effect.payload);
        }
        self.wipe = None;
        self.poll_packet()
    }
    pub fn acknowledge_text(&mut self, sequence: u64) -> Result<Vec<u8>, Box<dyn Error>> {
        if self.failed {
            return Err("story session stopped after error".into());
        }
        if self.page.as_ref().map(|p| p.sequence) != Some(sequence) {
            return Err("stale or missing dialogue acknowledgement".into());
        }
        if self.interactive_text && !self.text_reveal.as_ref().is_some_and(|r| r.advance) {
            return Err("dialogue advance was not granted".into());
        }
        self.text_reveal = None;
        self.machine.complete(None)?;
        self.page = None;
        self.motions.clear();
        self.motion_elapsed = 0;
        self.speaker.clear();
        self.voice.clear();
        self.poll_packet()
    }
    /// Concurrent dialogue animation: never completes a script request or changes the cached page.
    pub fn page_frame(
        &mut self,
        sequence: u64,
        elapsed_ms: u64,
    ) -> Result<Vec<u8>, Box<dyn Error>> {
        if self.failed
            || self.page.as_ref().map(|p| p.sequence) != Some(sequence)
            || self.motions.is_empty()
        {
            return Err("stale or missing dialogue motion request".into());
        }
        if elapsed_ms < self.motion_elapsed {
            return Err("invalid dialogue motion clock".into());
        }
        let mut motions = self.motions.clone();
        let positions = motions
            .iter_mut()
            .map(|m| (m.slot, m.position(elapsed_ms)))
            .collect::<Vec<_>>();
        let frame = self.scene.render_pixels(&positions)?;
        let finished = motions.iter().all(|m| m.finished());
        self.motions = motions;
        self.motion_elapsed = elapsed_ms;
        let mut packet = format!("{{\"frame_kind\":\"motion\",\"sequence\":{sequence},\"elapsed_ms\":{elapsed_ms},\"finished\":{finished},\"width\":{},\"height\":{},\"format\":\"RGBA8\"}}\n", frame.width(), frame.height()).into_bytes();
        packet.extend_from_slice(frame.pixels());
        Ok(packet)
    }
    pub fn flash_frame(&mut self, token: u64, phase: u64) -> Result<Vec<u8>, Box<dyn Error>> {
        if self.failed
            || self
                .pending_effect
                .as_ref()
                .map(|e| (e.token, e.kind.as_str()))
                != Some((token, "flash"))
        {
            return Err("stale or missing flash frame request".into());
        }
        let flash = self.flash.as_mut().ok_or("missing flash state")?;
        let (width, height) = flash.dimensions();
        let pixels = flash.frame(phase)?;
        let mut packet=format!("{{\"frame_kind\":\"flash\",\"token\":{token},\"phase\":{phase},\"width\":{width},\"height\":{height},\"format\":\"RGBA8\"}}\n").into_bytes();
        packet.extend_from_slice(pixels);
        Ok(packet)
    }
    pub fn effect_frame(&mut self, token: u64, elapsed_ms: u64) -> Result<Vec<u8>, Box<dyn Error>> {
        if self.failed || self.pending_effect.as_ref().map(|e| e.token) != Some(token) {
            return Err("stale or missing effect frame request".into());
        }
        if let Some(shake) = &mut self.shake {
            let (width, height) = shake.dimensions();
            let pixels = shake.frame(elapsed_ms)?;
            let mut packet=format!("{{\"frame_kind\":\"shake\",\"token\":{token},\"elapsed_ms\":{elapsed_ms},\"width\":{width},\"height\":{height},\"format\":\"RGBA8\"}}\n").into_bytes();
            packet.extend_from_slice(pixels);
            Ok(packet)
        } else {
            self.wipe_frame(token, elapsed_ms)
        }
    }
    pub fn wipe_frame(&self, token: u64, elapsed_ms: u64) -> Result<Vec<u8>, Box<dyn Error>> {
        if self.failed
            || self
                .pending_effect
                .as_ref()
                .map(|e| (e.token, e.kind.as_str()))
                != Some((token, "wipe"))
        {
            return Err("stale or missing wipe frame request".into());
        }
        let wipe = self.wipe.as_ref().ok_or("no wipe state")?;
        let frame = wipe.frame(elapsed_ms)?;
        let mut bytes=format!("{{\"wipe_frame\":{token},\"elapsed_ms\":{elapsed_ms},\"width\":{},\"height\":{},\"format\":\"RGBA8\"}}\n",frame.width(),frame.height()).into_bytes();
        bytes.extend_from_slice(frame.pixels());
        Ok(bytes)
    }
    fn emit(&mut self, kind: &str, name: &str) -> Result<(), Box<dyn Error>> {
        let (duration, payload) = match kind {
            "fade" => (duration_ms(name)?, self.render()?),
            "wait" | "bgm_fade" => (duration_ms(name)?, Vec::new()),
            "bgm_stop" | "env_stop" | "se_stop" => (0, Vec::new()),
            "bgm" | "env" | "se" | "voice" => {
                let directory = match kind {
                    "bgm" => "sound/bgm",
                    "env" => "sound/env",
                    "se" => "sound/se",
                    _ => "voice",
                };
                let bytes = self.store.read(directory, &format!("{name}.ogg"))?;
                if !bytes.starts_with(b"OggS") {
                    return Err("audio resource is not Ogg".into());
                }
                (0, bytes)
            }
            _ => return Err("unsupported host effect".into()),
        };
        self.host_sequence += 1;
        self.pending_effect = Some(HostEffect {
            token: self.host_sequence,
            kind: kind.into(),
            name: name.into(),
            duration_ms: duration,
            movie_left_skip: false,
            audio_loop: matches!(kind, "env" | "bgm"),
            payload,
        });
        Ok(())
    }
    fn stop(&mut self, error: Box<dyn Error>) -> Box<dyn Error> {
        self.failed = true;
        let (name, line) = self.machine.location().unwrap_or((b"<finished>", 0));
        format!(
            "story {}:{} depth={} steps={}: {error}",
            String::from_utf8_lossy(name),
            line,
            self.machine.depth(),
            self.machine.steps()
        )
        .into()
    }
    /// Read-only execution evidence, including commands since the last text page.
    pub fn checkpoint(&self) -> Result<String, String> {
        let (source, line) = self.machine.location().unwrap_or((b"<finished>", 0));
        let slots = self
            .machine
            .variables
            .slots()
            .iter()
            .map(|((b, i), v)| format!("[{b},{i},{v}]"))
            .collect::<Vec<_>>()
            .join(",");
        Ok(format!("{{\"source\":{},\"line\":{line},\"depth\":{},\"steps\":{},\"slots\":[{slots}],\"scene\":{},\"effects\":{},\"voice\":{},\"labels\":{}}}",quote(source)?,self.machine.depth(),self.machine.steps(),self.scene.metadata()?,array(&self.effects.iter().map(|s|s.as_bytes().to_vec()).collect::<Vec<_>>())?,quote(self.voice.as_bytes())?,self.labels.metadata()?))
    }
    fn render(&mut self) -> Result<Vec<u8>, Box<dyn Error>> {
        self.scene.render()
    }
    fn actor(&mut self, slot: u8, args: &[Vec<u8>]) -> Result<(), Box<dyn Error>> {
        let center = [
            i32::try_from(self.scene.dimensions[0] / 2)?,
            i32::try_from(self.scene.dimensions[1] / 2)?,
        ];
        let p = plan(args, &self.image_macros, center[0])?;
        if self.motions.iter().any(|m| m.slot == slot) {
            return Err(
                "multiple operations on a moving actor before dialogue are unsupported".into(),
            );
        }
        if p.clear {
            self.scene.actors.retain(|l| l.slot != slot);
            return Ok(());
        }
        let old = self.scene.actors.iter().find(|l| l.slot == slot);
        let name = p
            .name
            .clone()
            .or_else(|| old.map(|l| l.name.clone()))
            .ok_or("image operation requires an existing layer or resource")?;
        let folder = p.folder.clone().unwrap_or_else(|| {
            if p.name.is_some() {
                "image/actor".into()
            } else {
                old.unwrap().folder.clone()
            }
        });
        let mut anchor = old.map_or(center, |l| l.anchor);
        if let Some(x) = p.x {
            anchor[0] = x;
        }
        let mut segments = Vec::new();
        for kind in &p.motions {
            use crate::{motion::Motion, scene::MotionKind};
            let segment = match kind {
                MotionKind::Horizontal(dx) => Motion::new(slot, anchor, *dx, center)?,
                MotionKind::Dip => Motion::dip(slot, anchor, center)?,
                MotionKind::Stationary => Motion::stationary(slot, anchor, center)?,
                MotionKind::Excursion(dx) => {
                    Motion::horizontal_excursion(slot, anchor, *dx, center)?
                }
            };
            anchor = segment.end;
            segments.push(segment);
        }
        let motion = if segments.is_empty() {
            None
        } else {
            Some(crate::motion::Queue::new(slot, segments))
        };
        if p.name.is_none() && p.folder.is_none() {
            let index = self
                .scene
                .actors
                .iter()
                .position(|l| l.slot == slot)
                .unwrap();
            self.scene.actors[index].anchor = anchor;
            if p.order != crate::scene::Order::Keep {
                let layer = self.scene.actors.remove(index);
                self.scene.put(layer, p.order)?;
            }
            if let Some(m) = motion {
                self.motions.push(m);
            }
            return Ok(());
        }
        let bytes = self.store.read(&folder, &format!("{name}.png"))?;
        let image = decode_png(&bytes, PIXELS)?;
        let offset = png_position(&bytes)?;
        self.scene.put(
            Layer {
                filter: crate::filter::Filter::None,
                filter_pending: true,
                slot,
                name,
                folder,
                image,
                offset,
                anchor,
            },
            p.order,
        )?;
        if let Some(m) = motion {
            self.motions.push(m);
        }
        Ok(())
    }
    fn advance(&mut self) -> Result<(), Box<dyn Error>> {
        loop {
            match self.machine.advance(&mut self.store)? {
                Step::Finished => return Err("story ended".into()),
                Step::Request(Request::Speaker(fields)) => {
                    self.speaker = String::from_utf8(fields[0].clone())?;
                    self.voice = String::from_utf8(fields[1].clone())?;
                    if self.host_mode && !self.voice.is_empty() {
                        self.emit("voice", &self.voice.clone())?;
                        return Ok(());
                    }
                    self.machine.complete(None)?;
                }
                Step::Request(Request::Text(text)) => {
                    let png = self.render()?;
                    let (source, line) = self.machine.location().unwrap();
                    self.sequence += 1;
                    self.presented = Some(png.clone());
                    let motion = if self.motions.is_empty() {
                        None
                    } else {
                        Some(format!(
                            "{{\"duration_ms\":{},\"actors\":[{}]}}",
                            self.motions.iter().map(|m| m.duration()).max().unwrap(),
                            self.motions
                                .iter()
                                .map(|m| m.metadata())
                                .collect::<Vec<_>>()
                                .join(",")
                        ))
                    };
                    self.page = Some(Page {
                        motion,
                        labels: self.labels.clone(),
                        sequence: self.sequence,
                        source: String::from_utf8(source.to_vec())?,
                        line,
                        speaker: self.speaker.clone(),
                        voice: self.voice.clone(),
                        text: String::from_utf8(text)?,
                        effects: std::mem::take(&mut self.effects),
                        png,
                        scene: self.scene.metadata()?,
                        depth: self.machine.depth(),
                        steps: self.machine.steps(),
                        slots: self
                            .machine
                            .variables
                            .slots()
                            .iter()
                            .map(|((b, i), v)| (*b, *i, *v))
                            .collect(),
                    });
                    return Ok(());
                }
                Step::Request(Request::Command { name, args, .. }) => {
                    let command = std::str::from_utf8(&name)?;
                    if !self.motions.is_empty()
                        // Named BGM switches return before dialogue rendering in the original.
                        // Keep the queued motion across the host's audio-start acknowledgement.
                        && !(command == "bgm" && args.len() == 1)
                        && !matches!(
                            command,
                            "img1"
                                | "img2"
                                | "img3"
                                | "img4"
                                | "img5"
                                | "img6"
                                | "img7"
                                | "img8"
                                | "img9"
                        )
                    {
                        return Err(format!(
                            "unsupported command {command} between actor motion and dialogue"
                        )
                        .into());
                    }
                    match command {
                        "title" | "route" => {
                            self.system.eval_with_resources(
                                "story-label-command",
                                &format!(
                                    "com[{}].call(getroottable(),{})",
                                    quote(&name)?,
                                    array(&args)?
                                ),
                                &mut self.machine.variables,
                                &mut self.store,
                            )?;
                            self.labels = StoryLabels::capture(&mut self.system)?;
                        }
                        "filter" => {
                            self.scene.filter = crate::filter::Filter::parse(&args)?;
                        }
                        "img0" => {
                            if args.is_empty() {
                                self.scene.background = None;
                                self.scene.background_name = None;
                                self.scene.background_color = None;
                            } else if args.len() == 1 {
                                let name = std::str::from_utf8(&args[0])?;
                                if let Some(color) = solid(name) {
                                    self.scene.background = None;
                                    self.scene.background_name = None;
                                    self.scene.background_color = Some(color);
                                    self.scene.overlay = None;
                                    self.scene.visual = None;
                                } else {
                                    let bytes =
                                        self.store.read("image/back", &format!("{name}.png"))?;
                                    self.scene
                                        .set_background(name.into(), decode_png(&bytes, PIXELS)?)?;
                                }
                            } else {
                                return Err("unsupported background arguments".into());
                            }
                        }
                        "img1" | "img2" | "img3" | "img4" | "img5" | "img6" | "img7" | "img8"
                        | "img9" => self.actor(command.as_bytes()[3] - b'0', &args)?,
                        "imgx" => self.scene.reorder(&args)?,
                        "img" | "page" => {
                            if args.is_empty() {
                                self.scene.clear();
                            } else if command == "img" && args.len() == 1 {
                                let name = std::str::from_utf8(&args[0])?;
                                if let Some(color) = solid(name) {
                                    self.scene.overlay = Some(color);
                                    self.scene.visual = None;
                                } else {
                                    let bytes =
                                        self.store.read("image/visual", &format!("{name}.png"))?;
                                    if png_position(&bytes)? != [0, 0] {
                                        return Err("unsupported visual offsets".into());
                                    }
                                    self.scene
                                        .set_visual(name.into(), decode_png(&bytes, PIXELS)?)?;
                                }
                            } else {
                                return Err("unsupported clear/overlay arguments".into());
                            }
                        }
                        "movie" => {
                            if args.is_empty()
                                || args.len() > 2
                                || (args.len() == 2 && args[1] != b"skip")
                            {
                                return Err("unsupported movie arguments".into());
                            }
                            let name = std::str::from_utf8(&args[0])?;
                            if name.is_empty()
                                || name.contains(['/', '\\', ':'])
                                || name == "."
                                || name == ".."
                                || !name.to_ascii_lowercase().ends_with(".mpg")
                            {
                                return Err("unsupported movie resource name".into());
                            }
                            self.effects.push(format!("movie {name}"));
                            if self.host_mode {
                                self.host_sequence += 1;
                                self.pending_effect = Some(HostEffect {
                                    token: self.host_sequence,
                                    kind: "movie".into(),
                                    name: name.into(),
                                    duration_ms: 0,
                                    movie_left_skip: args.len() == 2,
                                    audio_loop: false,
                                    payload: Vec::new(),
                                });
                                return Ok(());
                            }
                        }
                        "env" => {
                            let (kind, name) = match args.as_slice() {
                                [] => ("env_stop", ""),
                                [name] => {
                                    let name = std::str::from_utf8(name)?;
                                    if name.is_empty()
                                        || ["fade", "loop", "once", "pan", "volume"].contains(&name)
                                    {
                                        return Err(
                                            "unsupported environment audio arguments".into()
                                        );
                                    }
                                    ("env", name)
                                }
                                _ => return Err("unsupported environment audio arguments".into()),
                            };
                            self.effects.push(if name.is_empty() {
                                kind.into()
                            } else {
                                format!("{kind} {name}")
                            });
                            if self.host_mode {
                                self.emit(kind, name)?;
                                return Ok(());
                            }
                        }
                        "bgm" if args.is_empty() => {
                            self.effects.push("bgm_stop".into());
                            if self.host_mode {
                                self.emit("bgm_stop", "")?;
                                return Ok(());
                            }
                        }
                        "bgm" if args.first().is_some_and(|a| a == b"fade") => {
                            let time = match args.as_slice() {
                                [_] => "4",
                                [_, duration] => std::str::from_utf8(duration)?,
                                _ => return Err("unsupported BGM fade arguments".into()),
                            };
                            duration_ms(time)?;
                            self.effects.push(format!("bgm fade {time}"));
                            if self.host_mode {
                                self.emit("bgm_fade", time)?;
                                return Ok(());
                            }
                        }
                        "flash" => {
                            let options = args
                                .iter()
                                .map(|a| std::str::from_utf8(a))
                                .collect::<Result<Vec<_>, _>>()?;
                            let spec = crate::flash::Flash::parse(options.iter().copied())?;
                            let normalized = format!(
                                "{:02x}{:02x}{:02x} {}",
                                spec.rgb()[0],
                                spec.rgb()[1],
                                spec.rgb()[2],
                                spec.count()
                            );
                            self.effects.push(format!("flash {normalized}"));
                            let target = self.render()?;
                            if self.host_mode {
                                self.flash = Some(crate::flash::Frames::new(
                                    spec,
                                    decode_png(&target, PIXELS)?,
                                )?);
                                self.host_sequence += 1;
                                self.pending_effect = Some(HostEffect {
                                    token: self.host_sequence,
                                    kind: "flash".into(),
                                    name: normalized,
                                    duration_ms: spec.duration_ms(),
                                    movie_left_skip: false,
                                    audio_loop: false,
                                    payload: target,
                                });
                                return Ok(());
                            }
                            self.presented = Some(target);
                        }
                        "shake" => {
                            let options = args
                                .iter()
                                .map(|a| std::str::from_utf8(a))
                                .collect::<Result<Vec<_>, _>>()?;
                            let motion = crate::shake::Shake::parse(options.iter().copied())?;
                            self.effects.push(format!(
                                "shake{}",
                                if options.is_empty() {
                                    String::new()
                                } else {
                                    format!(" {}", options.join(" "))
                                }
                            ));
                            if self.host_mode {
                                let target = self.render()?;
                                let old = match &self.presented {
                                    Some(png) => decode_png(png, PIXELS)?,
                                    None => nekopack_image::RgbaImage::filled(
                                        self.scene.dimensions[0],
                                        self.scene.dimensions[1],
                                        [0, 0, 0, 255],
                                        PIXELS,
                                    )?,
                                };
                                self.shake = Some(crate::shake::Frames::new(
                                    motion,
                                    decode_png(&target, PIXELS)?,
                                    old,
                                )?);
                                self.host_sequence += 1;
                                self.pending_effect = Some(HostEffect {
                                    token: self.host_sequence,
                                    kind: "shake".into(),
                                    name: options.join(" "),
                                    duration_ms: u64::from(motion.duration_ms()),
                                    movie_left_skip: false,
                                    audio_loop: false,
                                    payload: target,
                                });
                                return Ok(());
                            }
                        }
                        "wipe" => {
                            let mut name = None;
                            let mut duration = 500u64;
                            let mut soft = false;
                            for arg in &args {
                                let word = std::str::from_utf8(arg)?;
                                if word == "fade" {
                                    soft = true;
                                } else if !word.is_empty()
                                    && word.bytes().all(|c| c.is_ascii_digit())
                                {
                                    duration = word.parse::<u64>()?;
                                    if duration > i32::MAX as u64 {
                                        return Err("wipe duration overflow".into());
                                    }
                                } else if name.replace(word).is_some() {
                                    return Err("multiple wipe masks".into());
                                }
                            }
                            let name = name.ok_or("wipe requires a mask")?;
                            if duration == 0 {
                                duration = 500;
                            }
                            let bytes = self.store.read("image/mask", &format!("{name}.png"))?;
                            if png_position(&bytes)? != [0, 0] {
                                return Err("unsupported wipe mask offsets".into());
                            }
                            let mask = decode_png(&bytes, PIXELS)?;
                            let target = self.render()?;
                            let old = match &self.presented {
                                Some(png) => decode_png(png, PIXELS)?,
                                None => nekopack_image::RgbaImage::filled(
                                    self.scene.dimensions[0],
                                    self.scene.dimensions[1],
                                    [0, 0, 0, 255],
                                    PIXELS,
                                )?,
                            };
                            let wipe = crate::transition::Wipe::new(
                                old,
                                decode_png(&target, PIXELS)?,
                                mask,
                                soft,
                                duration,
                            )?;
                            self.effects.push(format!(
                                "wipe {name} {duration}{}",
                                if soft { " fade" } else { "" }
                            ));
                            if self.host_mode {
                                self.host_sequence += 1;
                                self.pending_effect = Some(HostEffect {
                                    token: self.host_sequence,
                                    kind: "wipe".into(),
                                    name: name.into(),
                                    duration_ms: duration,
                                    movie_left_skip: false,
                                    audio_loop: false,
                                    payload: target,
                                });
                                self.wipe = Some(wipe);
                                return Ok(());
                            }
                            self.presented = Some(target);
                        }
                        "fade" => {
                            let text = match args.as_slice() {
                                [] => "0",
                                [value] => std::str::from_utf8(value)?,
                                _ => return Err("unsupported fade arguments".into()),
                            };
                            let seconds = text.split('.').next().unwrap().parse::<i64>()?;
                            let normalized = if seconds <= 0 {
                                "0.5".to_owned()
                            } else {
                                seconds.to_string()
                            };
                            duration_ms(&normalized)?;
                            self.effects.push(format!("fade {normalized}"));
                            if self.host_mode {
                                self.emit("fade", &normalized)?;
                                return Ok(());
                            }
                            self.presented = Some(self.render()?);
                        }
                        "se" => {
                            if args.is_empty() {
                                self.effects.push("se_stop".into());
                                if self.host_mode {
                                    self.emit("se_stop", "")?;
                                    return Ok(());
                                }
                            } else {
                                let name = std::str::from_utf8(&args[0])?;
                                if name.is_empty()
                                    || ["loop", "once", "fade", "volume", "pan"].contains(&name)
                                {
                                    return Err("unsupported sound effect arguments".into());
                                }
                                let looping = match args.as_slice() {
                                    [_] => false,
                                    [_, option] if option == b"loop" => true,
                                    [_, option] if option == b"once" => false,
                                    _ => return Err("unsupported sound effect arguments".into()),
                                };
                                self.effects.push(format!(
                                    "se {name}{}",
                                    if looping { " loop" } else { "" }
                                ));
                                if self.host_mode {
                                    self.emit("se", name)?;
                                    self.pending_effect.as_mut().unwrap().audio_loop = looping;
                                    return Ok(());
                                }
                            }
                        }
                        "bgm" | "wait" => {
                            if args.len() != 1 {
                                return Err("preview effect requires one argument".into());
                            }
                            let name = std::str::from_utf8(&args[0])?;
                            self.effects.push(format!("{command} {name}"));
                            if self.host_mode {
                                self.emit(command, name)?;
                                return Ok(());
                            }
                        }
                        _ => {
                            return Err(
                                format!("unsupported story preview command {command}").into()
                            )
                        }
                    }
                    self.machine.complete(None)?;
                }
                Step::Request(Request::System(_)) => {
                    return Err("unexpected system module in story preview".into())
                }
            }
        }
    }
}
fn solid(name: &str) -> Option<[u8; 4]> {
    match name {
        "白" => Some([255; 4]),
        "黒" => Some([0, 0, 0, 255]),
        _ => None,
    }
}
/// PNG pixel offsets are the observed actor placement contract, not MNG timing.
fn png_position(bytes: &[u8]) -> Result<[i32; 2], Box<dyn Error>> {
    let mut at = 8;
    let mut position = [0, 0];
    while at < bytes.len() {
        let header = bytes.get(at..at + 8).ok_or("truncated PNG chunk")?;
        let length = u32::from_be_bytes(header[..4].try_into()?) as usize;
        let end = at
            .checked_add(length)
            .and_then(|n| n.checked_add(12))
            .ok_or("PNG chunk overflow")?;
        let chunk = bytes.get(at..end).ok_or("truncated PNG chunk")?;
        if &header[4..] == b"oFFs" {
            if length != 9 || chunk[16] != 0 {
                return Err("unsupported PNG offsets".into());
            }
            position = [
                i32::from_be_bytes(chunk[8..12].try_into()?),
                i32::from_be_bytes(chunk[12..16].try_into()?),
            ];
        }
        at = end;
    }
    Ok(position)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nekopack_image::RgbaImage;
    use nekopack_script::runtime::Source;
    fn session(text: &[u8]) -> StorySession<std::io::Cursor<Vec<u8>>> {
        let mut bytes = b"NEKOPACK".to_vec();
        for n in [0u32, 0, 0, 8, 0, 0] {
            bytes.extend(n.to_le_bytes());
        }
        let store = DatGame::mount([std::io::Cursor::new(bytes)]).unwrap();
        let mut machine = Machine::new(
            Source {
                name: b"test".to_vec(),
                bytes: text.to_vec(),
            },
            100,
            4,
        );
        machine.enable_dialogue();
        StorySession {
            store,
            machine,
            system: SystemVm::new(),
            labels: StoryLabels::default(),
            scene: Scene::new([1, 1]),
            motions: Vec::new(),
            motion_elapsed: 0,
            presented: None,
            wipe: None,
            shake: None,
            flash: None,
            image_macros: BTreeMap::new(),
            page: None,
            speaker: String::new(),
            voice: String::new(),
            sequence: 0,
            failed: false,
            host_mode: false,
            interactive_text: false,
            text_reveal: None,
            pending_effect: None,
            host_sequence: 0,
            effects: Vec::new(),
        }
    }

    #[test]
    fn interactive_text_requires_two_phase_input_and_preserves_cached_page() {
        let mut s = session("一二三四五六\nnext".as_bytes());
        s.system
            .eval("config", "Config <- {TextSpeed=0,KeepVoice=1}")
            .unwrap();
        s.enable_interactive_text();
        let packet = s.page().unwrap().packet().unwrap();
        let n = s.page.as_ref().unwrap().sequence;
        assert!(s.acknowledge_text(n).is_err());
        assert!(s.next(n).is_err());
        assert!(s.text_frame(n, 1, false).is_err());
        assert!(s.text_frame(n + 1, 0, false).is_err());
        s.text_frame(n, 0, false).unwrap();
        let first = String::from_utf8(s.text_frame(n, 100, true).unwrap()).unwrap();
        assert!(first.contains("\"complete\":true,\"advance\":false"));
        assert!(first.contains("\"stop_voice\":false"));
        assert_eq!(s.poll_packet().unwrap(), packet);
        assert!(s.acknowledge_text(n).is_err());
        // KeepVoice is consulted at NEXT, unlike the page-entry TextSpeed.
        s.page.as_mut().unwrap().voice = "voice".into();
        s.set_text_config(1, 0).unwrap();
        let second = String::from_utf8(s.text_frame(n, 100, true).unwrap()).unwrap();
        assert!(second.contains("\"advance\":true,\"stop_voice\":true"));
        assert!(s.text_frame(n, 100, true).is_err());
        s.acknowledge_text(n).unwrap();
        assert!(s.text_frame(n, 100, false).is_err());
        assert!(s.acknowledge_text(n + 1).is_err());
        s.text_frame(n + 1, 0, false).unwrap();
        assert!(s.text_frame(n + 1, 1, false).is_ok());
        assert!(s.text_frame(n + 1, 0, false).is_err());
        assert!(s.set_text_config(1, 2).is_err());
    }

    #[test]
    fn audio_config_uses_vm_state_and_rejects_invalid_updates_without_advancing() {
        let mut s = session(b"first\nsecond");
        s.system.eval("config", "Config <- {VolBGM=50,VolVO=75,VolSE=50,VolENV=50,VolSYS=50}\nfunction RestoreVolume(){HostVolume <- Config.VolBGM\n}").unwrap();
        let page = s.page().unwrap().packet().unwrap();
        assert_eq!(
            String::from_utf8(s.audio_config().unwrap()).unwrap(),
            "{\"bgm\":50,\"voice\":75,\"se\":50,\"env\":50,\"system\":50}"
        );
        s.set_audio_config(0, 23).unwrap();
        s.system
            .eval("check", "return HostVolume.tostring()")
            .unwrap();
        assert_eq!(s.system.result(), "23");
        let before = s.audio_config().unwrap();
        assert!(s.set_audio_config(5, 50).is_err());
        assert!(s.set_audio_config(0, 101).is_err());
        assert!(s.set_audio_config(u64::MAX, 50).is_err());
        assert_eq!(s.audio_config().unwrap(), before);
        assert_eq!(s.page().unwrap().packet().unwrap(), page);
        s.set_audio_config(0, 0).unwrap();
        s.set_audio_config(0, 100).unwrap();
        s.system.eval("bad", "Config.VolVO = -1").unwrap();
        assert!(s.audio_config().is_err());
        s.system.eval("bad", "Config.VolVO = 50.0").unwrap();
        assert!(s.audio_config().is_err());
        s.system.eval("bad", "Config.VolVO = 101").unwrap();
        assert!(s.audio_config().is_err());
    }

    #[test]
    fn filter_changes_future_actor_loads_not_retained_pixels_or_background() {
        use crate::filter::Filter;
        let mut s = session(
            ".filter 夜\nfirst\n.filter\n.img1 center keep\nsecond\n.filter unknown".as_bytes(),
        );
        s.scene
            .set_background(
                "bg".into(),
                RgbaImage::filled(1, 1, [100, 110, 120, 255], 4).unwrap(),
            )
            .unwrap();
        s.page().unwrap();
        assert_eq!(s.scene.filter, Filter::Night);
        assert_eq!(
            decode_png(&s.render().unwrap(), 4).unwrap().pixels(),
            [100, 110, 120, 255]
        );
        let mut image = RgbaImage::filled(1, 1, [100, 110, 120, 255], 4).unwrap();
        s.scene.filter.apply(&mut image);
        s.scene
            .put(
                Layer {
                    filter: Filter::Night,
                    filter_pending: false,
                    slot: 1,
                    name: "retained".into(),
                    folder: "image/actor".into(),
                    image,
                    offset: [0, 0],
                    anchor: [0, 0],
                },
                crate::scene::Order::Front,
            )
            .unwrap();
        let before = s.render().unwrap();
        assert_eq!(before, s.render().unwrap());
        s.next(1).unwrap();
        assert_eq!(s.scene.filter, Filter::None);
        assert_eq!(s.scene.actors[0].filter, Filter::Night);
        assert_eq!(before, s.render().unwrap());
        assert!(s.next(2).is_err());
        assert_eq!(s.scene.filter, Filter::None);
    }

    #[test]
    fn route_dispatch_retains_system_state_across_movie_and_text() {
        let mut s = session(".route before\n.movie op.mpg\n.route after ignored\n.title chapter\ntext\n.route\nnext\n.unknown".as_bytes());
        s.system
            .eval(
                "test-commands",
                r#"
            StoryRoute <- ""
            StoryTitle <- ""
            Host <- {caption=""}
            com <- {
                route=function(args) { StoryRoute=args.len() ? args[0] : "" },
                title=function(args) { StoryTitle=args[0]; Host.caption=StoryRoute+":"+StoryTitle }
            }
            function InRoute() { return StoryRoute!="" }
        "#,
            )
            .unwrap();
        s.enable_host_effects();
        let packet = s.poll_packet().unwrap();
        assert_eq!(s.labels.route, "before");
        let checkpoint = s.checkpoint().unwrap();
        assert_eq!(packet, s.poll_packet().unwrap());
        assert_eq!(checkpoint, s.checkpoint().unwrap());
        assert!(s.acknowledge_movie(1, 1).is_err());
        s.acknowledge_movie(1, 0).unwrap();
        let labels = &s.page.as_ref().unwrap().labels;
        assert_eq!(labels.route, "after");
        assert_eq!(labels.title, "chapter");
        assert_eq!(labels.caption, "after:chapter");
        assert!(labels.in_route);
        s.acknowledge_text(1).unwrap();
        assert_eq!(s.labels.route, "");
        assert_eq!(s.labels.title, "chapter");
        assert!(!s.labels.in_route);
        assert!(s.acknowledge_text(2).is_err());
        assert_eq!(s.machine.location().unwrap().1, 8);
    }

    #[test]
    fn flash_cannot_skip_phases_or_complete_early() {
        let mut s = session(".flash 青 2\ntext".as_bytes());
        s.enable_host_effects();
        let packet = s.poll_packet().unwrap();
        let before = s.checkpoint().unwrap();
        assert!(s.flash_frame(2, 0).is_err());
        assert!(s.flash_frame(1, 1).is_err());
        assert!(s.acknowledge_effect(1).is_err());
        let frame = s.flash_frame(1, 0).unwrap();
        assert_eq!(s.flash_frame(1, 0).unwrap(), frame);
        assert_eq!(s.poll_packet().unwrap(), packet);
        assert_eq!(s.checkpoint().unwrap(), before);
        for phase in 1..4 {
            s.flash_frame(1, phase).unwrap();
        }
        s.acknowledge_effect(1).unwrap();
        assert!(s.flash.is_none());
        assert!(s.flash_frame(1, 3).is_err());
    }
    #[test]
    fn shake_frames_require_matching_token_and_completion_without_advancing_pc() {
        let mut s = session(b".shake once\ntext");
        s.enable_host_effects();
        let pending = s.poll_packet().unwrap();
        let before = s.checkpoint().unwrap();
        assert_eq!(s.poll_packet().unwrap(), pending);
        assert!(s.acknowledge_effect(1).is_err());
        assert!(s.effect_frame(2, 7).is_err());
        let frame = s.effect_frame(1, 7).unwrap();
        assert_eq!(s.effect_frame(1, 7).unwrap(), frame);
        assert!(s.effect_frame(1, 6).is_err());
        assert_eq!(s.checkpoint().unwrap(), before);
        s.effect_frame(1, 50).unwrap();
        s.acknowledge_effect(1).unwrap();
        assert!(s.shake.is_none());
        assert!(s.effect_frame(1, 50).is_err());
        assert!(s.acknowledge_effect(1).is_err());
    }
    #[test]
    fn host_wait_requires_matching_completion_before_text() {
        let mut s = session(b".wait 0.125\nfirst\n.wait 0\nsecond");
        s.enable_host_effects();
        let pending = s.poll_packet().unwrap();
        assert!(String::from_utf8(pending.clone())
            .unwrap()
            .contains("\"duration_ms\":125"));
        let location = s.machine.location().unwrap().1;
        let budget = s.machine.steps();
        assert_eq!(s.poll_packet().unwrap(), pending);
        assert_eq!(s.machine.steps(), budget);
        assert!(s.acknowledge_text(1).is_err());
        assert!(s.acknowledge_effect(9).is_err());
        assert_eq!(s.machine.location().unwrap().1, location);
        let page = s.acknowledge_effect(1).unwrap();
        assert_eq!(s.poll_packet().unwrap(), page);
        assert!(s.acknowledge_effect(1).is_err());
        let second = s.acknowledge_text(1).unwrap();
        assert!(String::from_utf8(second)
            .unwrap()
            .contains("\"host_effect\":\"wait\""));
        let text = s.acknowledge_effect(2).unwrap();
        assert!(String::from_utf8_lossy(&text).contains("second"));
    }
    #[test]
    fn clearing_slots_preserves_other_layers_and_clear_all_releases_them() {
        let mut s = session(b".img1\nfirst\n.img\nsecond");
        for slot in [1, 2] {
            s.scene.actors.push(Layer {
                filter: crate::filter::Filter::None,
                filter_pending: false,
                slot,
                name: "test".into(),
                folder: "image/actor".into(),
                image: RgbaImage::filled(1, 1, [255; 4], 4).unwrap(),
                offset: [0, 0],
                anchor: [0, 0],
            });
        }
        s.page().unwrap();
        assert_eq!(
            s.scene.actors.iter().map(|l| l.slot).collect::<Vec<_>>(),
            [2]
        );
        s.next(1).unwrap();
        assert!(s.scene.actors.is_empty());
    }
    #[test]
    fn movie_waits_and_validates_skip_reason_without_loading_dat_audio() {
        for (script, primary) in [
            (".movie op.mpg\ntext", false),
            (".movie op.mpg skip\ntext", true),
        ] {
            let mut s = session(script.as_bytes());
            s.enable_host_effects();
            let packet = s.poll_packet().unwrap();
            let before = s.checkpoint().unwrap();
            assert!(s.acknowledge_effect(1).is_err());
            assert!(s.acknowledge_movie(2, 0).is_err());
            assert!(s.acknowledge_movie(1, 3).is_err());
            assert_eq!(s.poll_packet().unwrap(), packet);
            assert_eq!(s.checkpoint().unwrap(), before);
            if primary {
                s.acknowledge_movie(1, 1).unwrap();
            } else {
                assert!(s.acknowledge_movie(1, 1).is_err());
                s.acknowledge_movie(1, 2).unwrap();
            }
            assert!(s.page.is_some());
            assert!(s.acknowledge_movie(1, 0).is_err());
        }
        let mut s = session(b".movie op.mpg\ntext");
        s.enable_host_effects();
        s.poll_packet().unwrap();
        s.acknowledge_movie(1, 0).unwrap();
        assert!(s.page.is_some());
        let mut s = session(b".movie ../op.mpg\ntext");
        assert!(s.poll_packet().is_err());
    }
    #[test]
    fn each_movie_completion_resumes_only_its_own_pending_request() {
        let mut s = session(b".movie op.mpg\n.movie op.mpg skip\ntext\n.unknown");
        s.enable_host_effects();
        s.poll_packet().unwrap();
        let second = s.acknowledge_movie(1, 0).unwrap();
        assert_eq!(s.machine.location().unwrap().1, 2);
        assert!(s.acknowledge_movie(1, 2).is_err());
        assert_eq!(s.poll_packet().unwrap(), second);
        s.acknowledge_movie(2, 1).unwrap();
        assert_eq!(s.page.as_ref().unwrap().text, "text");
        assert!(s.acknowledge_movie(2, 0).is_err());
        assert!(s.acknowledge_text(1).is_err());
        assert!(s.failed);
        assert!(s.acknowledge_movie(2, 0).is_err());
    }
    #[test]
    fn bare_bgm_stops_without_resource_and_waits_for_host_confirmation() {
        let mut s = session(b".bgm\n.bgm\ntext");
        s.enable_host_effects();
        let packet = s.poll_packet().unwrap();
        let before = s.checkpoint().unwrap();
        let effect = s.pending_effect.as_ref().unwrap();
        assert_eq!(
            (
                effect.kind.as_str(),
                effect.name.as_str(),
                effect.duration_ms
            ),
            ("bgm_stop", "", 0)
        );
        assert!(effect.payload.is_empty());
        assert!(s.acknowledge_effect(2).is_err());
        assert_eq!(s.poll_packet().unwrap(), packet);
        assert_eq!(s.checkpoint().unwrap(), before);
        s.acknowledge_effect(1).unwrap();
        assert_eq!(s.pending_effect.as_ref().unwrap().token, 2);
        assert!(s.acknowledge_effect(1).is_err());
        s.acknowledge_effect(2).unwrap();
        assert!(s.pending_effect.is_none());
        assert!(s.page.is_some());
    }
    #[test]
    fn bgm_fade_requests_explicit_and_original_default_duration() {
        let mut s = session(b".bgm fade 1\ntext\n.bgm fade\nnever");
        s.enable_host_effects();
        let packet = s.poll_packet().unwrap();
        assert!(String::from_utf8_lossy(&packet).contains("\"host_effect\":\"bgm_fade\""));
        assert_eq!(s.poll_packet().unwrap(), packet);
        assert!(s.acknowledge_effect(2).is_err());
        s.acknowledge_effect(1).unwrap();
        let packet = s.acknowledge_text(1).unwrap();
        assert!(String::from_utf8_lossy(&packet).contains("\"duration_ms\":4000"));
        assert!(s.wipe_frame(2, 0).is_err());
        s.acknowledge_effect(2).unwrap();
    }
    #[test]
    fn original_default_fade_and_wipe_poll_contract() {
        let mut s = session(b".fade\n.fade 2.5\ntext");
        s.enable_host_effects();
        assert!(String::from_utf8_lossy(&s.poll_packet().unwrap()).contains("\"duration_ms\":500"));
        assert!(String::from_utf8_lossy(&s.acknowledge_effect(1).unwrap())
            .contains("\"duration_ms\":2000"));
        let mut s = session(b".wipe MASK 100\ntext");
        s.enable_host_effects();
        s.machine.advance(&mut s.store).unwrap();
        let image = || RgbaImage::filled(1, 1, [255; 4], 4).unwrap();
        s.wipe = Some(crate::transition::Wipe::new(image(), image(), image(), false, 100).unwrap());
        s.pending_effect = Some(HostEffect {
            token: 7,
            kind: "wipe".into(),
            name: "MASK".into(),
            duration_ms: 100,
            movie_left_skip: false,
            audio_loop: false,
            payload: vec![],
        });
        let before = s.machine.steps();
        assert!(s.wipe_frame(8, 50).is_err());
        let frame = s.wipe_frame(7, 50).unwrap();
        assert_eq!(frame, s.wipe_frame(7, 50).unwrap());
        s.wipe_frame(7, u64::MAX).unwrap();
        assert_eq!(before, s.machine.steps());
        assert!(s.acknowledge_text(1).is_err());
        s.acknowledge_effect(7).unwrap();
        assert!(s.wipe.is_none());
        assert!(s.wipe_frame(7, 50).is_err());
    }
    #[test]
    fn bad_duration_and_missing_audio_are_terminal() {
        assert_eq!(duration_ms("2.5").unwrap(), 2500);
        for invalid in ["-1", "NaN", "inf", "1.0001", "99999999999999999999"] {
            assert!(duration_ms(invalid).is_err());
        }
        for source in [b".wait -1\ntext".as_slice(), b".bgm missing\ntext"] {
            let mut s = session(source);
            s.enable_host_effects();
            assert!(s.poll_packet().is_err());
            assert!(s.page.is_none());
            assert!(s.poll_packet().unwrap_err().to_string().contains("stopped"));
        }
    }
    #[test]
    fn page_poll_and_stale_input_do_not_advance() {
        let mut s = session(b":Alice:voice:A\nfirst\nsecond\n.unknown\nthird");
        let original = s.page().unwrap().packet().unwrap();
        assert_eq!(s.page().unwrap().packet().unwrap(), original);
        assert!(s.next(0).is_err());
        assert_eq!(s.page().unwrap().packet().unwrap(), original);
        let p = s.next(1).unwrap();
        assert_eq!(p.sequence, 2);
        assert_eq!(p.text, "second");
        assert_eq!(p.speaker, "");
        assert_eq!(p.voice, "");
        assert!(s.next(1).is_err());
        assert_eq!(s.page().unwrap().sequence, 2);
        assert!(s
            .next(2)
            .err()
            .unwrap()
            .to_string()
            .contains("unsupported story preview command"));
        assert!(s.page().err().unwrap().to_string().contains("stopped"));
    }
    fn moving_session(text: &[u8]) -> StorySession<std::io::Cursor<Vec<u8>>> {
        let mut s = session(text);
        s.scene.dimensions = [800, 1];
        s.scene.actors.push(Layer {
            filter: crate::filter::Filter::Night,
            filter_pending: false,
            slot: 6,
            name: "cached".into(),
            folder: "image/actor".into(),
            image: RgbaImage::filled(1, 1, [83, 88, 137, 255], PIXELS).unwrap(),
            offset: [0, 0],
            anchor: [400, 0],
        });
        s.enable_host_effects();
        s
    }
    #[test]
    fn page_finishes_only_after_all_actor_queues_finish() {
        let mut s = moving_session(b".img6 _ >>>>\n.img5 []\nfirst");
        s.scene.actors.push(Layer {
            filter: crate::filter::Filter::None,
            filter_pending: false,
            slot: 5,
            name: "other".into(),
            folder: "image/actor".into(),
            image: RgbaImage::filled(1, 1, [255, 0, 0, 255], PIXELS).unwrap(),
            offset: [0, 0],
            anchor: [10, 0],
        });
        s.poll_packet().unwrap();
        for (time, finished) in [(0, false), (270, false), (500, false), (520, true)] {
            let frame = s.page_frame(1, time).unwrap();
            let split = frame.iter().position(|b| *b == b'\n').unwrap();
            assert!(std::str::from_utf8(&frame[..split])
                .unwrap()
                .contains(&format!("\"finished\":{finished}")));
            assert_eq!(&frame[split + 1 + 40..split + 1 + 44], [255, 0, 0, 255]);
            assert_eq!(s.motions[1].finished(), time >= 250);
        }
        assert_eq!(
            s.scene.actors.iter().find(|l| l.slot == 6).unwrap().anchor,
            [600, 0]
        );
    }
    #[test]
    fn queued_segments_complete_in_core_without_advancing_dialogue() {
        for early in [false, true] {
            let mut s = moving_session(b".img6 _ >>>>\nfirst\nsecond");
            let page = s.poll_packet().unwrap();
            let steps = s.machine.steps();
            assert_eq!(s.scene.actors[0].anchor, [600, 0]);
            for (time, done) in [(0, false), (270, false), (500, false), (520, true)] {
                let frame = s.page_frame(1, time).unwrap();
                let header =
                    std::str::from_utf8(frame.split(|b| *b == b'\n').next().unwrap()).unwrap();
                assert!(header.contains(&format!("\"finished\":{done}")));
                assert_eq!(s.page_frame(1, time).unwrap(), frame);
                assert_eq!(s.poll_packet().unwrap(), page);
                assert_eq!(s.machine.steps(), steps);
                if early {
                    break;
                }
            }
            s.acknowledge_text(1).unwrap();
            assert_eq!(s.scene.actors[0].anchor, [600, 0]);
            assert!(s.motions.is_empty());
            assert!(s.page_frame(1, 999).is_err());
            assert_eq!(s.page.as_ref().unwrap().text, "second");
        }
    }
    #[test]
    fn leftward_queue_survives_poll_and_early_dialogue_completion() {
        for early in [true, false] {
            let mut s = moving_session(b".img6 <<<<< >\nfirst\nsecond");
            let packet = s.poll_packet().unwrap();
            let steps = s.machine.steps();
            s.page_frame(1, 125).unwrap();
            assert_eq!(s.poll_packet().unwrap(), packet);
            if !early {
                s.page_frame(1, 250).unwrap();
                assert!(!s.motions[0].finished());
                s.page_frame(1, 500).unwrap();
                assert!(s.motions[0].finished());
            }
            assert_eq!(s.machine.steps(), steps);
            s.acknowledge_text(1).unwrap();
            assert_eq!(s.scene.actors[0].anchor, [200, 0]);
            assert!(s.motions.is_empty());
            assert_eq!(s.page.as_ref().unwrap().text, "second");
        }
    }
    #[test]
    fn dialogue_motion_is_concurrent_tokened_monotonic_and_keeps_cached_filter() {
        let mut s = moving_session(b".img6 >>>>>>>\nfirst\nsecond");
        let packet = s.poll_packet().unwrap();
        let location = (
            s.machine.location().map(|(n, l)| (n.to_vec(), l)),
            s.machine.steps(),
        );
        assert_eq!(s.page.as_ref().unwrap().text, "first");
        assert!(s.pending_effect.is_none());
        assert!(s.page_frame(2, 0).is_err());
        assert!(s.acknowledge_effect(1).is_err());
        for (time, x) in [(0, 400), (50, 433), (125, 575), (250, 750)] {
            let bytes = s.page_frame(1, time).unwrap();
            let split = bytes.iter().position(|b| *b == b'\n').unwrap();
            let pixels = &bytes[split + 1..];
            assert_eq!(&pixels[x * 4..x * 4 + 4], [83, 88, 137, 255]);
            assert_eq!(
                pixels
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .filter(|p| **p != [0, 0, 0, 255])
                    .count(),
                1
            );
            assert_eq!(s.page_frame(1, time).unwrap(), bytes);
            assert_eq!(s.poll_packet().unwrap(), packet);
        }
        assert!(s.page_frame(1, 249).is_err());
        assert!(s.page_frame(1, 251).is_ok());
        assert_eq!(
            location,
            (
                s.machine.location().map(|(n, l)| (n.to_vec(), l)),
                s.machine.steps()
            )
        );
        s.acknowledge_text(1).unwrap();
        assert_eq!(s.page.as_ref().unwrap().text, "second");
        assert_eq!(s.scene.actors[0].anchor, [750, 0]);
        assert!(s.motions.is_empty());
        assert!(s.page_frame(1, 250).is_err());
        assert!(s.acknowledge_text(1).is_err());
    }
    #[test]
    fn left_excursion_frames_and_early_next_keep_base_position() {
        for early in [false, true] {
            let mut s = moving_session(b".img6 [[[[\nfirst\nsecond");
            s.scene.actors[0].anchor[0] = 200;
            let packet = s.poll_packet().unwrap();
            let steps = s.machine.steps();
            let samples = if early {
                vec![(0, 200), (125, 0)]
            } else {
                vec![(0, 200), (50, 83), (125, 0), (200, 83), (250, 200)]
            };
            for (time, x) in samples {
                let frame = s.page_frame(1, time).unwrap();
                let split = frame.iter().position(|b| *b == b'\n').unwrap();
                let pixels = &frame[split + 1..];
                assert_eq!(&pixels[x * 4..x * 4 + 4], [83, 88, 137, 255]);
                assert_eq!(
                    pixels
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .filter(|p| **p != [0, 0, 0, 255])
                        .count(),
                    1
                );
                assert_eq!(s.poll_packet().unwrap(), packet);
                assert_eq!(s.machine.steps(), steps);
            }
            s.acknowledge_text(1).unwrap();
            assert_eq!(s.scene.actors[0].anchor, [200, 0]);
            assert_eq!(s.scene.actors[0].filter, crate::filter::Filter::Night);
            assert!(s.motions.is_empty());
            assert!(s.page.as_ref().unwrap().motion.is_none());
            assert!(s.page_frame(1, 250).is_err());
        }
    }
    #[test]
    fn dip_frames_restore_y_and_early_next_never_commits_peak_or_reorders() {
        for early in [false, true] {
            let mut s = moving_session(b".img6 _\nfirst\nsecond");
            s.scene.dimensions = [800, 200];
            s.scene.actors[0].anchor[1] = 50;
            let packet = s.poll_packet().unwrap();
            let steps = s.machine.steps();
            let times = if early {
                vec![(0, 50), (125, 100)]
            } else {
                vec![(0, 50), (50, 80), (125, 100), (200, 80), (250, 50)]
            };
            for (time, y) in times {
                let bytes = s.page_frame(1, time).unwrap();
                let split = bytes.iter().position(|b| *b == b'\n').unwrap();
                let pixels = &bytes[split + 1..];
                let offset = (y * 800 + 400) * 4;
                assert_eq!(&pixels[offset..offset + 4], [83, 88, 137, 255]);
                assert_eq!(
                    pixels
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .filter(|p| **p != [0, 0, 0, 255])
                        .count(),
                    1
                );
                assert_eq!(s.poll_packet().unwrap(), packet);
                assert_eq!(s.machine.steps(), steps);
            }
            s.acknowledge_text(1).unwrap();
            assert_eq!(s.scene.actors[0].anchor, [400, 50]);
            assert_eq!(s.scene.actors[0].filter, crate::filter::Filter::Night);
            assert!(s.motions.is_empty());
            assert!(s.page_frame(1, 250).is_err());
            assert!(s.page.as_ref().unwrap().motion.is_none());
        }
    }
    #[test]
    fn early_rightward_ack_then_dip_uses_committed_origin_and_new_page_identity() {
        let mut s = moving_session(b".img6 >\nright\n.img6 _\ndip\nsettled");
        s.scene.dimensions = [800, 200];
        s.scene.actors[0].anchor[1] = 50;
        s.poll_packet().unwrap();
        s.page_frame(1, 50).unwrap();
        let second = s.acknowledge_text(1).unwrap();
        assert_eq!(s.scene.actors[0].anchor, [450, 50]);
        assert!(s.page_frame(1, 250).is_err());
        assert_eq!(s.poll_packet().unwrap(), second);
        let peak = s.page_frame(2, 125).unwrap();
        let split = peak.iter().position(|b| *b == b'\n').unwrap();
        assert_eq!(
            &peak[split + 1 + (100 * 800 + 450) * 4..split + 1 + (100 * 800 + 450) * 4 + 4],
            [83, 88, 137, 255]
        );
        s.acknowledge_text(2).unwrap();
        assert_eq!(s.scene.actors[0].anchor, [450, 50]);
        assert!(s.page_frame(2, 250).is_err());
    }
    #[test]
    fn stationary_motion_is_not_discarded_or_an_extra_script_wait() {
        let mut s = moving_session(b".img6 []\nfirst\nsecond");
        let page = s.poll_packet().unwrap();
        let steps = s.machine.steps();
        assert!(s
            .page
            .as_ref()
            .unwrap()
            .motion
            .as_ref()
            .unwrap()
            .contains("stationary"));
        assert!(s.pending_effect.is_none());
        let first = s.page_frame(1, 0).unwrap();
        let split = first.iter().position(|b| *b == b'\n').unwrap();
        for time in [50, 125, 250] {
            let packet = s.page_frame(1, time).unwrap();
            let split2 = packet.iter().position(|b| *b == b'\n').unwrap();
            assert_eq!(&packet[split2 + 1..], &first[split + 1..]);
            assert_eq!(s.machine.steps(), steps);
            assert_eq!(s.poll_packet().unwrap(), page);
        }
        assert!(s.page_frame(1, 249).is_err());
        s.acknowledge_text(1).unwrap();
        assert!(s.motions.is_empty());
        assert_eq!(s.scene.actors[0].anchor, [400, 0]);
        assert!(s.page_frame(1, 250).is_err());
        let mut early = moving_session(b".img6 []\nfirst\nsecond");
        early.poll_packet().unwrap();
        early.acknowledge_text(1).unwrap();
        assert_eq!(early.page.as_ref().unwrap().text, "second");
        assert_eq!(early.scene.actors[0].anchor, [400, 0]);
    }
    #[test]
    fn motion_early_next_finishes_position_and_unverified_queue_combinations_stop() {
        let mut s = moving_session(b".img6 >\nfirst\nsecond");
        s.poll_packet().unwrap();
        s.page_frame(1, 0).unwrap();
        s.acknowledge_text(1).unwrap();
        assert_eq!(s.scene.actors[0].anchor, [450, 0]);
        assert!(s.page_frame(1, 50).is_err());
        for text in [
            b".img6 >\n.img6 >\ntext".as_slice(),
            b".img6 >\n.fade\ntext",
            b".img6 []\n.wait 1\ntext",
            b".img6 []\n.bgm\ntext",
        ] {
            let mut s = moving_session(text);
            assert!(s.poll_packet().is_err());
            assert!(s.failed);
        }
    }
    fn audio_pack(directory: &[u8], name: &[u8]) -> std::io::Cursor<Vec<u8>> {
        // Minimal archive payload tests dispatch, not Ogg decoding (covered on device).
        let mut bytes = b"NEKOPACK".to_vec();
        for word in [
            1,
            0,
            0,
            16,
            nekopack_archive::name_hash(1, directory),
            1,
            nekopack_archive::name_hash(1, name),
            4,
            0,
            4,
        ] {
            bytes.extend(word.to_le_bytes());
        }
        bytes.extend(b"OggS");
        std::io::Cursor::new(bytes)
    }

    #[test]
    fn sound_effect_loop_once_and_stop_are_ordered_and_nonblocking() {
        let mut s =
            session(b".se knock\nfirst\n.se knock loop\nsecond\n.se knock once\n.se\n.se\nthird");
        s.store = DatGame::mount([audio_pack(b"sound/se", b"knock.ogg")]).unwrap();
        s.enable_host_effects();
        for (token, looping) in [(1, false), (2, true), (3, false)] {
            let packet = s.poll_packet().unwrap();
            let e = s.pending_effect.as_ref().unwrap();
            assert_eq!(e.kind, "se");
            assert_eq!(e.payload, b"OggS");
            assert_eq!(e.audio_loop, looping);
            assert!(String::from_utf8_lossy(&packet)
                .contains(&format!("\"loop\":{looping},\"volume_percent\":100")));
            let steps = s.machine.steps();
            assert_eq!(s.poll_packet().unwrap(), packet);
            assert!(s.acknowledge_effect(token + 1).is_err());
            assert_eq!(s.machine.steps(), steps);
            s.acknowledge_effect(token).unwrap();
            if token < 3 {
                s.acknowledge_text(token).unwrap();
            }
        }
        for token in [4, 5] {
            let e = s.pending_effect.as_ref().unwrap();
            assert_eq!(e.kind, "se_stop");
            assert!(e.payload.is_empty());
            assert_eq!(e.duration_ms, 0);
            s.acknowledge_effect(token).unwrap();
        }
        assert_eq!(s.page.as_ref().unwrap().text, "third");
        assert_eq!(
            s.page.as_ref().unwrap().effects,
            ["se knock", "se_stop", "se_stop"]
        );
    }
    #[test]
    fn sound_effect_invalid_options_and_missing_resource_fail() {
        for text in [
            b".se loop".as_slice(),
            b".se knock fade",
            b".se knock loop once",
            b".se knock 50",
        ] {
            for host in [false, true] {
                let mut s = session(text);
                if host {
                    s.enable_host_effects();
                }
                assert!(s.poll_packet().is_err());
                assert!(s.failed);
                assert!(s.pending_effect.is_none());
            }
        }
        let mut s = session(b".se missing loop");
        s.enable_host_effects();
        assert!(s.poll_packet().is_err());
        assert!(s.failed);
        assert!(s.pending_effect.is_none());
    }
    #[test]
    fn environment_audio_uses_own_resource_channel_and_ordered_acknowledgements() {
        let mut s =
            session(b".bgm music\n.env rain\n:name:line:actor\nfirst\n.env rain\n.env\nsecond");
        s.store = DatGame::mount([
            audio_pack(b"sound/bgm", b"music.ogg"),
            audio_pack(b"sound/env", b"rain.ogg"),
            audio_pack(b"voice", b"line.ogg"),
        ])
        .unwrap();
        s.enable_host_effects();
        for (token, kind) in [(1, "bgm"), (2, "env"), (3, "voice")] {
            let packet = s.poll_packet().unwrap();
            let effect = s.pending_effect.as_ref().unwrap();
            assert_eq!(effect.kind, kind);
            if matches!(kind, "bgm" | "env") {
                assert!(String::from_utf8_lossy(&packet)
                    .contains("\"loop\":true,\"volume_percent\":50"));
            }
            assert_eq!(effect.payload, b"OggS");
            let steps = s.machine.steps();
            assert_eq!(s.poll_packet().unwrap(), packet);
            assert!(s.acknowledge_effect(token + 1).is_err());
            assert_eq!(s.machine.steps(), steps);
            s.acknowledge_effect(token).unwrap();
        }
        assert_eq!(s.page.as_ref().unwrap().effects, ["bgm music", "env rain"]);
        s.acknowledge_text(1).unwrap();
        assert_eq!(s.pending_effect.as_ref().unwrap().kind, "env");
        s.acknowledge_effect(4).unwrap();
        let stop = s.pending_effect.as_ref().unwrap();
        assert_eq!(stop.kind, "env_stop");
        assert!(stop.payload.is_empty());
        assert_eq!(stop.duration_ms, 0);
        assert!(s.acknowledge_effect(4).is_err());
        s.acknowledge_effect(5).unwrap();
        assert_eq!(s.page.as_ref().unwrap().text, "second");
        assert_eq!(s.page.as_ref().unwrap().effects, ["env rain", "env_stop"]);
    }
    #[test]
    fn environment_stop_is_resource_free_and_unverified_options_fail() {
        let mut s = session(b".env\n.env\ntext");
        s.enable_host_effects();
        s.poll_packet().unwrap();
        s.acknowledge_effect(1).unwrap();
        s.acknowledge_effect(2).unwrap();
        assert_eq!(s.page.as_ref().unwrap().effects, ["env_stop", "env_stop"]);
        for text in [
            b".env fade\ntext".as_slice(),
            b".env loop\ntext",
            b".env once\ntext",
            b".env rain 50\ntext",
        ] {
            for host in [true, false] {
                let mut s = session(text);
                if host {
                    s.enable_host_effects();
                }
                assert!(s.poll_packet().is_err());
                assert!(s.failed);
                assert!(s.pending_effect.is_none());
            }
        }
        let mut missing = session(b".env missing\ntext");
        missing.enable_host_effects();
        assert!(missing.poll_packet().is_err());
        assert!(missing.failed);
        assert!(missing.pending_effect.is_none());
    }
    #[test]
    fn queued_motion_survives_bgm_and_voice_acknowledgements_exactly_once() {
        for modifier in ["[]", ">", "_", "[[[["] {
            let text = format!(".img6 {modifier}\n.bgm music\n:name:line:actor\nfirst\nsecond");
            let mut s = moving_session(text.as_bytes());
            s.store = DatGame::mount([
                audio_pack(b"sound/bgm", b"music.ogg"),
                audio_pack(b"voice", b"line.ogg"),
            ])
            .unwrap();
            let bgm = s.poll_packet().unwrap();
            assert_eq!(s.pending_effect.as_ref().unwrap().kind, "bgm");
            let steps = s.machine.steps();
            assert_eq!(s.poll_packet().unwrap(), bgm);
            assert!(s.acknowledge_effect(2).is_err());
            assert_eq!(s.machine.steps(), steps);
            assert_eq!(s.motions.len(), 1);
            assert!(s.page_frame(1, 0).is_err());
            let voice = s.acknowledge_effect(1).unwrap();
            assert_eq!(s.pending_effect.as_ref().unwrap().kind, "voice");
            assert_eq!(s.poll_packet().unwrap(), voice);
            assert!(s.acknowledge_effect(1).is_err());
            assert_eq!(s.motions.len(), 1);
            s.acknowledge_effect(2).unwrap();
            assert!(s.pending_effect.is_none());
            assert_eq!(s.page.as_ref().unwrap().effects, ["bgm music"]);
            assert_eq!(s.page.as_ref().unwrap().voice, "line");
            assert!(s.page.as_ref().unwrap().motion.is_some());
            assert_eq!(s.host_sequence, 2);
            assert!(s.acknowledge_effect(2).is_err());
            let steps = s.machine.steps();
            for elapsed in [0, 125, 250] {
                s.page_frame(1, elapsed).unwrap();
            }
            assert_eq!(s.machine.steps(), steps);
            s.acknowledge_text(1).unwrap();
            assert!(s.motions.is_empty());
            assert!(s.page.as_ref().unwrap().motion.is_none());
            assert!(s.page.as_ref().unwrap().effects.is_empty());
        }
        let mut missing = moving_session(b".img6 []\n.bgm missing\ntext");
        assert!(missing
            .poll_packet()
            .unwrap_err()
            .to_string()
            .contains("missing"));
        assert!(missing.failed);
        assert!(missing.pending_effect.is_none());
        assert!(missing.poll_packet().is_err());
    }
    #[test]
    fn actor_offsets_use_canvas_center_and_solid_overlay_retains_layers() {
        let mut s = session("one\n.img 白\ntwo\n".as_bytes());
        s.scene.dimensions = [4, 4];
        s.scene.actors.push(Layer {
            filter: crate::filter::Filter::None,
            filter_pending: false,
            slot: 1,
            name: "test".into(),
            folder: "image/actor".into(),
            image: RgbaImage::filled(1, 1, [255, 0, 0, 255], PIXELS).unwrap(),
            offset: [-1, -1],
            anchor: [2, 2],
        });
        let image = decode_png(&s.page().unwrap().png, PIXELS).unwrap();
        assert_eq!(&image.pixels()[20..24], &[255, 0, 0, 255]);
        let image = decode_png(&s.next(1).unwrap().png, PIXELS).unwrap();
        assert!(image.pixels().iter().all(|b| *b == 255));
        assert_eq!(s.scene.actors.len(), 1);
    }
}
