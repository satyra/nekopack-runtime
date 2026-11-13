//! Observed BLESS ViewLog: four entries, one-entry scrolling, optional voice replay.
use crate::{
    GameResources,
    dat::DatGame,
    quote,
    text_render::{SharedFont, draw_log_text},
    title::{contains, update_pointer},
};
use nekopack_image::{Mng, RgbaImage};
use nekopack_script::{
    checkpoint::{Reader, Writer},
    system::SystemVm,
};
use std::{
    collections::BTreeMap,
    error::Error,
    io::{Read, Seek},
};
type Result<T> = std::result::Result<T, Box<dyn Error>>;
const PIXELS: usize = 64 * 1024 * 1024;
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Entry {
    pub name: String,
    pub voice: String,
    pub face: String,
    pub text: String,
}
#[derive(Clone, Default)]
pub(crate) struct History {
    pub entries: Vec<Entry>,
    bytes: usize,
}
impl History {
    pub fn push(&mut self, e: Entry) -> Result<()> {
        if e.name.chars().count() > 4096
            || e.text.chars().count() > 4096
            || e.voice.contains(['/', '\\', '\0'])
            || e.voice == ".."
        {
            return Err("invalid message log entry".into());
        }
        let bytes = e.name.len() + e.voice.len() + e.face.len() + e.text.len();
        if self.entries.len() >= 32768 || self.bytes + bytes > 8 * 1024 * 1024 {
            return Err("message log budget exceeded".into());
        }
        self.bytes += bytes;
        self.entries.push(e);
        Ok(())
    }
    pub fn save(&self, w: &mut Writer) {
        w.number(self.entries.len() as u64);
        for e in &self.entries {
            w.text(&e.name);
            w.text(&e.voice);
            w.text(&e.face);
            w.text(&e.text)
        }
    }
    pub fn load(r: &mut Reader<'_>) -> Result<Self> {
        let mut h = Self::default();
        for _ in 0..r.count(32768)? {
            h.push(Entry {
                name: r.text()?,
                voice: r.text()?,
                face: r.text()?,
                text: r.text()?,
            })?
        }
        Ok(h)
    }
}

