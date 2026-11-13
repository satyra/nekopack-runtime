//! Shared observed BLESS saveload.mng composition and pointer contract.
use crate::{
    GameResources,
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
    pub text: String,
    pub thumbnail: RgbaImage,
}
#[derive(Clone)]
struct Layout {
    items: usize,
    save_pages: usize,
    load_pages: usize,
    switch_modes: bool,
    mode_titles: Option<[String; 2]>,
    labels: Vec<LabelSpec>,
    thumbnail_at: [i32; 2],
}
#[derive(Clone)]
struct LabelSpec {
    field: String,
    at: [i32; 2],
    size: f32,
    color: [u8; 3],
}
fn source_code(source: &str) -> String {
    let mut result = String::new();
    let mut block = false;
    for line in source.lines() {
        let mut rest = line;
        while !rest.is_empty() {
            if block {
                if let Some((_, tail)) = rest.split_once("*/") {
                    block = false;
                    rest = tail;
                } else {
                    break;
                }
            } else if let Some((head, tail)) = rest.split_once("/*") {
                result.push_str(head);
                block = true;
                rest = tail;
            } else {
                result.push_str(rest.split("//").next().unwrap());
                break;
            }
        }
        result.push('\n');
    }
    result
}
fn source_labels(source: &str) -> Result<Vec<LabelSpec>> {
    let tail = source
        .split_once("function ViewSavePage(")
        .ok_or("missing ViewSavePage function")?
        .1;
    let start = tail.find('{').ok_or("missing ViewSavePage body")?;
    let mut depth = 0usize;
    let mut quoted = false;
    let mut escaped = false;
    let mut end = None;
    for (offset, ch) in tail[start..].char_indices() {
        if quoted {
            if escaped {
                escaped = false
            } else if ch == '\\' {
                escaped = true
            } else if ch == '"' {
                quoted = false
            }
        } else if ch == '"' {
            quoted = true
        } else if ch == '{' {
            depth += 1
        } else if ch == '}' {
            depth -= 1;
            if depth == 0 {
                end = Some(start + offset);
                break;
            }
        }
    }
    let body = &tail[start + 1..end.ok_or("unterminated ViewSavePage body")?];
    let mut size = 0.0;
    let mut color = [0; 3];
    let mut labels = Vec::new();
    for line in body.lines() {
        let code = line.split_whitespace().collect::<String>();
        if let Some(value) = code.strip_prefix("g.Size=") {
            size = value.trim_end_matches(';').parse()?;
        }
        if let Some(value) = code.strip_prefix("g.Color=0x") {
            let rgb = u32::from_str_radix(value.trim_end_matches(';'), 16)?;
            color = [(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8];
        }
        if let Some(args) = code.strip_prefix("g.Print(normal,") {
            let args = args
                .trim_end_matches(';')
                .trim_end_matches(')')
                .split(',')
                .collect::<Vec<_>>();
            let [field, x, y, ..] = args.as_slice() else {
                return Err("save UI Glyph.Print requires field/x/y".into());
            };
            let field = match *field {
                "code" => "code",
                "info.stamp" | "datetime" | "t" => "stamp",
                "info.route" => "route",
                "info.title" => "title",
                "text" => "text",
                _ => return Err(format!("unsupported save UI Glyph.Print field {field}").into()),
            };
            if size <= 0.0 {
                return Err("save UI label has no literal Glyph.Size".into());
            }
            labels.push(LabelSpec {
                field: field.into(),
                at: [x.parse()?, y.parse()?],
                size,
                color,
            });
        }
    }
    if !labels.iter().any(|label| label.field == "code") {
        return Err("save UI has no slot code label".into());
    }
    Ok(labels)
}
fn sprite_key(name: &str) -> String {
    let name = name.trim();
    if let Some(prefix) = name.chars().next().filter(|c| "=+@*-".contains(*c)) {
        format!(
            "{prefix} {}",
            name[prefix.len_utf8()..]
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .to_ascii_uppercase()
        )
    } else {
        name.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_ascii_uppercase()
    }
}
impl Layout {
    fn from_source(source: &str, images: &BTreeMap<String, Image>, separate: bool) -> Result<Self> {
        let source = source_code(source);
        let compact = source.split_whitespace().collect::<String>();
        let constant = |name: &str| -> Result<Option<usize>> {
            let marker = format!("const{name}=");
            let Some((_, tail)) = compact.split_once(&marker) else {
                return Ok(None);
            };
            let digits = tail
                .chars()
                .take_while(char::is_ascii_digit)
                .collect::<String>();
            Ok(Some(digits.parse()?))
        };
        let items = images
            .keys()
            .filter(|name| {
                name.strip_prefix("DATA").is_some_and(|tail| {
                    !tail.is_empty() && tail.bytes().all(|b| b.is_ascii_digit())
                })
            })
            .count();
        if items == 0 || (0..items).any(|i| !images.contains_key(&format!("DATA{i}"))) {
            return Err("save UI requires contiguous DATA item anchors".into());
        }
        if let Some(expected) = constant("SAVE_ITEMS")? {
            if expected != items {
                return Err("save UI script item count differs from anchors".into());
            }
        }
        let (save_pages, load_pages) = if let Some(pages) = constant("SAVE_PAGES")? {
            (pages, pages)
        } else if compact.contains("RadioButton(images,\"PAGE\",(mode!=ID_SAVE)?11:10)") {
            (10, 11)
        } else {
            return Err("unsupported save UI page declaration".into());
        };
        if save_pages == 0 || (0..load_pages).any(|i| !images.contains_key(&format!("= PAGE#{i}")))
        {
            return Err("save UI page sprites differ from script count".into());
        }
        let switch_modes = compact.contains("spots.Bind(\"Load\",ID_LOAD)")
            || compact.contains("spots.BindR(\"Load\",ID_LOAD)");
        let mode_titles = if separate {
            None
        } else if compact.contains("?\"SBASE\":\"LBASE\"") {
            Some(["SBASE".into(), "LBASE".into()])
        } else if compact.contains("?\"SAVE\":\"LOAD\"") {
            Some(["SAVE".into(), "LOAD".into()])
        } else {
            return Err("unsupported save UI mode title declaration".into());
        };
        let (_, draw) = compact
            .split_once("normal.Draw(info.thumb,")
            .ok_or("missing save UI thumbnail Draw")?;
        let (args, _) = draw
            .split_once(')')
            .ok_or("unterminated save UI thumbnail Draw")?;
        let (x, y) = args
            .split_once(',')
            .ok_or("save UI thumbnail Draw requires x/y")?;
        let thumbnail_at = if x == "sample.x" && y == "sample.y" {
            let sample = images
                .get("SAMPLE")
                .ok_or("save UI thumbnail SAMPLE missing")?;
            let normal = images.get("DATA=").ok_or("save UI normal card missing")?;
            [
                sample.position[0] - normal.position[0],
                sample.position[1] - normal.position[1],
            ]
        } else {
            [x.parse()?, y.parse()?]
        };
        Ok(Self {
            items,
            save_pages,
            load_pages,
            switch_modes,
            mode_titles,
            labels: source_labels(&source)?,
            thumbnail_at,
        })
    }
}
pub struct SaveUi {
    images: BTreeMap<String, Image>,
    layout: Layout,
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
        let source = String::from_utf8(store.system(b"saveload.nut")?)?;
        let layout = Layout::from_source(&source, &images, load_images.is_some())?;
        for key in ["BASE", "DATA=", "DATA+", "= CLOSE"] {
            if !images.contains_key(key) {
                return Err(format!("missing save UI sprite {key}").into());
            }
        }
        Ok(Self {
            images,
            layout,
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
            let key = sprite_key(&crate::dat::decode(sprite.name)?);
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
    fn page_count(&self) -> usize {
        if self.saving {
            self.layout.save_pages
        } else {
            self.layout.load_pages
        }
    }
    fn slot_ids(&self) -> Vec<usize> {
        (0..self.layout.items)
            .map(|i| self.page * self.layout.items + i + 1)
            .collect()
    }
    fn controls(&self) -> Result<Vec<(i32, bool, [i32; 4])>> {
        let mut buttons = vec![(-1, true, self.bounds("= CLOSE")?)];
        if self.layout.switch_modes && self.saving {
            buttons.push((-2, true, self.bounds("= LOAD")?));
        } else if self.layout.switch_modes && !self.from_title {
            buttons.push((-2, true, self.bounds("= SAVE")?));
        }
        for p in 0..self.page_count() {
            buttons.push((200 + p as i32, true, self.bounds(&format!("= PAGE#{p}"))?));
        }
        for (i, id) in self.slot_ids().into_iter().enumerate() {
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
        if let Some(titles) = &self.layout.mode_titles {
            draw(&mut canvas, &titles[if self.saving { 0 } else { 1 }])?;
        }
        for p in 0..self.page_count() {
            let prefix = if self.pressed == Some(200 + p as i32) {
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
        if self.layout.switch_modes && (self.saving || !self.from_title) {
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
        for (i, id) in self.slot_ids().into_iter().enumerate() {
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
                let at = self.layout.thumbnail_at;
                card.draw(&slot.thumbnail, [at[0] - shift[0], at[1] - shift[1]]);
            }
            card.draw(&frame.pixels, [0, 0]);
            for label in &self.layout.labels {
                let text = if label.field == "code" {
                    format!(
                        "{:03}",
                        if self.page >= self.layout.save_pages {
                            id - 100
                        } else {
                            id
                        }
                    )
                } else if let Some(slot) = self.slots.get(&id) {
                    match label.field.as_str() {
                        "stamp" => slot.stamp.clone(),
                        "route" => slot.route.clone(),
                        "title" => slot.title.clone(),
                        "text" => slot.text.chars().take(38).collect(),
                        _ => unreachable!(),
                    }
                } else if self.errors.contains_key(&id) && label.field != "code" {
                    "损坏存档".into()
                } else {
                    continue;
                };
                draw_save_label(
                    &mut card,
                    font,
                    &text,
                    [label.at[0] - shift[0], label.at[1] - shift[1]],
                    label.size,
                    label.color,
                    (frame.pixels.width() as i32 - label.at[0]).max(0),
                )?;
            }
            canvas.draw(&card, [base[0] + shift[0], base[1] + shift[1]]);
            slots.push(format!(
                "{{\"id\":{id},\"occupied\":{},\"error\":{}}}",
                self.slots.contains_key(&id),
                quote(self.errors.get(&id).map_or("", String::as_str).as_bytes())?
            ));
        }
        let slot_ids = self
            .slot_ids()
            .iter()
            .map(usize::to_string)
            .collect::<Vec<_>>()
            .join(",");
        let items = self.layout.items;
        let [width, height] = self.size;
        let buttons=self.controls()?.iter().map(|(id,enabled,b)|format!("{{\"id\":{id},\"enabled\":{enabled},\"supported\":true,\"bounds\":[{},{},{},{}]}}",b[0],b[1],b[2],b[3])).collect::<Vec<_>>().join(",");
        let mut out=format!("{{\"kind\":\"save_ui\",\"size\":[{width},{height}],\"saving\":{},\"page\":{},\"from_title\":{},\"slots_per_page\":{items},\"slot_ids\":[{slot_ids}],\"input_sequence\":{},\"action\":{action},\"refresh\":{refresh},\"buttons\":[{buttons}],\"slots\":[{}]}}\n",self.saving,self.page,self.from_title,self.input_sequence,slots.join(",")).into_bytes();
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
    #[test]
    fn source_glyph_layout_uses_print_state_and_function_boundaries() {
        let source = source_code(
            r#"
function ViewSavePage(mode){
    g.Color = 0x123456
    g.Size = 25
    /* g.Print(normal, unsupported, 1, 2) */
    g.Print(normal, code, 42, 17)
}
function Other(){ g.Print(normal, unsupported, 9, 9) }
"#,
        );
        let labels = source_labels(&source).unwrap();
        assert_eq!(labels.len(), 1);
        assert_eq!(labels[0].field, "code");
        assert_eq!(labels[0].at, [42, 17]);
        assert_eq!(labels[0].size, 25.0);
        assert_eq!(labels[0].color, [0x12, 0x34, 0x56]);
        assert!(source_labels(&source.replace("code, 42, 17", "unknown, 42, 17")).is_err());
        assert!(source_labels(&source.replace("code, 42, 17", "code, 42")).is_err());
    }
    #[test]
    fn sprite_binding_names_normalize_spacing_and_case() {
        assert_eq!(sprite_key("=CLOSE"), "= CLOSE");
        assert_eq!(sprite_key("= Load"), "= LOAD");
        assert_eq!(sprite_key(" @ PAGE#3 "), "@ PAGE#3");
        assert_eq!(sprite_key("DATA="), "DATA=");
    }
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
            layout: Layout {
                items: 10,
                save_pages: 10,
                load_pages: 11,
                switch_modes: true,
                mode_titles: Some(["SBASE".into(), "LBASE".into()]),
                labels: Vec::new(),
                thumbnail_at: [38, 5],
            },
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
