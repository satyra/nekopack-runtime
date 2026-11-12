//! Observed normal config sprite profile; unsupported controls never mutate Config.
use crate::{
    GameResources,
    dat::DatGame,
    preferences::{self, FIELDS, Result},
    quote,
};
use nekopack_image::{Mng, RgbaImage};
use nekopack_script::system::SystemVm;
use std::{
    collections::BTreeMap,
    io::{Read, Seek},
};
const BUDGET: usize = 32 * 1024 * 1024;
mod script_view;

struct Image {
    pixels: RgbaImage,
    position: [i32; 2],
    scrolls: bool,
}
struct Scroll {
    origin: [i32; 2],
    step: i32,
    last: i32,
    index: i32,
}

impl Scroll {
    fn from_script(script: &str, last: i32) -> Result<Self> {
        let declaration = |name: &str| {
            let prefix = format!("local {name} = ");
            script.lines().find_map(|line| line.trim().strip_prefix(&prefix))
                .ok_or_else(|| format!("missing settings declaration {name}"))
        };
        let vertical = declaration("offset_y")?;
        let offset_y = vertical.replace("guibase.Get(\"PAGE\")", "index");
        let mut vm = SystemVm::try_new()?;
        vm.eval("config-scroll", &format!(
            "local pagespan = {}\nlocal offset_x = {}\n\
             local index = 0\nlocal first_y = {offset_y}\n\
             index = 1\nlocal next_y = {offset_y}\n\
             return offset_x.tostring()+\" \"+first_y.tostring()+\" \"+(first_y-next_y).tostring()",
            declaration("pagespan")?, declaration("offset_x")?,
        ))?;
        let values = vm.result().split_whitespace().map(str::parse::<i32>)
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let [x, y, step] = values.as_slice() else {
            return Err("invalid settings scroll layout".into());
        };
        if *step <= 0 || step.checked_mul(last).is_none() {
            return Err("invalid settings scroll range".into());
        }
        Ok(Self { origin: [*x, *y], step: *step, last, index: 0 })
    }

    fn offset(&self) -> i32 {
        self.index * self.step
    }
}
#[cfg(test)]
#[derive(Clone, Debug, PartialEq)]
enum Control {
    Slider(usize),
    Edge(usize, i32),
    Radio(usize, i32),
    ExtendedRadio(&'static str, i32),
    VoiceRadio(String, i32),
    Page(i32),
    Track,
    Shortcut(usize),
    Choice(usize),
    Close,
    Reset,
    ResetShortcuts,
    Unsupported,
}
pub(crate) struct Settings {
    pub page: u32,
    pages: u32,
    #[cfg(test)]
    bindings: BTreeMap<String, i32>,
    size: [u32; 2],
    images: BTreeMap<String, Image>,
    pointer_down: bool,
    #[cfg(test)]
    pressed: Option<Control>,
    #[cfg(test)]
    slider_x: i32,
    pub unsupported: bool,
    editing: Option<usize>,
    preview_text: Option<String>,
    scroll: Option<Scroll>,
    close_requested: bool,
    #[cfg(test)]
    reset_requested: u8,
    script_view: Option<script_view::ScriptView>,
}
impl Settings {
    pub fn open<R: Read + Seek>(
        store: &mut DatGame<R>,
        vm: &mut SystemVm,
        page: u32,
    ) -> Result<Self> {
        let mut settings = Self::open_layout(store, vm, page)?;
        settings.script_view = Some(script_view::ScriptView::open(
            store, vm, settings.pages, settings.scroll.is_some(), page,
        )?);
        Ok(settings)
    }