struct Sprite {
    image: RgbaImage,
    at: [i32; 2],
}
pub(crate) struct LogUi {
    sprites: BTreeMap<String, Sprite>,
    base: RgbaImage,
    pub page: usize,
    pressed: Option<i32>,
    dragging: bool,
    sequence: u64,
}
impl LogUi {
    pub fn open<R: Read + Seek>(
        store: &mut DatGame<R>,
        mut base: RgbaImage,
        count: usize,
    ) -> Result<Self> {
        let bytes = store.read("system", "log.mng")?;
        let mng = Mng::parse(&bytes, 100000)?;
        if [base.width(), base.height()] != [mng.header[0], mng.header[1]] {
            return Err("log dimensions differ".into());
        }
        let mut retained = base.pixels().len() * 2;
        if retained > PIXELS {
            return Err("log pixel budget exceeded".into());
        }
        let mut sprites = BTreeMap::new();
        for s in mng.sprites()? {
            let name = crate::dat::decode(s.name)?
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            let image = mng.decode_image(s.ordinal, PIXELS - retained)?;
            retained += image.pixels().len();
            sprites.insert(
                name,
                Sprite {
                    image,
                    at: s.position,
                },
            );
        }
        for name in ["BASE", "*SCROLL", "@SCROLL", "= SCROLL--", "= SCROLL++"] {
            if !sprites.contains_key(name) {
                return Err(format!("missing log sprite {name}").into());
            }
        }
        for i in 0..4 {
            for prefix in ["NAME", "TEXT", "= ITEM", "+ ITEM", "- ITEM"] {
                let name = format!("{prefix}{i}");
                if !sprites.contains_key(&name) {
                    return Err(format!("missing log sprite {name}").into());
                }
            }
        }
        let s = &sprites["BASE"];
        base.draw(&s.image, s.at);
        Ok(Self {
            sprites,
            base,
            page: count.saturating_sub(4),
            pressed: None,
            dragging: false,
            sequence: 0,
        })
    }
    fn bounds(&self, key: &str) -> [i32; 4] {
        let s = &self.sprites[key];
        [
            s.at[0],
            s.at[1],
            s.image.width() as i32,
            s.image.height() as i32,
        ]
    }
    fn hit(b: [i32; 4], x: i32, y: i32) -> bool {
        contains([b[0], b[1]], [b[2] as u32, b[3] as u32], [x, y])
    }
    pub fn pointer(&mut self, h: &History, kind: i32, x: i32, y: i32) -> Result<usize> {
        self.sequence += 1;
        let max = h.entries.len().saturating_sub(4);
        let bar = self.bounds("*SCROLL");
        if kind == 0 && Self::hit(bar, x, y) {
            self.dragging = true
        }
        if self.dragging && matches!(kind, 0..=2) {
            let thumb = self.sprites["@SCROLL"].image.height() as i32;
            let travel = (bar[3] - thumb).max(1);
            self.page = ((y - bar[1] - thumb / 2).clamp(0, travel) as usize * max
                + travel as usize / 2)
                / travel as usize;
        }
        if matches!(kind, 2 | 3) {
            self.dragging = false
        }
        let mut hit = None;
        for (id, key) in [(-1, "= SCROLL--"), (-2, "= SCROLL++")] {
            if Self::hit(self.bounds(key), x, y) {
                hit = Some(id)
            }
        }
        for i in 0..4 {
            if let Some(e) = h.entries.get(self.page + i) {
                if !e.name.is_empty()
                    && !e.voice.is_empty()
                    && Self::hit(self.bounds(&format!("= ITEM{i}")), x, y)
                {
                    hit = Some(i as i32 + 1)
                }
            }
        }
        match update_pointer(&mut self.pressed, kind, hit)? {
            Some(-1) => self.page = self.page.saturating_sub(1),
            Some(-2) => self.page = (self.page + 1).min(max),
            Some(n) if n > 0 => return Ok(self.page + n as usize),
            _ => {}
        }
        Ok(0)
    }
    pub fn packet(
        &self,
        h: &History,
        font: &SharedFont,
        voice: usize,
        sound: u8,
    ) -> Result<Vec<u8>> {
        let mut image = RgbaImage::transparent(self.base.width(), self.base.height(), PIXELS)?;
        image.draw(&self.base, [0, 0]);
        let mut entries = Vec::new();
        for i in 0..4 {
            if let Some(e) = h.entries.get(self.page + i) {
                // BLESS log.mng contains no FACE slots; future profiles must not invent portraits.
                let name = &self.sprites[&format!("NAME{i}")];
                let text = &self.sprites[&format!("TEXT{i}")];
                let key = format!(
                    "{} ITEM{i}",
                    if e.name.is_empty() {
                        "-"
                    } else if self.pressed == Some(i as i32 + 1) {
                        "+"
                    } else {
                        "="
                    }
                );
                let frame = &self.sprites[&key];
                image.draw(&frame.image, frame.at);
                let mut label =
                    RgbaImage::transparent(name.image.width(), name.image.height(), PIXELS)?;
                draw_log_text(&mut label, font, &e.name, 0, true)?;
                image.draw(&label, name.at);
                let mut body =
                    RgbaImage::transparent(text.image.width(), text.image.height(), PIXELS)?;
                draw_log_text(&mut body, font, &e.text, 10, false)?;
                image.draw(&body, text.at);
                let b = self.bounds(&format!("= ITEM{i}"));
                entries.push(format!("{{\"index\":{},\"name\":{},\"voice\":{},\"text\":{},\"bounds\":[{},{},{},{}]}}",self.page+i+1,quote(e.name.as_bytes())?,quote(e.voice.as_bytes())?,quote(e.text.as_bytes())?,b[0],b[1],b[2],b[3]));
            }
        }
        for key in ["= SCROLL--", "= SCROLL++"] {
            let s = &self.sprites[key];
            image.draw(&s.image, s.at)
        }
        let max = h.entries.len().saturating_sub(4);
        let b = self.bounds("*SCROLL");
        let thumb = &self.sprites["@SCROLL"];
        let offset = ((b[3] - thumb.image.height() as i32) as usize * self.page)
            .checked_div(max)
            .unwrap_or(0) as i32;
        image.draw(&thumb.image, [b[0], b[1] + offset]);
        let rect = |b: [i32; 4]| format!("[{},{},{},{}]", b[0], b[1], b[2], b[3]);
        let mut packet=format!("{{\"kind\":\"message_log\",\"page\":{},\"count\":{},\"max_page\":{max},\"input_sequence\":{},\"replay\":{voice},\"system_sound\":{sound},\"previous\":{},\"next\":{},\"scroll\":{},\"entries\":[{}]}}\n",self.page,h.entries.len(),self.sequence,rect(self.bounds("= SCROLL--")),rect(self.bounds("= SCROLL++")),rect(b),entries.join(",")).into_bytes();
        image.write_png(&mut packet)?;
        Ok(packet)
    }
}
pub(crate) enum Audio<'a> {
    Voice(&'a str),
    Save(u32),
    Page,
    Click,
}
pub(crate) fn audio<R: Read + Seek>(
    store: &mut DatGame<R>,
    system: &mut SystemVm,
    audio: Audio<'_>,
) -> Result<Vec<u8>> {
    let save_sound;
    let (kind, folder, name, volume) = match audio {
        Audio::Voice(name) => {
            if !store.has_resource("voice", &format!("{name}.ogg"))? {
                return Ok(format!("{{\"audio_unavailable\":true,\"name\":{}}}\n", quote(name.as_bytes())?).into_bytes());
            }
            ("voice", "voice", name, "VolVO")
        }
        Audio::Save(action) => {
            let source = String::from_utf8(store.system(b"saveload.nut")?)?;
            save_sound = format!("se_{}", save_sound_event(&source, action)?);
            ("system", "sound/se", save_sound.as_str(), "VolSYS")
        }
        Audio::Page => ("system", "sound/se", "se_PAGE", "VolSYS"),
        Audio::Click => ("system", "sound/se", "se_CLICK", "VolSYS"),
    };
    let percent = audio_volume(system, volume)?;
    audio_packet(store, kind, folder, name, percent)
}
// Use the mounted game's literal event for the requested save/load function.
// Sorakoi uses OK, while the five observed Dungeon profiles use STAR.
fn save_sound_event(source: &str, action: u32) -> Result<&str> {
    let function = match action {
        0 => "QuickSave",
        1 => "QuickLoad",
        2 => "NormalSave",
        3 => "NormalLoad",
        _ => return Err("unknown save/load sound action".into()),
    };
    let signature = format!("function {function}(");
    let body = source
        .split_once(&signature)
        .ok_or_else(|| format!("missing save/load function {function}"))?
        .1
        .split("\nfunction ")
        .next()
        .unwrap();
    let sound = body
        .split_once("sound(\"")
        .and_then(|(_, tail)| tail.split_once("\")").map(|(name, _)| name))
        .ok_or_else(|| format!("missing literal save/load sound in {function}"))?;
    if sound.is_empty()
        || !sound
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return Err("invalid save/load sound event".into());
    }
    Ok(sound)
}
fn audio_packet<R: Read + Seek>(
    store: &mut DatGame<R>,
    kind: &str,
    folder: &str,
    name: &str,
    percent: u32,
) -> Result<Vec<u8>> {
    let bytes = store.read(folder, &format!("{name}.ogg"))?;
    if !bytes.starts_with(b"OggS") {
        return Err("invalid UI audio resource".into());
    }
    let mut out=format!("{{\"host_effect\":\"{kind}\",\"name\":{},\"category_percent\":{percent},\"duration_ms\":0,\"loop\":false}}\n",quote(name.as_bytes())?).into_bytes();
    out.extend(bytes);
    Ok(out)
}

