//! Observed normal config sprite profile; unsupported controls never mutate Config.
use crate::{
    dat::DatGame,
    preferences::{self, Result, FIELDS},
    quote,
};
use nekopack_image::{Mng, RgbaImage};
use nekopack_script::system::SystemVm;
use std::{
    collections::BTreeMap,
    io::{Read, Seek},
};
const BUDGET: usize = 32 * 1024 * 1024;
struct Image {
    pixels: RgbaImage,
    position: [i32; 2],
}
#[derive(Clone, Debug, PartialEq)]
enum Control {
    Slider(usize),
    Edge(usize, i32),
    Radio(usize, i32),
    Page(i32),
    Track,
    Shortcut(usize),
    Choice(usize),
    Unsupported,
}
pub(crate) struct Settings {
    pub page: u32,
    size: [u32; 2],
    images: BTreeMap<String, Image>,
    pressed: Option<Control>,
    pub unsupported: bool,
    editing: Option<usize>,
    preview_text: Option<String>,
}
impl Settings {
    pub fn open<R: Read + Seek>(store: &mut DatGame<R>, page: u32) -> Result<Self> {
        if !(1..=5).contains(&page) {
            return Err("invalid settings page".into());
        }
        let mut images = BTreeMap::new();
        let mut retained = 0;
        let mut size = [0, 0];
        for (i, file) in ["config.mng".to_owned(), format!("config{page}.mng")]
            .iter()
            .enumerate()
        {
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
                let key = if raw == "BASE" {
                    format!("BASE{i}")
                } else {
                    normalize(&raw)
                };
                let pixels = mng.decode_image(sprite.ordinal, BUDGET - retained)?;
                retained += pixels.pixels().len();
                // Match DatResources::Assets: the last frame with a name wins.
                if let Some(old) = images.insert(
                    key,
                    Image {
                        pixels,
                        position: sprite.position,
                    },
                ) {
                    retained -= old.pixels.pixels().len();
                }
            }
        }
        let preview_text = if page == 2 {
            let script = crate::dat::decode(&store.read("system", "config.nut")?)?;
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
            page,
            size,
            images,
            pressed: None,
            unsupported: false,
            editing: None,
        })
    }
    fn hit_image(&self, key: &str, x: i32, y: i32) -> bool {
        self.images.get(key).is_some_and(|image| {
            crate::title::contains(
                image.position,
                [image.pixels.width(), image.pixels.height()],
                [x, y],
            )
        })
    }
    pub fn cancel_popup(&mut self) -> bool {
        self.pressed = None;
        self.editing.take().is_some()
    }
    fn hit(&self, x: i32, y: i32) -> Option<Control> {
        if self.editing.is_some() {
            return preferences::ACTIONS
                .iter()
                .enumerate()
                .find(|(_, name)| {
                    self.hit_image(
                        &format!("= {}", if name.is_empty() { "DELETE" } else { name }),
                        x,
                        y,
                    )
                })
                .map(|(i, _)| Control::Choice(i));
        }
        if self.page == 5 {
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
            if preferences::maximum(i) == 1 {
                for v in 0..=1 {
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
                for (suffix, value) in [("--", 0), ("++", preferences::maximum(i))] {
                    if self.hit_image(&format!("* {name}{suffix}"), x, y) {
                        return Some(Control::Edge(i, value));
                    }
                }
                if self.hit_image(&format!("* {name}"), x, y) {
                    return Some(Control::Slider(i));
                }
            }
        }
        self.images
            .iter()
            .any(|(k, _)| {
                (k.starts_with("* ") || visible_normal(self.page, k)) && self.hit_image(k, x, y)
            })
            .then_some(Control::Unsupported)
    }
    fn slide(&self, vm: &mut SystemVm, i: usize, x: i32) -> Result<()> {
        let area = &self.images[&format!("* {}", FIELDS[i])];
        preferences::set(
            vm,
            i,
            slider_value(
                x,
                area.position[0],
                area.pixels.width(),
                preferences::maximum(i),
            ),
        )
    }
    /// Returns a requested page; page resource replacement is owned by the session.
    pub fn pointer(&mut self, vm: &mut SystemVm, kind: i32, x: i32, y: i32) -> Result<Option<u32>> {
        self.unsupported = false;
        let hit = self.hit(x, y);
        match kind {
            0 => {
                self.pressed = hit;
                if let Some(Control::Slider(i)) = self.pressed {
                    self.slide(vm, i, x)?
                }
            }
            1 => {
                if let Some(Control::Slider(i)) = self.pressed {
                    self.slide(vm, i, x)?
                } else if self.pressed != hit {
                    self.pressed = None
                }
            }
            2 => match self.pressed.take() {
                Some(Control::Slider(i)) => self.slide(vm, i, x)?,
                Some(control) if Some(control.clone()) == hit => match control {
                    Control::Edge(i, v) | Control::Radio(i, v) => preferences::set(vm, i, v)?,
                    Control::Page(d) => return Ok(Some((self.page as i32 + d).clamp(1, 5) as u32)),
                    Control::Track => {
                        let area = &self.images["* PAGE"];
                        return Ok(Some(
                            ((i64::from(y) - i64::from(area.position[1])) * 5
                                / i64::from(area.pixels.height()))
                            .clamp(0, 4) as u32
                                + 1,
                        ));
                    }
                    Control::Shortcut(slot) => self.editing = Some(slot),
                    Control::Choice(action) => {
                        let slot = self.editing.ok_or("shortcut picker is not open")?;
                        preferences::set_shortcut(vm, slot, action)?;
                        self.editing = None;
                    }
                    Control::Unsupported => self.unsupported = true,
                    Control::Slider(_) => unreachable!(),
                },
                _ => {}
            },
            3 => self.pressed = None,
            _ => return Err("invalid settings pointer event".into()),
        }
        Ok(None)
    }
    pub fn packet(
        &self,
        vm: &mut SystemVm,
        input_sequence: u64,
        font: Option<&crate::text_render::SharedFont>,
    ) -> Result<Vec<u8>> {
        let values = preferences::values(vm)?;
        let mut canvas = RgbaImage::filled(self.size[0], self.size[1], [0, 0, 0, 255], BUDGET)?;
        let draw = |canvas: &mut RgbaImage, key: &str, offset: [i32; 2]| {
            if let Some(image) = self.images.get(key) {
                canvas.draw(
                    &image.pixels,
                    [image.position[0] + offset[0], image.position[1] + offset[1]],
                )
            }
        };
        draw(&mut canvas, "BASE0", [0, 0]);
        draw(&mut canvas, "BASE1", [0, 0]);
        for key in self.images.keys().filter(|k| visible_normal(self.page, k)) {
            let selected = key[2..].split_once('#').is_some_and(|(name, value)| {
                let name = match name {
                    "BOLD" => "Bold",
                    "BORDER" => "Border",
                    n => n,
                };
                values
                    .get(name)
                    .is_some_and(|v| value.parse::<i32>().ok() == Some(*v))
            });
            let active = format!("@ {}", &key[2..]);
            draw(
                &mut canvas,
                if selected && self.images.contains_key(&active) {
                    &active
                } else {
                    key
                },
                [0, 0],
            );
        }
        let mut shortcuts = Vec::new();
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
                if !name.is_empty() {
                    if !self.images.contains_key(*name) {
                        return Err("missing shortcut caption sprite".into());
                    }
                    draw(&mut canvas, name, offset);
                }
                shortcuts.push(format!(
                    "{{\"key\":{},\"action\":{},\"offset\":[{},{}]}}",
                    quote(key.as_bytes())?,
                    quote(name.as_bytes())?,
                    offset[0],
                    offset[1]
                ));
            }
        }
        for (name, v) in &values {
            let mask = format!("* {name}");
            let bar = format!("@ {name}");
            if let Some(area) = self.images.get(&mask) {
                if self.images.contains_key(&bar) {
                    let max = if name == "BoxOpacity" { 255 } else { 100 };
                    draw(
                        &mut canvas,
                        &bar,
                        [
                            (i64::from(area.pixels.width()) * i64::from(*v) / max) as i32,
                            0,
                        ],
                    )
                }
            }
        }
        let track = &self.images["* PAGE"];
        let bar = &self.images["@ PAGE"];
        draw(
            &mut canvas,
            "@ PAGE",
            [
                0,
                ((track.pixels.height() - bar.pixels.height()) * (self.page - 1) / 4) as i32,
            ],
        );
        if let Some(text) = &self.preview_text {
            let style = preferences::style(vm)?;
            let sample = &self.images["SAMPLE"];
            let mut pixels = RgbaImage::from_rgba(
                sample.pixels.width(),
                sample.pixels.height(),
                sample.pixels.pixels().to_vec(),
                BUDGET,
            )?;
            for p in pixels.pixels_mut().as_chunks_mut::<4>().0 {
                p[3] = (u32::from(p[3]) * u32::from(style.opacity) / 255) as u8;
            }
            canvas.draw(&pixels, sample.position);
            crate::text_render::DialogueRenderer::draw_preview(
                font.ok_or("settings preview font is not installed")?,
                text,
                &mut canvas,
                self.images["TSAMPLE"].position,
                style.bold,
                style.border,
            )?;
        }
        if self.editing.is_some() {
            draw(&mut canvas, "LIST", [0, 0]);
            for (id, name) in preferences::ACTIONS.iter().enumerate() {
                let key = if name.is_empty() { "DELETE" } else { name };
                let prefix = if self.pressed == Some(Control::Choice(id)) {
                    "+"
                } else {
                    "="
                };
                draw(&mut canvas, &format!("{prefix} {key}"), [0, 0]);
            }
        }
        let config = FIELDS
            .iter()
            .map(|k| {
                Ok(format!(
                    "{}:{}",
                    quote(k.as_bytes())?,
                    values.get(*k).ok_or("missing preference")?
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
                        **k == format!("= {}", if name.is_empty() { "DELETE" } else { name })
                    })
                } else {
                    k.starts_with("* ") || visible_normal(self.page, k)
                }
            })
            .map(|(k, i)| {
                Ok(format!(
                    "{}:[{},{},{},{}]",
                    quote(k.as_bytes())?,
                    i.position[0],
                    i.position[1],
                    i.pixels.width(),
                    i.pixels.height()
                ))
            })
            .collect::<Result<Vec<_>>>()?
            .join(",");
        let preview = if self.preview_text.is_some() {
            let image = &self.images["SAMPLE"];
            let anchor = &self.images["TSAMPLE"];
            format!("{{\"bounds\":[{},{},{},{}],\"text_origin\":[{},{}],\"font_size\":22,\"synthetic_bold\":true}}",image.position[0],image.position[1],image.pixels.width(),image.pixels.height(),anchor.position[0],anchor.position[1])
        } else {
            "null".into()
        };
        let shortcuts = shortcuts.join(",");
        let mut packet=format!("{{\"kind\":\"settings\",\"page\":{},\"editing_slot\":{},\"input_sequence\":{input_sequence},\"unsupported\":{},\"config\":{{{config}}},\"preview\":{preview},\"shortcuts\":[{shortcuts}],\"areas\":{{{areas}}}}}\n",self.page,self.editing.unwrap_or(0),self.unsupported).into_bytes();
        canvas.write_png(&mut packet)?;
        Ok(packet)
    }
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
fn slider_value(x: i32, left: i32, width: u32, maximum: i32) -> i32 {
    ((i64::from(x) - i64::from(left)) * i64::from(maximum) / i64::from(width))
        .clamp(0, i64::from(maximum)) as i32
}
#[cfg(test)]
mod tests {
    use super::*;
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
                },
            )
        })
        .collect();
        let mut page = Settings {
            page: 5,
            size: [100, 100],
            images,
            pressed: None,
            unsupported: false,
            editing: None,
            preview_text: None,
        };
        let mut vm = SystemVm::try_new().unwrap();
        vm.eval("setup","Config <- {ShortCuts=[\"CONFIG\",\"MENU\",\"RESET\",\"QUIT\",\"SAVE\",\"LOAD\",\"QSAVE\",\"QLOAD\",\"LOG\",\"HIDE\",\"AUTO\",\"SKIP\"]}").unwrap();
        page.pointer(&mut vm, 0, 5, 5).unwrap();
        page.pointer(&mut vm, 2, 5, 5).unwrap();
        assert_eq!(page.editing, Some(3));
        assert_eq!(page.hit(35, 5), None);
        assert_eq!(page.hit(5, 5), None);
        page.pointer(&mut vm, 0, 55, 5).unwrap();
        page.pointer(&mut vm, 1, 35, 5).unwrap();
        page.pointer(&mut vm, 2, 55, 5).unwrap();
        assert_eq!(page.editing, Some(3));
        assert_eq!(preferences::shortcut_ids(&mut vm).unwrap()[0], 9);
        assert!(page.cancel_popup());
        assert_eq!(page.editing, None);
        page.pointer(&mut vm, 0, 5, 5).unwrap();
        page.pointer(&mut vm, 2, 5, 5).unwrap();
        page.pointer(&mut vm, 0, 55, 5).unwrap();
        page.pointer(&mut vm, 2, 55, 5).unwrap();
        assert_eq!(page.editing, None);
        assert_eq!(preferences::shortcut_ids(&mut vm).unwrap()[0], 5);
        page.pointer(&mut vm, 0, 5, 5).unwrap();
        page.pointer(&mut vm, 2, 5, 5).unwrap();
        page.pointer(&mut vm, 0, 75, 5).unwrap();
        page.pointer(&mut vm, 2, 75, 5).unwrap();
        assert_eq!(preferences::shortcut_ids(&mut vm).unwrap()[0], 0);
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