    fn open_layout<R: Read + Seek>(
        store: &mut DatGame<R>,
        vm: &mut SystemVm,
        page: u32,
    ) -> Result<Self> {
        let script = String::from_utf8(store.system(b"config.nut")?)?;
        let scrolling = store.has_resource("system", "config_items.mng")?;
        vm.eval("config-bindings-host", include_str!("../host/config_bindings.nut"))?;
        vm.eval("config-bindings", &format!("return ConfigPageBindings({page})"))?;
        let bindings = vm.result().lines().map(|line| {
            let (name, range) = line.split_once('\t').ok_or("invalid settings binding")?;
            Ok((name.to_owned(), range.parse()?))
        }).collect::<Result<BTreeMap<_, _>>>()?;
        let last = *bindings.get("PAGE").ok_or("missing original settings scrollbar")?;
        if !(1..i32::MAX).contains(&last) {
            return Err("invalid settings scrollbar range".into());
        }
        let pages = if scrolling { 1 } else { last as u32 + 1 };
        if !(1..=pages).contains(&page) {
            return Err("invalid settings page".into());
        }
        let page_file = if scrolling {
            "config_items.mng".to_owned()
        } else {
            format!("config{page}.mng")
        };
        let budget = if scrolling { 48 * 1024 * 1024 } else { BUDGET };
        let mut images = BTreeMap::new();
        let mut retained = 0;
        let mut size = [0, 0];
        let mut files = vec!["config.mng", &page_file];
        if scrolling {
            files.push("config_shortcut.mng");
        }
        for (i, file) in files.into_iter().enumerate() {
            let bytes = store.read("system", file)?;
            let mng = Mng::parse(&bytes, 100000)?;
            if i == 0 {
                size = [mng.header[0], mng.header[1]];
                retained = RgbaImage::transparent(size[0], size[1], BUDGET)?
                    .pixels()
                    .len()
            }
            for sprite in mng.sprites()? {
                let raw = crate::dat::decode(sprite.name)?;
                let key = match raw.as_str() {
                    _ if i == 2 => format!("PICK {}", normalize(&raw)),
                    "BASE" => format!("BASE{i}"),
                    "BOX_SAMPLE" if scrolling => "SAMPLE".into(),
                    "BOX_TEXT" if scrolling => "TSAMPLE".into(),
                    "= PAGE+" if scrolling => "= PAGE++".into(),
                    "= PAGE-" if scrolling => "= PAGE--".into(),
                    _ => normalize(&raw),
                };
                let pixels = mng.decode_image(sprite.ordinal, budget - retained)?;
                retained += pixels.pixels().len();
                // Match DatResources::Assets: the last frame with a name wins.
                if let Some(old) = images.insert(
                    key,
                    Image {
                        pixels,
                        position: if i == 2 {
                            [sprite.position[0] + 440, sprite.position[1] + 60]
                        } else {
                            sprite.position
                        },
                        scrolls: scrolling && i == 1,
                    },
                ) {
                    retained -= old.pixels.pixels().len();
                }
            }
        }
        let scroll = if scrolling {
            Some(Scroll::from_script(&script, last)?)
        } else {
            None
        };
        let preview_text = if page == 2 || scrolling {
            let text = script
                .lines()
                .find_map(|l| l.trim().strip_prefix("local text = "))
                .ok_or("missing static config preview text")?;
            let text = text
                .trim()
                .strip_prefix('"')
                .and_then(|s| s.strip_suffix('"'))
                .ok_or("unsupported config preview literal")?;
            if text.contains('\\') {
                return Err("escaped config preview literal is not supported".into());
            }
            Some(text.to_owned())
        } else {
            None
        };
        Ok(Self {
            preview_text,
            scroll,
            close_requested: false,
            #[cfg(test)]
            reset_requested: 0,
            script_view: None,
            page,
            pages,
            #[cfg(test)]
            bindings,
            size,
            images,
            pointer_down: false,
            #[cfg(test)]
            pressed: None,
            #[cfg(test)]
            slider_x: 0,
            unsupported: false,
            editing: None,
        })
    }
    pub fn script_input<R: Read + Seek>(
        &mut self,
        store: &mut DatGame<R>,
        vm: &mut SystemVm,
        kind: i32,
        x: i32,
        y: i32,
    ) -> Result<Vec<crate::message_log::Cue>> {
        let event = match kind {
            0 => { self.pointer_down = true; "0" }
            1 if self.pointer_down => "ID_DRAG",
            1 => "0",
            2 => {
                if std::mem::take(&mut self.pointer_down) {
                    if self.editing.is_none() {
                        let view = self.script_view.as_ref().ok_or("settings script is not running")?;
                        self.editing = view.buttons.iter().find_map(|(name, &[left, top, w, h])| {
                            let slot: usize = name.strip_prefix('F')?.parse().ok()?;
                            crate::title::contains([left, top], [w as u32, h as u32], [x, y])
                                .then_some(slot)
                        });
                    }
                    "ID_OK"
                } else { "0" }
            }
            3 => { self.pointer_down = false; "0" }
            4 => "ConfigPageHost.cancel",
            _ => return Err("invalid settings input".into()),
        };
        self.run_script(store, vm, event, x, y, 0)
    }

    fn run_script<R: Read + Seek>(
        &mut self,
        store: &mut DatGame<R>,
        vm: &mut SystemVm,
        event: &str,
        x: i32,
        y: i32,
        elapsed: u64,
    ) -> Result<Vec<crate::message_log::Cue>> {
        let view = self.script_view.as_mut().ok_or("settings script is not running")?;
        let cues = view.step(store, vm, event, x, y, elapsed)?;
        if view.page == 0 {
            self.editing = None;
            self.close_requested = true;
        } else if view.page != self.page {
            let layout = Self::open_layout(store, vm, view.page)?;
            let view = self.script_view.take();
            *self = layout;
            self.script_view = view;
        }
        if let (Some(scroll), Some(view)) = (&mut self.scroll, &self.script_view) {
            scroll.index = view.index.clamp(0, scroll.last);
        }
        if self.editing.is_some() && self.script_view.as_ref().is_some_and(|view|
            !view.buttons.keys().any(|name| name.eq_ignore_ascii_case("DELETE")))
        {
            self.editing = None;
        }
        Ok(cues)
    }