pub(crate) fn audio_volume(system: &mut SystemVm, field: &str) -> Result<u32> {
    crate::audio_config::category(system, field)
}

pub(crate) type Cue = (u8, i32);
pub(crate) fn page_cue(vm: &mut SystemVm) -> Result<Vec<Cue>> {
    Ok(vec![(1, audio_volume(vm, "VolSYS")? as i32)])
}
pub(crate) fn with_sounds(packet: Vec<u8>, cues: &[Cue]) -> Vec<u8> {
    let sounds = cues
        .iter()
        .map(|(id, volume)| format!("{{\"id\":{id},\"category_percent\":{volume}}}"))
        .collect::<Vec<_>>()
        .join(",");
    let mut out = format!("{{\"system_sounds\":[{sounds}],").into_bytes();
    out.extend_from_slice(&packet[1..]);
    out
}
pub(crate) fn available_ui_sound<R: Read + Seek>(
    store: &mut DatGame<R>,
    id: u8,
) -> Result<Option<u8>> {
    let file = format!("{}.ogg", ui_sound_name(u64::from(id))?);
    if store.has_resource("sound/se", &file)? {
        return Ok(Some(id));
    }
    if matches!(id, 3 | 4) && store.has_resource("sound/se", "se_CLICK.ogg")? {
        return Ok(Some(2));
    }
    Ok(None)
}

