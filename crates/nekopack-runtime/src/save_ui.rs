//! Shared observed BLESS saveload.mng composition and pointer contract.
use crate::{
    dat::DatGame,
    quote,
    text_render::{SharedFont, draw_save_label},
    title::{contains, update_pointer},
};
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
pub struct Slot {
    pub title: String,
    pub route: String,
    pub stamp: String,
    pub thumbnail: RgbaImage,
}
pub struct SaveUi {
    images: BTreeMap<String, Image>,
    load_images: Option<BTreeMap<String, Image>>,
    size: [u32; 2],
    slots: BTreeMap<usize, Slot>,
    errors: BTreeMap<usize, String>,
    pub saving: bool,
    pub page: usize,
    pub from_title: bool,
    pressed: Option<i32>,
    input_sequence: u64,
}
impl SaveUi {
    pub(crate) fn open<R: Read + Seek>(
        store: &mut DatGame<R>,
        saving: bool,
        from_title: bool,
    ) -> Result<Self> {
        if saving && from_title {
            return Err("cannot save from title".into());
        }
        let (size, images, load_images) =
            if let Some(bytes) = store.read_optional("system", "saveload.mng")? {
                let (size, images) = Self::decode_images(&bytes)?;
                (size, images, None)
            } else {
                let (size, images) = Self::decode_images(&store.read("system", "save.mng")?)?;
                let (load_size, load) = Self::decode_images(&store.read("system", "load.mng")?)?;
                if size != load_size {
                    return Err("save and load canvas sizes differ".into());
                }
                (size, images, Some(load))
            };
        for key in ["BASE", "DATA=", "DATA+", "= CLOSE"] {
            if !images.contains_key(key) {
                return Err(format!("missing save UI sprite {key}").into());
            }
        }
        Ok(Self {
            images,
            load_images,
            size,
            slots: BTreeMap::new(),
            errors: BTreeMap::new(),
            saving,
            page: 0,
            from_title,
            pressed: None,
            input_sequence: 0,
        })
    }
    fn decode_images(bytes: &[u8]) -> Result<([u32; 2], BTreeMap<String, Image>)> {
        let mng = Mng::parse(bytes, 100000)?;
        let size = [mng.header[0], mng.header[1]];
        let mut retained = RgbaImage::transparent(size[0], size[1], BUDGET)?
            .pixels()
            .len();
        let mut images = BTreeMap::new();
        for sprite in mng.sprites()? {
            let key = crate::dat::decode(sprite.name)?
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            let pixels = mng.decode_image(sprite.ordinal, BUDGET - retained)?;
            retained += pixels.pixels().len();
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
        Ok((size, images))
    }
    fn images(&self) -> &BTreeMap<String, Image> {
        if self.saving {
            &self.images
        } else {
            self.load_images.as_ref().unwrap_or(&self.images)
        }
    }
    pub fn set_slot(
        &mut self,
        id: usize,
        value: std::result::Result<Option<Slot>, String>,
    ) -> Result<()> {
        if !(1..=110).contains(&id) {
            return Err("invalid slot index".into());
        }
        self.slots.remove(&id);
        self.errors.remove(&id);
        match value {
            Ok(Some(slot)) => {
                self.slots.insert(id, slot);
            }
            Ok(None) => {}
            Err(e) => {
                self.errors.insert(id, e);
            }
        }
        Ok(())
    }
    fn bounds(&self, key: &str) -> Result<[i32; 4]> {
        let i = self
            .images()
            .get(key)
            .ok_or_else(|| format!("missing save sprite {key}"))?;
        Ok([
            i.position[0],
            i.position[1],
            i.pixels.width() as i32,
            i.pixels.height() as i32,
        ])
    }
    fn controls(&self) -> Result<Vec<(i32, bool, [i32; 4])>> {
        let mut buttons = vec![(-1, true, self.bounds("= CLOSE")?)];
        if self.saving {
            buttons.push((-2, true, self.bounds("= LOAD")?));
        } else if !self.from_title {
            buttons.push((-2, true, self.bounds("= SAVE")?));
        }
        for p in 0..if self.saving { 10 } else { 11 } {
            buttons.push((200 + p, true, self.bounds(&format!("= PAGE#{p}"))?));
        }
        for i in 0..10 {
            let id = self.page * 10 + i + 1;
            buttons.push((
                id as i32,
                self.saving || self.slots.contains_key(&id),
                self.bounds(&format!("DATA{i}"))?,
            ));
        }
        Ok(buttons)
    }
    pub fn pointer(&mut self, kind: i32, x: i32, y: i32) -> Result<(i32, bool)> {
        let hit = self
            .controls()?
            .into_iter()
            .find(|(_, enabled, b)| {
                *enabled && contains([b[0], b[1]], [b[2] as u32, b[3] as u32], [x, y])
            })
            .map(|(id, _, _)| id);
        self.input_sequence += 1;
        let action = update_pointer(&mut self.pressed, kind, hit)?.unwrap_or(0);
        if (200..=210).contains(&action) {
            self.page = (action - 200) as usize;
            return Ok((0, true));
        }
        if action == -2 {
            self.saving = !self.saving;
            self.page = 0;
            return Ok((0, true));
        }
        Ok((action, false))
    }
    pub(crate) fn packet(&self, font: &SharedFont, action: i32, refresh: bool) -> Result<Vec<u8>> {
        let mut canvas = RgbaImage::filled(self.size[0], self.size[1], [0, 0, 0, 255], BUDGET)?;
        let draw = |canvas: &mut RgbaImage, key: &str| -> Result<()> {
            let i = self
                .images()
                .get(key)
                .ok_or_else(|| format!("missing save sprite {key}"))?;
            canvas.draw(&i.pixels, i.position);
            Ok(())
        };
        draw(&mut canvas, "BASE")?;
        if self.load_images.is_none() {
            draw(&mut canvas, if self.saving { "SBASE" } else { "LBASE" })?;
        }
        for p in 0..if self.saving { 10 } else { 11 } {
            let prefix = if self.pressed == Some(200 + p) {
                "+"
            } else if p as usize == self.page {
                "@"
            } else {
                "="
            };
            draw(&mut canvas, &format!("{prefix} PAGE#{p}"))?;
        }
        draw(
            &mut canvas,
            if self.pressed == Some(-1) {
                "+ CLOSE"
            } else {
                "= CLOSE"
            },
        )?;
        if self.saving || !self.from_title {
            let name = if self.saving { "LOAD" } else { "SAVE" };
            draw(
                &mut canvas,
                &format!(
                    "{} {name}",
                    if self.pressed == Some(-2) { "+" } else { "=" }
                ),
            )?;
        }
        let mut slots = Vec::new();
        for i in 0..10 {
            let id = self.page * 10 + i + 1;
            let base = self.bounds(&format!("DATA{i}"))?;
            let hover = self.pressed == Some(id as i32);
            let normal = &self.images()["DATA="];
            let frame = &self.images()[if hover { "DATA+" } else { "DATA=" }];
            let shift = [
                frame.position[0] - normal.position[0],
                frame.position[1] - normal.position[1],
            ];
            let mut card =
                RgbaImage::transparent(frame.pixels.width(), frame.pixels.height(), BUDGET)?;
            if let Some(slot) = self.slots.get(&id) {
                card.draw(&slot.thumbnail, [38 - shift[0], 5 - shift[1]]);
            }
            card.draw(&frame.pixels, [0, 0]);
            let color = if hover { [255; 3] } else { [0x49, 8, 8] };
            draw_save_label(
                &mut card,
                font,
                &format!("{:03}", if self.page == 10 { id - 100 } else { id }),
                [188 - shift[0], 10 - shift[1]],
                16.0,
                color,
                75,
            )?;
            if let Some(slot) = self.slots.get(&id) {
                draw_save_label(
                    &mut card,
                    font,
                    &slot.stamp,
                    [270 - shift[0], 10 - shift[1]],
                    16.0,
                    color,
                    270,
                )?;
                draw_save_label(
                    &mut card,
                    font,
                    &slot.route,
                    [194 - shift[0], 34 - shift[1]],
                    18.0,
                    color,
                    350,
                )?;
                draw_save_label(
                    &mut card,
                    font,
                    &slot.title,
                    [194 - shift[0], 54 - shift[1]],
                    18.0,
                    color,
                    350,
                )?;
            } else if self.errors.contains_key(&id) {
                draw_save_label(
                    &mut card,
                    font,
                    "损坏存档",
                    [194 - shift[0], 34 - shift[1]],
                    18.0,
                    color,
                    350,
                )?;
            }
            canvas.draw(&card, [base[0] + shift[0], base[1] + shift[1]]);
            slots.push(format!(
                "{{\"id\":{id},\"occupied\":{},\"error\":{}}}",
                self.slots.contains_key(&id),
                quote(self.errors.get(&id).map_or("", String::as_str).as_bytes())?
            ));
        }
        let buttons=self.controls()?.iter().map(|(id,enabled,b)|format!("{{\"id\":{id},\"enabled\":{enabled},\"supported\":true,\"bounds\":[{},{},{},{}]}}",b[0],b[1],b[2],b[3])).collect::<Vec<_>>().join(",");
        let mut out=format!("{{\"kind\":\"save_ui\",\"saving\":{},\"page\":{},\"from_title\":{},\"input_sequence\":{},\"action\":{action},\"refresh\":{refresh},\"buttons\":[{buttons}],\"slots\":[{}]}}\n",self.saving,self.page,self.from_title,self.input_sequence,slots.join(",")).into_bytes();
        canvas.write_png(&mut out)?;
        Ok(out)
    }
}

pub(crate) fn thumbnail(image: &RgbaImage) -> Result<RgbaImage> {
    let mut result = RgbaImage::transparent(128, 72, 128 * 72 * 4)?;
    for y in 0..72u32 {
        for x in 0..128u32 {
            let x0 = x * image.width() / 128;
            let x1 = ((x + 1) * image.width() / 128)
                .max(x0 + 1)
                .min(image.width());
            let y0 = y * image.height() / 72;
            let y1 = ((y + 1) * image.height() / 72)
                .max(y0 + 1)
                .min(image.height());
            let mut sum = [0u64; 4];
            let mut n = 0;
            for sy in y0..y1 {
                for sx in x0..x1 {
                    let at = ((sy * image.width() + sx) * 4) as usize;
                    for (k, v) in sum.iter_mut().enumerate() {
                        *v += u64::from(image.pixels()[at + k]);
                    }
                    n += 1;
                }
            }
            let at = ((y * 128 + x) * 4) as usize;
            for (k, v) in sum.iter().enumerate() {
                result.pixels_mut()[at + k] = (v / n) as u8;
            }
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn ui(from_title: bool) -> SaveUi {
        let mut images = BTreeMap::new();
        let mut add = |key: String, x: i32| {
            images.insert(
                key,
                Image {
                    pixels: RgbaImage::transparent(8, 8, 256).unwrap(),
                    position: [x, 0],
                },
            );
        };
        add("= CLOSE".into(), 0);
        add("= LOAD".into(), 10);
        add("= SAVE".into(), 10);
        for i in 0..11 {
            add(format!("= PAGE#{i}"), 20 + i * 10);
        }
        for i in 0..10 {
            add(format!("DATA{i}"), 200 + i * 10);
        }
        SaveUi {
            images,
            load_images: None,
            size: [320, 200],
            slots: BTreeMap::new(),
            errors: BTreeMap::new(),
            saving: !from_title,
            page: 0,
            from_title,
            pressed: None,
            input_sequence: 0,
        }
    }
    #[test]
    fn original_pages_mode_switch_empty_slots_and_title_restrictions() {
        let mut s = ui(false);
        s.pointer(0, 111, 1).unwrap();
        assert_eq!(s.pointer(2, 111, 1).unwrap(), (0, true));
        assert_eq!(s.page, 9);
        s.pointer(0, 201, 1).unwrap();
        assert_eq!(s.pointer(2, 201, 1).unwrap(), (91, false));
        s.pointer(0, 11, 1).unwrap();
        s.pointer(2, 11, 1).unwrap();
        assert!(!s.saving);
        assert_eq!(s.page, 0);
        s.pointer(0, 201, 1).unwrap();
        assert_eq!(s.pointer(2, 201, 1).unwrap(), (0, false));
        s.pointer(0, 121, 1).unwrap();
        assert_eq!(s.pointer(2, 121, 1).unwrap(), (0, true));
        assert_eq!(s.page, 10);
        assert_eq!(s.controls().unwrap()[13].0, 101);
        let t = ui(true);
        assert!(!t.controls().unwrap().iter().any(|(id, _, _)| *id == -2));
        assert!(s.set_slot(0, Ok(None)).is_err());
        assert!(s.set_slot(111, Ok(None)).is_err());
    }
    #[test]
    fn thumbnail_uses_area_average_and_preserves_uniform_images() {
        let input = RgbaImage::filled(256, 144, [10, 20, 30, 255], 256 * 144 * 4).unwrap();
        let output = thumbnail(&input).unwrap();
        assert_eq!([output.width(), output.height()], [128, 72]);
        assert!(
            output
                .pixels()
                .as_chunks::<4>()
                .0
                .iter()
                .all(|p| *p == [10, 20, 30, 255])
        );
        let tiny = RgbaImage::filled(1, 1, [1, 2, 3, 255], 4).unwrap();
        assert!(
            thumbnail(&tiny)
                .unwrap()
                .pixels()
                .as_chunks::<4>()
                .0
                .iter()
                .all(|p| *p == [1, 2, 3, 255])
        );
    }
}
