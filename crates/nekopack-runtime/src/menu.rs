//! Observed ViewMenu sprite profile; destinations remain owned by StorySession.
use crate::{dat::DatGame, quote, title::contains};
use nekopack_image::{Mng, RgbaImage};
use std::{
    collections::BTreeMap,
    error::Error,
    io::{Read, Seek},
};
type Result<T> = std::result::Result<T, Box<dyn Error>>;
const BUDGET: usize = 64 * 1024 * 1024;
struct Image {
    pixels: RgbaImage,
    position: [i32; 2],
}
struct Button {
    name: String,
    enabled: bool,
    normal: Option<usize>,
    hover: Option<usize>,
    area: usize,
}
pub(crate) struct Menu {
    images: BTreeMap<usize, Image>,
    buttons: Vec<Button>,
    base: usize,
    background: RgbaImage,
    pressed: Option<i32>,
    input_sequence: u64,
}
impl Menu {
    pub fn open<R: Read + Seek>(
        store: &mut DatGame<R>,
        plan: &str,
        background: RgbaImage,
    ) -> Result<Self> {
        let bytes = store.read("system", "menu.mng")?;
        let mng = Mng::parse(&bytes, 100000)?;
        if [background.width(), background.height()] != [mng.header[0], mng.header[1]] {
            return Err("menu/scene dimensions differ".into());
        }
        let sprites = mng.sprites()?;
        let mut retained = background.pixels().len() * 2;
        if retained > BUDGET {
            return Err("menu pixel budget exceeded".into());
        }
        let mut images = BTreeMap::new();
        let mut buttons = Vec::new();
        let mut base = None;
        for line in plan.lines() {
            let f: Vec<_> = line.split('\t').collect();
            let ordinal = |s: &str| -> Result<Option<usize>> {
                let n = s.parse::<i32>()?;
                if n == -1 {
                    Ok(None)
                } else {
                    Ok(Some(usize::try_from(n)?))
                }
            };
            let needed = match f.as_slice() {
                ["base", i] => {
                    let n = ordinal(i)?.ok_or("missing menu base")?;
                    base = Some(n);
                    vec![n]
                }
                ["button", name, enabled, normal, hover, area] => {
                    let area = ordinal(area)?.ok_or("missing menu hit area")?;
                    let normal = ordinal(normal)?;
                    let hover = ordinal(hover)?;
                    buttons.push(Button {
                        name: (*name).into(),
                        enabled: *enabled == "1",
                        normal,
                        hover,
                        area,
                    });
                    normal.into_iter().chain(hover).chain(Some(area)).collect()
                }
                _ => return Err("invalid menu plan".into()),
            };
            for i in needed {
                if images.contains_key(&i) {
                    continue;
                }
                let sprite = sprites.get(i).ok_or("invalid menu sprite")?;
                let pixels = mng.decode_image(i, BUDGET - retained)?;
                retained += pixels.pixels().len();
                images.insert(
                    i,
                    Image {
                        pixels,
                        position: sprite.position,
                    },
                );
            }
        }
        buttons.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(Self {
            images,
            buttons,
            base: base.ok_or("missing menu base")?,
            background,
            pressed: None,
            input_sequence: 0,
        })
    }
    pub fn pointer(&mut self, kind: i32, x: i32, y: i32) -> Result<Option<String>> {
        let hit = self
            .buttons
            .iter()
            .position(|b| {
                let i = &self.images[&b.area];
                b.enabled && contains(i.position, [i.pixels.width(), i.pixels.height()], [x, y])
            })
            .map(|i| i as i32);
        let action = crate::title::update_pointer(&mut self.pressed, kind, hit)?;
        self.input_sequence += 1;
        Ok(action.map(|i| self.buttons[i as usize].name.clone()))
    }
    pub fn packet(&self, checkpoint: &str, unsupported: Option<&str>) -> Result<Vec<u8>> {
        let mut canvas =
            RgbaImage::transparent(self.background.width(), self.background.height(), BUDGET)?;
        canvas.draw(&self.background, [0, 0]);
        let base = &self.images[&self.base];
        canvas.draw(&base.pixels, base.position);
        for (n, b) in self.buttons.iter().enumerate() {
            if !b.enabled {
                continue;
            }
            if let Some(i) = if self.pressed == Some(n as i32) {
                b.hover.or(b.normal)
            } else {
                b.normal
            } {
                let image = &self.images[&i];
                canvas.draw(&image.pixels, image.position)
            }
        }
        let buttons = self
            .buttons
            .iter()
            .map(|b| {
                let i = &self.images[&b.area];
                Ok(format!(
                    "{{\"action\":{},\"enabled\":{},\"supported\":{},\"bounds\":[{},{},{},{}]}}",
                    quote(b.name.as_bytes())?,
                    b.enabled,
                    supported(&b.name),
                    i.position[0],
                    i.position[1],
                    i.pixels.width(),
                    i.pixels.height()
                ))
            })
            .collect::<Result<Vec<_>>>()?
            .join(",");
        let mut out=format!("{{\"kind\":\"story_menu\",\"input_sequence\":{},\"checkpoint\":{checkpoint},\"unsupported\":{},\"buttons\":[{buttons}]}}\n",self.input_sequence,quote(unsupported.unwrap_or("").as_bytes())?).into_bytes();
        canvas.write_png(&mut out)?;
        Ok(out)
    }
}
pub(crate) fn supported(name: &str) -> bool {
    matches!(
        name,
        "CONFIG" | "QUIT" | "RESET" | "SAVE" | "LOAD" | "QSAVE" | "QLOAD"
    )
}

#[cfg(test)]
impl Menu {
    pub(crate) fn fixture() -> Self {
        let mut images = BTreeMap::new();
        images.insert(
            0,
            Image {
                pixels: RgbaImage::transparent(4, 1, 16).unwrap(),
                position: [0, 0],
            },
        );
        let buttons = ["LOG", "CONFIG", "QUIT", "RESET"]
            .iter()
            .enumerate()
            .map(|(i, name)| {
                images.insert(
                    i + 1,
                    Image {
                        pixels: RgbaImage::filled(1, 1, [255, 255, 255, 255], 4).unwrap(),
                        position: [i as i32, 0],
                    },
                );
                Button {
                    name: (*name).into(),
                    enabled: true,
                    normal: Some(i + 1),
                    hover: None,
                    area: i + 1,
                }
            })
            .collect();
        Self {
            images,
            buttons,
            base: 0,
            background: RgbaImage::filled(4, 1, [0, 0, 0, 255], 16).unwrap(),
            pressed: None,
            input_sequence: 0,
        }
    }
}