    pub fn script_command<R: Read + Seek>(
        &mut self,
        store: &mut DatGame<R>,
        vm: &mut SystemVm,
        command: u64,
    ) -> Result<Vec<crate::message_log::Cue>> {
        if command == 0 {
            return self.script_input(store, vm, 4, -1, -1);
        }
        let name = match command {
            1 => "= DEFAULT",
            2 => "= ResetShortcut",
            _ => return Err("unknown settings command".into()),
        };
        let Some(image) = self.images.get(name) else { return Ok(Vec::new()) };
        let [x, y] = self.position(image);
        self.run_script(store, vm, "ID_OK", x + image.pixels.width() as i32 / 2,
            y + image.pixels.height() as i32 / 2, 0)
    }

    pub fn tick_delay(&self) -> u64 {
        self.script_view.as_ref().map_or(0, |view| view.delay.max(0) as u64)
    }

    pub fn tick<R: Read + Seek>(
        &mut self,
        store: &mut DatGame<R>,
        vm: &mut SystemVm,
        elapsed: u64,
    ) -> Result<Vec<crate::message_log::Cue>> {
        self.run_script(store, vm, "0", -1, -1, elapsed)
    }
    fn position(&self, image: &Image) -> [i32; 2] {
        let [x, y] = image.position;
        if image.scrolls {
            let scroll = self.scroll.as_ref().unwrap();
            [x + scroll.origin[0], y + scroll.origin[1] - scroll.offset()]
        } else {
            [x, y]
        }
    }

    fn visible(&self, key: &str) -> bool {
        visible_normal(self.page, key) && !(self.scroll.is_some() && key == "= FRAME")
    }

