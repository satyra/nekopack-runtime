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
pub struct Page {
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
        Ok(format!("{{\"sequence\":{},\"source\":{},\"line\":{},\"speaker\":{},\"voice\":{},\"text\":{},\"effects\":{},\"scene\":{},\"depth\":{},\"steps\":{},\"slots\":[{}]}}",self.sequence,quote(self.source.as_bytes())?,self.line,quote(self.speaker.as_bytes())?,quote(self.voice.as_bytes())?,quote(self.text.as_bytes())?,array(&self.effects.iter().map(|s|s.as_bytes().to_vec()).collect::<Vec<_>>())?,self.scene,self.depth,self.steps,slots))
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
    pub payload: Vec<u8>,
}
impl HostEffect {
    pub fn packet(&self) -> Result<Vec<u8>, String> {
        let mut result = format!(
            "{{\"host_effect\":{},\"token\":{},\"name\":{},\"duration_ms\":{}}}\n",
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
    scene: Scene,
    presented: Option<Vec<u8>>,
    wipe: Option<crate::transition::Wipe>,
    image_macros: BTreeMap<String, Vec<String>>,
    page: Option<Page>,
    speaker: String,
    voice: String,
    sequence: u64,
    failed: bool,
    host_mode: bool,
    pending_effect: Option<HostEffect>,
    host_sequence: u64,
    effects: Vec<String>,
}
impl<R: Read + Seek> StorySession<R> {
    pub fn open(mut store: DatGame<R>, choice: i32) -> Result<Self, Box<dyn Error>> {
        let (mut machine, system) = bootstrap(&mut store, choice, &mut std::io::sink())?;
        machine.enable_dialogue();
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
            scene: Scene::new([mng.header[0], mng.header[1]]),
            presented: None,
            wipe: None,
            image_macros,
            page: None,
            speaker: String::new(),
            voice: String::new(),
            sequence: 0,
            failed: false,
            host_mode: false,
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
        self.machine.complete(None)?;
        self.page = None;
        self.speaker.clear();
        self.voice.clear();
        self.page()
    }

    /// Enable once before polling; headless fixtures retain their explicit static profile.
    pub fn enable_host_effects(&mut self) {
        self.host_mode = true;
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
            Ok(effect.packet()?)
        } else {
            Ok(self
                .page
                .as_ref()
                .ok_or("no pending story event")?
                .packet()?)
        }
    }
    pub fn acknowledge_effect(&mut self, token: u64) -> Result<Vec<u8>, Box<dyn Error>> {
        if self.failed {
            return Err("story session stopped after error".into());
        }
        if self.pending_effect.as_ref().map(|e| e.token) != Some(token) {
            return Err("stale or missing host acknowledgement".into());
        }
        self.machine.complete(None)?;
        let effect = self.pending_effect.take().unwrap();
        if effect.kind == "fade" || effect.kind == "wipe" {
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
        self.machine.complete(None)?;
        self.page = None;
        self.speaker.clear();
        self.voice.clear();
        self.poll_packet()
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
            "bgm" | "se" | "voice" => {
                let directory = match kind {
                    "bgm" => "sound/bgm",
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
        Ok(format!("{{\"source\":{},\"line\":{line},\"depth\":{},\"steps\":{},\"slots\":[{slots}],\"scene\":{},\"effects\":{},\"voice\":{}}}",quote(source)?,self.machine.depth(),self.machine.steps(),self.scene.metadata()?,array(&self.effects.iter().map(|s|s.as_bytes().to_vec()).collect::<Vec<_>>())?,quote(self.voice.as_bytes())?))
    }
    fn render(&self) -> Result<Vec<u8>, Box<dyn Error>> {
        self.scene.render()
    }
    fn actor(&mut self, slot: u8, args: &[Vec<u8>]) -> Result<(), Box<dyn Error>> {
        let center = [
            i32::try_from(self.scene.dimensions[0] / 2)?,
            i32::try_from(self.scene.dimensions[1] / 2)?,
        ];
        let p = plan(args, &self.image_macros, center[0])?;
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
        let bytes = self.store.read(&folder, &format!("{name}.png"))?;
        let image = decode_png(&bytes, PIXELS)?;
        let offset = png_position(&bytes)?;
        self.scene.put(
            Layer {
                slot,
                name,
                folder,
                image,
                offset,
                anchor,
            },
            p.order,
        )?;
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
                    self.page = Some(Page {
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
                    match command {
                        "title" => {
                            self.system.eval_with_resources(
                                "story-title",
                                &format!("com[\"title\"].call(getroottable(),{})", array(&args)?),
                                &mut self.machine.variables,
                                &mut self.store,
                            )?;
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
                        "bgm" | "se" | "wait" => {
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
            scene: Scene::new([1, 1]),
            presented: None,
            wipe: None,
            image_macros: BTreeMap::new(),
            page: None,
            speaker: String::new(),
            voice: String::new(),
            sequence: 0,
            failed: false,
            host_mode: false,
            pending_effect: None,
            host_sequence: 0,
            effects: Vec::new(),
        }
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
    #[test]
    fn actor_offsets_use_canvas_center_and_solid_overlay_retains_layers() {
        let mut s = session("one\n.img 白\ntwo\n".as_bytes());
        s.scene.dimensions = [4, 4];
        s.scene.actors.push(Layer {
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
