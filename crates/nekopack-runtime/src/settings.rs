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
    Unsupported,
}
pub(crate) struct Settings {
    pub page: u32,
    size: [u32; 2],
    images: BTreeMap<String, Image>,
    pressed: Option<Control>,
    pub unsupported: bool,
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
        Ok(Self {
            page,
            size,
            images,
            pressed: None,
            unsupported: false,
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
    fn hit(&self, x: i32, y: i32) -> Option<Control> {
        for (key, d) in [("= PAGE--", -1), ("= PAGE++", 1)] {
            if self.hit_image(key, x, y) {
                return Some(Control::Page(d));
            }
        }
        if self.hit_image("* PAGE", x, y) {
            return Some(Control::Track);
        }
        for (i, name) in FIELDS.iter().enumerate() {
            if i == 1 {
                for v in 0..=1 {
                    if self.hit_image(&format!("= {name}#{v}"), x, y) {
                        return Some(Control::Radio(i, v));
                    }
                }
            } else {
                for (suffix, value) in [("--", 0), ("++", 100)] {
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
            .any(|(k, _)| (k.starts_with("* ") || k.starts_with("= ")) && self.hit_image(k, x, y))
            .then_some(Control::Unsupported)
    }
    fn slide(&self, vm: &mut SystemVm, i: usize, x: i32) -> Result<()> {
        let area = &self.images[&format!("* {}", FIELDS[i])];
        preferences::set(
            vm,
            i,
            slider_value(x, area.position[0], area.pixels.width()),
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
    pub fn packet(&self, vm: &mut SystemVm, input_sequence: u64) -> Result<Vec<u8>> {
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
        for key in self.images.keys().filter(|k| k.starts_with("= ")) {
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
        // Preview panel image is original; dynamic sample text is a separate future feature.
        draw(&mut canvas, "SAMPLE", [0, 0]);
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
            .filter(|(k, _)| k.starts_with("* ") || k.starts_with("= "))
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
        let mut packet=format!("{{\"kind\":\"settings\",\"page\":{},\"input_sequence\":{input_sequence},\"unsupported\":{},\"config\":{{{config}}},\"areas\":{{{areas}}}}}\n",self.page,self.unsupported).into_bytes();
        canvas.write_png(&mut packet)?;
        Ok(packet)
    }
}
fn normalize(raw: &str) -> String {
    if raw.starts_with(['*', '@', '=', '+']) {
        format!("{} {}", &raw[..1], raw[1..].trim_start())
    } else {
        raw.into()
    }
}
fn slider_value(x: i32, left: i32, width: u32) -> i32 {
    ((i64::from(x) - i64::from(left)) * 100 / i64::from(width)).clamp(0, 100) as i32
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_slider_range_and_clamping() {
        assert_eq!(slider_value(640, 215, 850), 50);
        assert_eq!(slider_value(-1, 215, 850), 0);
        assert_eq!(slider_value(2000, 215, 850), 100);
        assert_eq!(slider_value(i32::MAX, i32::MIN, 850), 100)
    }
    #[test]
    fn sprite_prefix_spacing_is_optional() {
        assert_eq!(normalize("*VolBGM"), "* VolBGM");
        assert_eq!(normalize("@ TextSpeed"), "@ TextSpeed");
        assert_eq!(normalize("BASE"), "BASE")
    }
}