pub(crate) fn with_available_sounds<R: Read + Seek>(
    store: &mut DatGame<R>,
    packet: Vec<u8>,
    cues: &[Cue],
) -> Result<Vec<u8>> {
    let mut available = Vec::new();
    for &(id, volume) in cues {
        if let Some(id) = available_ui_sound(store, id)? {
            available.push((id, volume));
        }
    }
    Ok(with_sounds(packet, &available))
}
pub(crate) fn sound_at<R: Read + Seek>(
    store: &mut DatGame<R>,
    id: u64,
    volume: i32,
) -> Result<Vec<u8>> {
    if !(0..=100).contains(&volume) {
        return Err("invalid UI sound volume".into());
    }
    audio_packet(
        store,
        "system",
        "sound/se",
        ui_sound_name(id)?,
        volume as u32,
    )
}

pub(crate) fn ui_sound_name(id: u64) -> Result<&'static str> {
    match id {
        1 => Ok("se_PAGE"),
        2 => Ok("se_CLICK"),
        3 => Ok("se_OK"),
        4 => Ok("se_SELECT"),
        _ => Err("unknown UI sound".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn save_audio_follows_each_original_script_contract() {
        let source = ["QuickSave", "QuickLoad", "NormalSave", "NormalLoad"]
            .into_iter()
            .zip(["STAR", "OK", "SAVE", "LOAD"])
            .map(|(function, event)| format!("function {function}(){{ sound(\"{event}\"); }}"))
            .collect::<Vec<_>>()
            .join("\n");
        for (action, event) in ["STAR", "OK", "SAVE", "LOAD"].into_iter().enumerate() {
            assert_eq!(save_sound_event(&source, action as u32).unwrap(), event);
        }
        assert!(save_sound_event(&source, 4).is_err());
        assert!(save_sound_event("function QuickSave(){}", 0).is_err());
        assert!(save_sound_event("function QuickSave(){ sound(\"../bad\"); }", 0).is_err());
    }
    #[test]
    #[ignore = "requires NEKO_SAVE_SOUND_GAME_DIRS with the five Dungeon games and Sorakoi"]
    fn save_audio_reads_six_original_game_resources() {
        use std::{fs::File, io::Read};
        let directories = std::env::var_os("NEKO_SAVE_SOUND_GAME_DIRS").unwrap();
        let mut observed = Vec::new();
        for directory in std::env::split_paths(&directories) {
            let mut paths = std::fs::read_dir(directory)
                .unwrap()
                .map(|entry| entry.unwrap().path())
                .filter(|path| {
                    path.extension()
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("dat"))
                })
                .filter(|path| {
                    let mut magic = [0; 8];
                    File::open(path).unwrap().read_exact(&mut magic).unwrap();
                    &magic == b"NEKOPACK"
                })
                .collect::<Vec<_>>();
            paths.sort();
            let mut store = DatGame::mount_named(
                paths.iter().map(|path| {
                    (
                        path.file_name().unwrap().to_str().unwrap().to_owned(),
                        File::open(path).unwrap(),
                    )
                }),
                true,
            )
            .unwrap();
            let source = String::from_utf8(store.system(b"saveload.nut").unwrap()).unwrap();
            let event = save_sound_event(&source, 0).unwrap().to_owned();
            let mut vm = SystemVm::new();
            vm.eval("config", "Config <- {VolSYS=17}").unwrap();
            for action in 0..4 {
                let expected_event = save_sound_event(&source, action).unwrap();
                assert_eq!(expected_event, event);
                let packet = audio(&mut store, &mut vm, Audio::Save(action)).unwrap();
                let split = packet.iter().position(|byte| *byte == b'\n').unwrap();
                let meta = std::str::from_utf8(&packet[..split]).unwrap();
                assert!(meta.contains(&format!("\"name\":\"se_{event}\"")));
                assert!(meta.contains("\"category_percent\":17"));
                assert!(packet[split + 1..].starts_with(b"OggS"));
                assert_eq!(
                    &packet[split + 1..],
                    store.read("sound/se", &format!("se_{event}.ogg")).unwrap()
                );
            }
            observed.push(event);
        }
        assert_eq!(observed.iter().filter(|event| *event == "STAR").count(), 5);
        assert_eq!(observed.iter().filter(|event| *event == "OK").count(), 1);
    }
    #[test]
    fn ui_audio_uses_system_and_voice_categories() {
        let mut vm = SystemVm::new();
        vm.eval("config", "Config <- {VolSYS=17,VolSE=92,VolVO=63,VolBGM=50,VolENV=50}")
            .unwrap();
        assert_eq!(audio_volume(&mut vm, "VolSYS").unwrap(), 17);
        assert_eq!(audio_volume(&mut vm, "VolVO").unwrap(), 63);
        for value in ["-1", "101", "50.0", "\"50\""] {
            vm.eval("invalid", &format!("Config.VolSYS={value}"))
                .unwrap();
            assert!(audio_volume(&mut vm, "VolSYS").is_err());
        }
    }
    #[test]
    fn history_roundtrip_and_external_input_boundaries() {
        let mut h = History::default();
        let e = Entry {
            name: "人物".into(),
            voice: "KA0001".into(),
            face: "KA".into(),
            text: "一行目\n二行目".into(),
        };
        h.push(e.clone()).unwrap();
        let mut w = Writer::default();
        h.save(&mut w);
        let mut r = Reader::new(&w.0).unwrap();
        assert_eq!(History::load(&mut r).unwrap().entries, vec![e.clone()]);
        r.finish().unwrap();
        let mut bad = e.clone();
        bad.voice = "../other".into();
        assert!(h.push(bad).is_err());
        let mut bad = e;
        bad.text = "字".repeat(4097);
        assert!(h.push(bad).is_err());
        assert_eq!(h.entries.len(), 1);
        let mut short = Reader::new(&w.0[..w.0.len() - 1]).unwrap();
        assert!(History::load(&mut short).is_err());
    }
}
