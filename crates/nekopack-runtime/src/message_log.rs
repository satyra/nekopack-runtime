//! Observed BLESS ViewLog: four entries, one-entry scrolling, optional voice replay.
use crate::{
    dat::DatGame,
    quote,
    text_render::{draw_log_text, SharedFont},
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
    Star,
    Page,
    Click,
}
pub(crate) fn audio<R: Read + Seek>(
    store: &mut DatGame<R>,
    system: &mut SystemVm,
    audio: Audio<'_>,
) -> Result<Vec<u8>> {
    let (kind, folder, name, volume) = match audio {
        Audio::Voice(name) => ("voice", "voice", name, "VolVO"),
        Audio::Star => ("system", "sound/se", "se_STAR", "VolSYS"),
        Audio::Page => ("system", "sound/se", "se_PAGE", "VolSYS"),
        Audio::Click => ("system", "sound/se", "se_CLICK", "VolSYS"),
    };
    let percent = audio_volume(system, volume)?;
    let bytes = store.read(folder, &format!("{name}.ogg"))?;
    if !bytes.starts_with(b"OggS") {
        return Err("invalid UI audio resource".into());
    }
    let mut out=format!("{{\"host_effect\":\"{kind}\",\"name\":{},\"category_percent\":{percent},\"duration_ms\":0,\"loop\":false}}\n",quote(name.as_bytes())?).into_bytes();
    out.extend(bytes);
    Ok(out)
}

fn audio_volume(system: &mut SystemVm, field: &str) -> Result<u32> {
    system.eval("ui-audio-volume", &format!("if(typeof Config.{field} != \"integer\")throw \"invalid UI audio configuration type\"\nreturn Config.{field}.tostring()"))?;
    let percent = system.result().trim().parse::<u32>()?;
    if percent > 100 {
        return Err("invalid UI audio volume".into());
    }
    Ok(percent)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ui_audio_uses_system_and_voice_categories() {
        let mut vm = SystemVm::new();
        vm.eval("config", "Config <- {VolSYS=17,VolSE=92,VolVO=63}")
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