    #[cfg(test)]
    fn hit_image(&self, key: &str, x: i32, y: i32) -> bool {
        self.images.get(key).is_some_and(|image| {
            crate::title::contains(
                self.position(image),
                [image.pixels.width(), image.pixels.height()],
                [x, y],
            )
        })
    }
    pub fn take_close_request(&mut self) -> bool {
        std::mem::take(&mut self.close_requested)
    }
    #[cfg(test)]
    pub fn cancel_popup(&mut self) -> bool {
        self.pressed = None;
        self.reset_requested = 0;
        self.editing.take().is_some()
    }
    #[cfg(test)]
    fn track_index(&self, y: i32, positions: i64) -> i32 {
        let track = &self.images["* PAGE"];
        let distance = i64::from(y) - i64::from(track.position[1]);
        let height = i64::from(track.pixels.height());
        (distance * positions / height).clamp(0, positions - 1) as i32
    }
    #[cfg(test)]
    fn scroll_to(&mut self, y: i32) {
        let positions = i64::from(self.scroll.as_ref().unwrap().last) + 1;
        let index = self.track_index(y, positions);
        self.scroll.as_mut().unwrap().index = index;
    }
    #[cfg(test)]
    fn hit(&self, x: i32, y: i32, voices: &BTreeMap<String, i32>) -> Option<Control> {
        if self.editing.is_some() {
            return preferences::ACTIONS
                .iter()
                .enumerate()
                .find(|(_, name)| {
                    self.hit_image(
                        &format!(
                            "{}= {}",
                            if self.scroll.is_some() { "PICK " } else { "" },
                            if name.is_empty() { "DELETE" } else { name }
                        ),
                        x,
                        y,
                    )
                })
                .map(|(i, _)| Control::Choice(i));
        }
        if self.hit_image("= CLOSE", x, y) {
            return Some(Control::Close);
        }
        if self.hit_image("= DEFAULT", x, y) {
            return Some(Control::Reset);
        }
        if self.scroll.is_some() {
            if self.hit_image("= ResetShortcut", x, y) { return Some(Control::ResetShortcuts); }
            for slot in 2..=12 {
                if self.hit_image(&format!("= F{slot}"), x, y) {
                    return Some(Control::Shortcut(slot));
                }
            }
        }
        for tag in voices.keys().filter(|_| self.bindings.contains_key("VoiceCheck")) {
            for value in 0..=1 {
                if self.hit_image(&format!("= {tag}#{value}"), x, y) {
                    return Some(Control::VoiceRadio(tag.clone(), value));
                }
            }
        }
        if self.page == 5 && self.scroll.is_none() {
            for slot in 3..=12 {
                if self.hit_image(&format!("* F{slot}"), x, y) {
                    return Some(Control::Shortcut(slot));
                }
            }
        }
        for (key, d) in [("= PAGE--", -1), ("= PAGE++", 1)] {
            if self.hit_image(key, x, y) {
                return Some(Control::Page(d));
            }
        }
        if self.hit_image("* PAGE", x, y) {
            return Some(Control::Track);
        }
        for (i, name) in FIELDS.iter().enumerate() {
            let Some(&range) = self.bindings.get(*name) else { continue };
            if preferences::maximum(i) == 1 {
                for v in 0..range {
                    let key = match *name {
                        "Bold" => "BOLD",
                        "Border" => "BORDER",
                        _ => name,
                    };
                    if self.hit_image(&format!("= {key}#{v}"), x, y) {
                        return Some(Control::Radio(i, v));
                    }
                }
            } else {
                for (suffix, value) in [("--", 0), ("++", range)] {
                    if self.hit_image(&format!("* {name}{suffix}"), x, y) {
                        return Some(Control::Edge(i, value));
                    }
                }
                if self.hit_image(&format!("* {name}"), x, y) {
                    return Some(Control::Slider(i));
                }
            }
        }
        for name in preferences::EXTENDED.into_iter().chain(preferences::READING).chain(preferences::INPUT).chain(preferences::PRESENTATION) {
            let Some(&count) = self.bindings.get(name) else { continue };
            for value in 0..count {
                if self.hit_image(&format!("= {name}#{value}"), x, y) {
                    return Some(Control::ExtendedRadio(name, value));
                }
            }
        }
        self.images
            .iter()
            .any(|(k, _)| (k.starts_with("* ") || self.visible(k)) && self.hit_image(k, x, y))
            .then_some(Control::Unsupported)
    }
    #[cfg(test)]
    fn slide(&self, vm: &mut SystemVm, i: usize, x: i32) -> Result<()> {
        let area = &self.images[&format!("* {}", FIELDS[i])];
        preferences::set(
            vm,
            i,
            slider_value(
                x,
                self.position(area)[0],
                area.pixels.width(),
                self.bindings[FIELDS[i]],
            ),
        )
    }
    /// Returns an actual page change and the sounds with their trigger-time volume.
    /// The session owns page resource replacement.
    #[cfg(test)]
    pub fn pointer(
        &mut self,
        vm: &mut SystemVm,
        kind: i32,
        x: i32,
        y: i32,
    ) -> Result<(Option<u32>, Vec<crate::message_log::Cue>)> {
        self.unsupported = false;
        self.reset_requested = 0;
        let hit = self.hit(x, y, &preferences::voice_values(vm)?);
        let mut page = None;
        let mut sound = None;
        match kind {
            0 => {
                self.pressed = hit;
                if let Some(Control::Slider(i)) = self.pressed {
                    sound = Some(sound_cue(vm, 2)?);
                    self.slide(vm, i, x)?;
                    self.slider_x = x;
                }
            }
            1 => {
                if let Some(Control::Slider(i)) = self.pressed {
                    if hit == self.pressed {
                        sound = Some(sound_cue(vm, 2)?);
                    }
                    self.slide(vm, i, x)?;
                    self.slider_x = x;
                } else if self.pressed == Some(Control::Track) && self.scroll.is_some() {
                    if hit == Some(Control::Track) {
                        self.scroll_to(y);
                    }
                } else if self.pressed != hit {
                    self.pressed = None
                }
            }
            2 => match self.pressed.take() {
                Some(Control::Slider(i)) => {
                    if x != self.slider_x && hit == Some(Control::Slider(i)) {
                        sound = Some(sound_cue(vm, 2)?);
                    }
                    self.slide(vm, i, x)?;
                }
                Some(control) if Some(control.clone()) == hit => match control {
                    Control::Edge(i, v) => {
                        sound = Some(sound_cue(vm, 2)?);
                        preferences::set(vm, i, v)?;
                    }
                    Control::Radio(i, v) => {
                        sound = Some(sound_cue(vm, 3)?);
                        preferences::set(vm, i, v)?;
                    }
                    Control::ExtendedRadio(name, value) => {
                        sound = Some(sound_cue(vm, 3)?);
                        preferences::set_extended(vm, name, value)?;
                    }
                    Control::VoiceRadio(tag, value) => {
                        sound = Some(sound_cue(vm, 3)?);
                        preferences::set_voice(vm, &tag, value)?;
                    }
                    Control::Page(d) => {
                        if let Some(scroll) = &mut self.scroll {
                            scroll.index = (scroll.index + d).clamp(0, scroll.last);
                        } else {
                            page = Some((self.page as i32 + d).clamp(1, self.pages as i32) as u32);
                        }
                    }
                    Control::Track => {
                        if self.scroll.is_some() {
                            self.scroll_to(y);
                        } else {
                            page = Some(self.track_index(y, i64::from(self.pages)) as u32 + 1);
                        }
                    }
                    Control::Shortcut(slot) => self.editing = Some(slot),
                    Control::Choice(action) => {
                        let slot = self.editing.ok_or("shortcut picker is not open")?;
                        sound = Some(sound_cue(vm, 2)?);
                        if slot == 2 {
                            preferences::set_second_shortcut(vm, action)?;
                        } else {
                            preferences::set_shortcut(vm, slot, action)?;
                        }
                        self.editing = None;
                    }
                    Control::Close => self.close_requested = true,
                    Control::Reset => {
                        sound = Some(sound_cue(vm, 2)?);
                        self.reset_requested = 1;
                    }
                    Control::ResetShortcuts => {
                        sound = Some(sound_cue(vm, 2)?);
                        self.reset_requested = 2;
                    }
                    Control::Unsupported => self.unsupported = true,
                    Control::Slider(_) => unreachable!(),
                },
                _ => {}
            },
            3 => self.pressed = None,
            _ => return Err("invalid settings pointer event".into()),
        }
        let page = page.filter(|&page| page != self.page);
        if page.is_some() {
            sound = Some(sound_cue(vm, 1)?);
        }
        Ok((page, sound.into_iter().collect()))
    }
    pub fn packet(
        &self,
        vm: &mut SystemVm,
        input_sequence: u64,
        font: Option<&crate::text_render::SharedFont>,
        raw: bool,
    ) -> Result<Vec<u8>> {
        let values = preferences::values(vm)?;
        let voices = preferences::voice_values(vm)?;
        let mut shortcuts = Vec::new();
        if self.scroll.is_some() {
            for slot in 2..=12 {
                let action = preferences::shortcut_action(vm, slot)?;
                shortcuts.push(format!(
                    "{{\"key\":\"F{slot}\",\"action\":{}}}",
                    quote(action.as_bytes())?
                ));
            }
        }
        if self.page == 5 {
            let ids = preferences::shortcut_ids(vm)?;
            let names: Vec<_> = ids
                .iter()
                .map(|id| preferences::ACTIONS[*id as usize])
                .collect();
            let origin = self
                .images
                .get("* F3")
                .ok_or("missing F3 shortcut anchor")?
                .position;
            for (i, name) in names.iter().enumerate() {
                let key = format!("F{}", i + 3);
                let anchor = self
                    .images
                    .get(&format!("* {key}"))
                    .ok_or("missing shortcut anchor")?
                    .position;
                let offset = shortcut_offset(origin, anchor)?;
                shortcuts.push(format!(
                    "{{\"key\":{},\"action\":{},\"offset\":[{},{}]}}",
                    quote(key.as_bytes())?,
                    quote(name.as_bytes())?,
                    offset[0],
                    offset[1]
                ));
            }
        }
        let config = FIELDS.into_iter()
            .chain(preferences::INPUT.into_iter().chain(preferences::READING).chain(preferences::EXTENDED).chain(preferences::PRESENTATION).filter(|k| values.contains_key(*k)))
            .chain(voices.keys().map(String::as_str))
            .map(|k| {
                Ok(format!(
                    "{}:{}",
                    quote(k.as_bytes())?,
                    values.get(k).ok_or("missing preference")?
                ))
            })
            .collect::<Result<Vec<_>>>()?
            .join(",");
        let areas = self
            .images
            .iter()
            .filter(|(k, _)| {
                if self.editing.is_some() {
                    preferences::ACTIONS.iter().any(|name| {
                        **k == format!(
                            "{}= {}",
                            if self.scroll.is_some() { "PICK " } else { "" },
                            if name.is_empty() { "DELETE" } else { name }
                        )
                    })
                } else {
                    k.starts_with("* ") || self.visible(k)
                }
            })
            .map(|(k, i)| {
                Ok(format!(
                    "{}:[{},{},{},{}]",
                    quote(k.strip_prefix("PICK ").unwrap_or(k).as_bytes())?,
                    self.position(i)[0],
                    self.position(i)[1],
                    i.pixels.width(),
                    i.pixels.height()
                ))
            })
            .collect::<Result<Vec<_>>>()?
            .join(",");
        let preview = if self.preview_text.is_some() {
            let image = &self.images["SAMPLE"];
            let anchor = &self.images["TSAMPLE"];
            format!(
                "{{\"bounds\":[{},{},{},{}],\"text_origin\":[{},{}],\"font_size\":22,\"synthetic_bold\":true}}",
                image.position[0],
                image.position[1],
                image.pixels.width(),
                image.pixels.height(),
                anchor.position[0],
                anchor.position[1]
            )
        } else {
            "null".into()
        };
        let shortcuts = shortcuts.join(",");
        let script_areas = self.script_view.iter().flat_map(|view| view.buttons.iter())
            .map(|(name, [x, y, w, h])| {
                Ok(format!("{}:[{x},{y},{w},{h}]", quote(name.as_bytes())?))
            }).collect::<Result<Vec<_>>>()?.join(",");
        let tick_ms = self.tick_delay();
        let pixels = if raw {
            format!(",\"bounds\":[0,0,{},{}]", self.size[0], self.size[1])
        } else {
            String::new()
        };
        let mut packet=format!("{{\"kind\":\"settings\",\"scroll\":{},\"page\":{},\"editing_slot\":{},\"input_sequence\":{input_sequence},\"unsupported\":{},\"config\":{{{config}}},\"preview\":{preview},\"shortcuts\":[{shortcuts}],\"areas\":{{{areas}}},\"tick_ms\":{tick_ms},\"script_areas\":{{{script_areas}}}{pixels}}}\n",self.scroll.as_ref().map_or(0, Scroll::offset),self.page,self.editing.unwrap_or(0),self.unsupported).into_bytes();
        let frame = self.script_view.as_ref().ok_or("settings script is not running")?
            .render(self.size, font)?;
        if raw {
            packet.extend_from_slice(frame.pixels());
        } else {
            frame.write_png(&mut packet)?;
        }
        Ok(packet)
    }
}
#[cfg(test)]
fn sound_cue(vm: &mut SystemVm, id: u8) -> Result<(u8, i32)> {
    Ok((id, crate::message_log::audio_volume(vm, "VolSYS")? as i32))
}

// A shared sprite sheet also contains ShortCutsWindow's modal choices. Only
// controls bound by ViewConfigPage/its GUI belong to the normal fifth page.
fn visible_normal(page: u32, key: &str) -> bool {
    if !key.starts_with("= ") {
        return false;
    }
    page != 5
        || matches!(&key[2..], "PAGE++" | "PAGE--" | "CLOSE" | "DEFAULT")
        || key.starts_with("= CenterKey#")
        || key.starts_with("= SpaceKey#")
}
fn shortcut_offset(origin: [i32; 2], anchor: [i32; 2]) -> Result<[i32; 2]> {
    Ok([
        anchor[0]
            .checked_sub(origin[0])
            .ok_or("shortcut x offset overflow")?,
        anchor[1]
            .checked_sub(origin[1])
            .ok_or("shortcut y offset overflow")?,
    ])
}
fn normalize(raw: &str) -> String {
    if raw.starts_with(['*', '@', '=', '+']) {
        format!("{} {}", &raw[..1], raw[1..].trim_start())
    } else {
        raw.into()
    }
}
#[cfg(test)]
fn slider_value(x: i32, left: i32, width: u32, maximum: i32) -> i32 {
    ((i64::from(x) - i64::from(left)) * i64::from(maximum) / i64::from(width))
        .clamp(0, i64::from(maximum)) as i32
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires original DAT resources"]
    fn original_config_page_runs_past_wait() {
        let directory = std::env::var_os("NEKO_CONFIG_TEST_DAT").unwrap();
        let files = std::fs::read_dir(directory).unwrap().map(|entry| entry.unwrap().path())
            .filter(|path| matches!(path.file_name().and_then(|name| name.to_str()),
                Some("actor.dat" | "image.dat" | "script.dat" | "sound.dat" | "system.dat" | "voice.dat")))
            .map(|path| std::fs::File::open(path).unwrap()).collect::<Vec<_>>();
        let mut store = DatGame::mount(files).unwrap();
        let (_, mut vm) = crate::bootstrap_begin(&mut store, 1, &mut std::io::sink(), true).unwrap();
        let mut settings = Settings::open(&mut store, &mut vm, 1).unwrap();
        let original = preferences::values(&mut vm).unwrap()["SkipNoRead"];
        let image = &settings.images[&format!("= SkipNoRead#{}", 1 - original)];
        let [x, y] = settings.position(image);
        let [x, y] = [x + image.pixels.width() as i32 / 2, y + image.pixels.height() as i32 / 2];
        settings.script_input(&mut store, &mut vm, 0, x, y).unwrap();
        settings.script_input(&mut store, &mut vm, 2, x, y).unwrap();
        assert_eq!(preferences::values(&mut vm).unwrap()["SkipNoRead"], 1 - original);
        if settings.scroll.is_some() {
            settings.script_command(&mut store, &mut vm, 1).unwrap();
            let view = settings.script_view.as_ref().unwrap();
            let [x, y, w, h] = view.buttons.iter().find(|(name, _)| name.eq_ignore_ascii_case("cancel"))
                .unwrap_or_else(|| panic!("confirmation frame buttons: {:?}", view.buttons)).1;
            let (x, y) = (x + w / 2, y + h / 2);
            settings.script_input(&mut store, &mut vm, 0, x, y).unwrap();
            settings.script_input(&mut store, &mut vm, 2, x, y).unwrap();
            assert_eq!(preferences::values(&mut vm).unwrap()["SkipNoRead"], 1 - original);
        } else {
            let image = &settings.images["= PAGE++"];
            let [x, y] = settings.position(image);
            let (x, y) = (x + image.pixels.width() as i32 / 2, y + image.pixels.height() as i32 / 2);
            settings.script_input(&mut store, &mut vm, 0, x, y).unwrap();
            settings.script_input(&mut store, &mut vm, 2, x, y).unwrap();
            assert_eq!(settings.page, 2);
            let bold = preferences::values(&mut vm).unwrap()["Bold"];
            let image = &settings.images[&format!("= BOLD#{}", 1 - bold)];
            let [x, y] = settings.position(image);
            let (x, y) = (x + image.pixels.width() as i32 / 2, y + image.pixels.height() as i32 / 2);
            settings.script_input(&mut store, &mut vm, 0, x, y).unwrap();
            settings.script_input(&mut store, &mut vm, 2, x, y).unwrap();
            assert_eq!(preferences::values(&mut vm).unwrap()["Bold"], 1 - bold);
        }
        settings.script_command(&mut store, &mut vm, 0).unwrap();
        assert!(settings.take_close_request());
    }

    #[test]
    fn original_setup_selects_bindings_and_restores_the_host() {
        let mut vm = SystemVm::try_new().unwrap();
        vm.eval("config-bindings-host", include_str!("../host/config_bindings.nut"))
            .unwrap();
        vm.eval("config-bindings-fixture", r#"
function wait(time) { return 42 }
function ViewConfigPage(page) {
    local gui = GUI()
    local images = Images("config")
    if (page == 2) gui.Bind("Bold", RadioButton(images, "Bold", 2))
    else gui.Bind("VolSYS", VolumeBar(images, "VolSYS", 100))
    wait(0)
    throw "unexpected event loop entry"
}
return ConfigPageBindings(2) + wait(0).tostring()
"#).unwrap();
        assert_eq!(vm.result(), "Bold\t2\n42");
    }

    #[test]
    fn scroll_geometry_uses_the_script_expressions() {
        let mut scroll = Scroll::from_script(
            r#"
            local scrollmax = 1300-300
            local pagespan = 25
            guibase.Bind( "PAGE", ScrollBar(base_i, "PAGE", scrollmax/pagespan) )
            local offset_x = 20+5
            local offset_y = 30 - guibase.Get("PAGE") * pagespan
            "#,
            40,
        )
        .unwrap();
        assert_eq!(scroll.origin, [25, 30]);
        assert_eq!(scroll.last, 40);
        scroll.index = scroll.last;
        assert_eq!(scroll.offset(), 1000);
    }

    #[test]
    fn modal_blocks_page_and_commits_only_matching_choice() {
        let images = [
            ("* F3", 0),
            ("* PAGE", 30),
            ("= SAVE", 50),
            ("= DELETE", 70),
        ]
        .into_iter()
        .map(|(key, x)| {
            (
                key.into(),
                Image {
                    pixels: RgbaImage::transparent(10, 10, 400).unwrap(),
                    position: [x, 0],
                    scrolls: false,
                },
            )
        })
        .collect();
        let mut page = Settings {
            page: 5,
            pages: 5,
            bindings: BTreeMap::new(),
            size: [100, 100],
            images,
            pointer_down: false,
            pressed: None,
            slider_x: 0,
            unsupported: false,
            editing: None,
            preview_text: None,
            scroll: None,
            close_requested: false,
            reset_requested: 0,
            script_view: None,
        };
        let mut vm = SystemVm::try_new().unwrap();
        vm.eval("setup","Config <- {VolSYS=40,VoiceCheck={},ShortCuts=[\"CONFIG\",\"MENU\",\"RESET\",\"QUIT\",\"SAVE\",\"LOAD\",\"QSAVE\",\"QLOAD\",\"LOG\",\"HIDE\",\"AUTO\",\"SKIP\"]}").unwrap();
        page.pointer(&mut vm, 0, 5, 5).unwrap();
        assert_eq!(page.pointer(&mut vm, 2, 5, 5).unwrap(), (None, vec![]));
        assert_eq!(page.editing, Some(3));
        assert_eq!(page.hit(35, 5, &BTreeMap::new()), None);
        assert_eq!(page.hit(5, 5, &BTreeMap::new()), None);
        page.pointer(&mut vm, 0, 55, 5).unwrap();
        page.pointer(&mut vm, 1, 35, 5).unwrap();
        assert_eq!(page.pointer(&mut vm, 2, 55, 5).unwrap(), (None, vec![]));
        assert_eq!(page.editing, Some(3));
        assert_eq!(preferences::shortcut_ids(&mut vm).unwrap()[0], 9);
        assert!(page.cancel_popup());
        assert_eq!(page.editing, None);
        page.pointer(&mut vm, 0, 5, 5).unwrap();
        page.pointer(&mut vm, 2, 5, 5).unwrap();
        page.pointer(&mut vm, 0, 55, 5).unwrap();
        assert_eq!(
            page.pointer(&mut vm, 2, 55, 5).unwrap(),
            (None, vec![(2, 40)])
        );
        assert_eq!(page.editing, None);
        assert_eq!(preferences::shortcut_ids(&mut vm).unwrap()[0], 5);
        page.pointer(&mut vm, 0, 5, 5).unwrap();
        page.pointer(&mut vm, 2, 5, 5).unwrap();
        page.pointer(&mut vm, 0, 75, 5).unwrap();
        assert_eq!(
            page.pointer(&mut vm, 2, 75, 5).unwrap(),
            (None, vec![(2, 40)])
        );
        assert_eq!(preferences::shortcut_ids(&mut vm).unwrap()[0], 0);
    }
    #[test]
    fn touch_sounds_use_pre_mutation_volume_and_only_real_page_changes() {
        let images = [
            ("* VolSYS", [0, 0], 100, 10),
            ("* VolSYS++", [100, 0], 10, 10),
            ("= BOLD#0", [0, 10], 10, 10),
            ("* PAGE", [110, 0], 10, 50),
        ]
        .into_iter()
        .map(|(key, position, width, height)| {
            (
                key.into(),
                Image {
                    pixels: RgbaImage::transparent(width, height, 4000).unwrap(),
                    position,
                    scrolls: false,
                },
            )
        })
        .collect();
        let mut settings = Settings {
            page: 3,
            pages: 5,
            bindings: [("VolSYS".into(), 100), ("Bold".into(), 2)].into(),
            size: [120, 50],
            images,
            pointer_down: false,
            pressed: None,
            slider_x: 0,
            unsupported: false,
            editing: None,
            preview_text: None,
            scroll: None,
            close_requested: false,
            reset_requested: 0,
            script_view: None,
        };
        let mut vm = SystemVm::try_new().unwrap();
        vm.eval("setup", r#"
Config <- {TextSpeed=50,KeepVoice=1,VolBGM=50,VolVO=75,VolSE=50,VolENV=50,VolSYS=50,BoxOpacity=191,Bold=0,Border=1,AutoSpeed=50,SkipNoRead=0,VoiceCheck={},ShortCuts=["CONFIG","MENU","RESET","QUIT","SAVE","LOAD","QSAVE","QLOAD","LOG","HIDE","AUTO","SKIP"]}
function RestoreVolume(){}
"#).unwrap();
        let mut pointer = |kind, x, y| settings.pointer(&mut vm, kind, x, y).unwrap();
        assert_eq!(pointer(0, 50, 5), (None, vec![(2, 50)]));
        assert_eq!(pointer(1, 0, 5), (None, vec![(2, 50)]));
        assert_eq!(pointer(1, 70, 5), (None, vec![(2, 0)]));
        assert_eq!(pointer(2, 70, 5), (None, vec![]));
        assert_eq!(pointer(0, 70, 5), (None, vec![(2, 70)]));
        assert_eq!(pointer(2, 80, 5), (None, vec![(2, 70)]));
        assert_eq!(pointer(0, 105, 5), (None, vec![]));
        assert_eq!(pointer(2, 105, 5), (None, vec![(2, 80)]));
        assert_eq!(pointer(0, 5, 15), (None, vec![]));
        assert_eq!(pointer(2, 5, 15), (None, vec![(3, 100)]));
        assert_eq!(pointer(0, 115, 25), (None, vec![]));
        assert_eq!(pointer(2, 115, 25), (None, vec![]));
        assert_eq!(pointer(0, 115, 35), (None, vec![]));
        assert_eq!(pointer(2, 115, 35), (Some(4), vec![(1, 100)]));
    }
    #[test]
    fn shortcut_popup_choices_are_not_page_controls() {
        for key in [
            "= SAVE", "= LOAD", "= RESET", "= DELETE", "= AUTO", "= QUIT", "= QUIT_",
        ] {
            assert!(!visible_normal(5, key), "{key}")
        }
        for key in [
            "= CLOSE",
            "= DEFAULT",
            "= CenterKey#1",
            "= SpaceKey#0",
            "= PAGE++",
        ] {
            assert!(visible_normal(5, key), "{key}")
        }
        assert!(visible_normal(4, "= KeepVoice#0"));
        assert_eq!(shortcut_offset([304, 148], [811, 344]).unwrap(), [507, 196]);
        assert!(shortcut_offset([i32::MIN, 0], [i32::MAX, 0]).is_err());
    }
    #[test]
    fn original_slider_range_and_clamping() {
        assert_eq!(slider_value(640, 215, 850, 100), 50);
        assert_eq!(slider_value(640, 215, 850, 255), 127);
        assert_eq!(slider_value(2000, 215, 850, 255), 255);
        assert_eq!(slider_value(-1, 215, 850, 100), 0);
        assert_eq!(slider_value(2000, 215, 850, 100), 100);
        assert_eq!(slider_value(i32::MAX, i32::MIN, 850, 100), 100)
    }
    #[test]
    fn sprite_prefix_spacing_is_optional() {
        assert_eq!(normalize("*VolBGM"), "* VolBGM");
        assert_eq!(normalize("@ TextSpeed"), "@ TextSpeed");
        assert_eq!(normalize("BASE"), "BASE")
    }
}
