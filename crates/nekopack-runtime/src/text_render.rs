//! Ordinary Unicode dialogue overlay. Resource geometry and font bytes are host-independent.
//! Includes the evidenced NekoPack punctuation wrapping rules, not a general kinsoku engine.
use ab_glyph::{point, Font, FontVec};
use nekopack_image::{Mng, RgbaImage};
use std::error::Error;
const BUDGET: usize = 16 * 1024 * 1024;
/// Bounded host font input; Android 15's CJK collection exceeds 30 MiB.
pub const MAX_FONT_BYTES: usize = 40 * 1024 * 1024;
type Result<T> = std::result::Result<T, Box<dyn Error>>;

// BLESS Sprite.Clear(); Sprite.Blt(source, 0, 0, opacity): the native
// MMX loop interpolates all four channels, not straight-alpha source-over.
// See research/verify_dialogue_rendering.py. Keep this local to dialogue;
// ordinary scene composition must retain straight-alpha semantics.
fn box_opacity(image: &mut RgbaImage, opacity: u8) {
    if opacity == 255 {
        return; // message.nut skips Clear/Blt at full opacity.
    }
    for pixel in image.pixels_mut().as_chunks_mut::<4>().0 {
        let factor = (u32::from(pixel[3]) * u32::from(opacity)) >> 8;
        for channel in pixel {
            *channel = ((u32::from(*channel) * factor) >> 8) as u8;
        }
    }
}

struct Panel {
    image: RgbaImage,
    position: [i32; 2],
    origin: [i32; 2],
    width: i32,
}
struct Glyph {
    fill: RgbaImage,
    edge: RgbaImage,
    position: [i32; 2],
}
struct Layout {
    glyphs: Vec<Option<Glyph>>,
    origins: Vec<[i32; 2]>,
}
pub struct DialogueRenderer {
    font: FontVec,
    size: [u32; 2],
    bounds: [i32; 4],
    name: Panel,
    text: Panel,
    border: bool,
    page: Option<Prepared>,
}
struct Prepared {
    sequence: u64,
    name: Option<RgbaImage>,
    glyphs: Vec<Option<Glyph>>,
    visible: Option<usize>,
    png: Vec<u8>,
    lines: usize,
    clipped: bool,
    origins: Vec<[i32; 2]>,
}

/// BLESS Glyph wrapping: hanging commas/stops, one-character carry and
/// positive-gap justification on automatic breaks. Explicit/final lines stay left-aligned.
fn positions(
    chars: &[char],
    advances: &[Option<i32>],
    width: i32,
    line_height: i32,
) -> Result<Vec<[i32; 2]>> {
    if width <= 0 || line_height <= 0 {
        return Err("invalid text layout extent".into());
    }
    let (mut x, mut y) = (0, 0);
    let mut out: Vec<[i32; 2]> = Vec::with_capacity(advances.len());
    let mut start = 0;
    for (i, advance) in advances.iter().enumerate() {
        if let Some(advance) = advance {
            if *advance < 0 || *advance > width {
                return Err("glyph exceeds text width".into());
            }
            if !"、。，．".contains(chars[i]) && x + advance > width {
                let carry = x <= width
                    && i > start
                    && ("（〔［｛〈《「『【‘“".contains(chars[i - 1])
                        || "）〕］｝〉》」』】’”ゝゞヽヾ々？！：；♪".contains(chars[i])
                        || ("…‥―".contains(chars[i]) && chars[i - 1] == chars[i]));
                let end = i - usize::from(carry);
                if end == start {
                    return Err("text width cannot fit punctuation carry".into());
                }
                let count = end - start;
                let gap = width - (out[end - 1][0] + advances[end - 1].unwrap());
                if count > 1 && gap > 0 {
                    for (j, origin) in out[start..end].iter_mut().enumerate() {
                        origin[0] += (i64::from(gap) * j as i64 / (count - 1) as i64) as i32;
                    }
                }
                x = 0;
                y += line_height;
                start = end;
                if carry {
                    out[i - 1] = [0, y];
                    x = advances[i - 1].unwrap();
                }
            }
            out.push([x, y]);
            x += advance;
        } else {
            out.push([x, y]);
            x = 0;
            y += line_height;
            start = i + 1;
        }
    }
    Ok(out)
}

impl DialogueRenderer {
    pub fn new(
        font: Vec<u8>,
        collection_index: u32,
        mng: &[u8],
        opacity: u8,
        border: bool,
    ) -> Result<Self> {
        if font.len() > MAX_FONT_BYTES {
            return Err("font exceeds 40 MiB input budget".into());
        }
        let font = FontVec::try_from_vec_and_index(font, collection_index)?;
        let parsed = Mng::parse(mng, 4096)?;
        let size = [parsed.header[0], parsed.header[1]];
        RgbaImage::transparent(size[0], size[1], BUDGET)?;
        let sprites = parsed.sprites()?;
        let panel = |name: &[u8], anchor: &[u8]| -> Result<Panel> {
            let find = |key: &[u8]| -> Result<usize> {
                let found: Vec<_> = sprites
                    .iter()
                    .enumerate()
                    .filter(|(_, s)| s.name == key)
                    .map(|(i, _)| i)
                    .collect();
                if found.len() != 1 {
                    return Err("missing or ambiguous dialogue sprite".into());
                }
                Ok(found[0])
            };
            let i = find(name)?;
            let j = find(anchor)?;
            let mut image = parsed.decode_image(sprites[i].ordinal, BUDGET)?;
            box_opacity(&mut image, opacity);
            let area = parsed.decode_image(sprites[j].ordinal, BUDGET)?;
            let origin = [
                sprites[j].position[0]
                    .checked_sub(sprites[i].position[0])
                    .ok_or("dialogue anchor x overflow")?,
                sprites[j].position[1]
                    .checked_sub(sprites[i].position[1])
                    .ok_or("dialogue anchor y overflow")?,
            ];
            if origin[0] < 0
                || origin[1] < 0
                || origin[0] as u32 >= image.width()
                || origin[1] as u32 >= image.height()
                || area.width() > i32::MAX as u32
            {
                return Err("invalid dialogue anchor".into());
            }
            Ok(Panel {
                image,
                position: sprites[i].position,
                origin,
                width: area.width() as i32,
            })
        };
        let name = panel(b"NAME", b"*NAME")?;
        let text = panel(b"TEXT", b"*TEXT")?;
        let x = name.position[0].min(text.position[0]);
        let y = name.position[1].min(text.position[1]);
        let edge = |position: i32, extent: u32| {
            position
                .checked_add(extent as i32)
                .ok_or("dialogue panel edge overflow")
        };
        let right = edge(name.position[0], name.image.width())?
            .max(edge(text.position[0], text.image.width())?);
        let bottom = edge(name.position[1], name.image.height())?
            .max(edge(text.position[1], text.image.height())?);
        if x < 0 || y < 0 || right > size[0] as i32 || bottom > size[1] as i32 {
            return Err("dialogue panels outside canvas".into());
        }
        Ok(Self {
            font,
            size,
            bounds: [x, y, right - x, bottom - y],
            name,
            text,
            border,
            page: None,
        })
    }

    fn layout(&self, text: &str, width: i32) -> Result<Layout> {
        if text.chars().count() > 4096 {
            return Err("dialogue exceeds 4096 scalar budget".into());
        }
        // 22 pixels per em; normalize ab_glyph's ascent/descent-based scale explicitly.
        let units = self
            .font
            .units_per_em()
            .ok_or("font has invalid em units")?;
        let factor = 22.0 / units;
        let scale = self.font.height_unscaled() * factor;
        let baseline = self.font.ascent_unscaled() * factor;
        let mut advances = Vec::new();
        for ch in text.chars() {
            if ch == '\n' {
                advances.push(None);
                continue;
            }
            if ch.is_control() {
                return Err("unsupported dialogue control character".into());
            }
            let id = self.font.glyph_id(ch);
            if id.0 == 0 {
                return Err(format!("font missing U+{:04X}", ch as u32).into());
            }
            advances.push(Some(
                (self.font.h_advance_unscaled(id) * factor).round() as i32
            ));
        }
        let locations = positions(&text.chars().collect::<Vec<_>>(), &advances, width, 30)?;
        let mut used = 0usize;
        let glyphs = text
            .chars()
            .zip(locations.iter().copied())
            .map(|(ch, [x, y])| {
                if ch == '\n' {
                    return Ok(None);
                }
                let glyph = self
                    .font
                    .glyph_id(ch)
                    .with_scale_and_position(scale, point(x as f32, y as f32 + baseline));
                let Some(outline) = self.font.outline_glyph(glyph) else {
                    return Ok(None);
                };
                let bounds = outline.px_bounds();
                let w = (bounds.max.x - bounds.min.x) as u32;
                let h = (bounds.max.y - bounds.min.y) as u32;
                if w > 128 || h > 128 {
                    return Err("glyph raster exceeds 128px extent".into());
                }
                used += (w as usize + 2) * (h as usize + 2) * 8;
                if used > BUDGET {
                    return Err("dialogue glyph cache budget exceeded".into());
                }
                let mut fill = RgbaImage::transparent(w + 2, h + 2, BUDGET)?;
                outline.draw(|x, y, coverage| {
                    let i = (((y + 1) * (w + 2) + x + 1) * 4) as usize;
                    fill.pixels_mut()[i..i + 4].copy_from_slice(&[
                        255,
                        255,
                        255,
                        (coverage * 255.0).round() as u8,
                    ]);
                });
                let mut edge = RgbaImage::transparent(w + 2, h + 2, BUDGET)?;
                for y in 0..h + 2 {
                    for x in 0..w + 2 {
                        let mut alpha = 0;
                        for dy in -1..=1 {
                            for dx in -1..=1 {
                                let xx = x as i32 + dx;
                                let yy = y as i32 + dy;
                                if xx >= 0 && yy >= 0 && xx < (w + 2) as i32 && yy < (h + 2) as i32
                                {
                                    alpha = alpha.max(
                                        fill.pixels()
                                            [((yy as u32 * (w + 2) + xx as u32) * 4 + 3) as usize],
                                    );
                                }
                            }
                        }
                        edge.pixels_mut()[((y * (w + 2) + x) * 4 + 3) as usize] = alpha;
                    }
                }
                Ok(Some(Glyph {
                    fill,
                    edge,
                    position: [bounds.min.x as i32 - 1, bounds.min.y as i32 - 1],
                }))
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Layout {
            glyphs,
            origins: locations,
        })
    }

    fn draw_glyphs(
        target: &mut RgbaImage,
        glyphs: &[Option<Glyph>],
        offset: [i32; 2],
        border: bool,
    ) {
        if border {
            for g in glyphs.iter().flatten() {
                target.draw(
                    &g.edge,
                    [g.position[0] + offset[0], g.position[1] + offset[1]],
                );
            }
        }
        for g in glyphs.iter().flatten() {
            target.draw(
                &g.fill,
                [g.position[0] + offset[0], g.position[1] + offset[1]],
            );
        }
    }
    /// Rebuild from original DAT pixels; never multiply an already faded panel.
    pub(crate) fn set_box_opacity(&mut self, mng: &[u8], opacity: u8) -> Result<()> {
        let parsed = Mng::parse(mng, 4096)?;
        let sprites = parsed.sprites()?;
        let decode = |name: &[u8]| -> Result<RgbaImage> {
            let sprite = sprites
                .iter()
                .find(|s| s.name == name)
                .ok_or("missing dialogue panel")?;
            let mut image = parsed.decode_image(sprite.ordinal, BUDGET)?;
            box_opacity(&mut image, opacity);
            Ok(image)
        };
        let name = decode(b"NAME")?;
        let text = decode(b"TEXT")?;
        self.name.image = name;
        self.text.image = text;
        Ok(())
    }
    pub fn prepare(&mut self, sequence: u64, speaker: &str, text: &str) -> Result<()> {
        let name = if speaker.is_empty() {
            None
        } else {
            let (encoded, _, errors) = encoding_rs::SHIFT_JIS.encode(speaker);
            if errors {
                return Err("speaker not encodable in active CP932 profile".into());
            }
            let x = (self.name.width - encoded.len() as i32 * 11) / 2;
            // Name Print occurs before message.nut assigns LineWidth for the body.
            // Long names remain on one line and are clipped by the NAME sprite.
            let glyphs = self.layout(speaker, i32::MAX)?;
            let mut image = RgbaImage::from_rgba(
                self.name.image.width(),
                self.name.image.height(),
                self.name.image.pixels().to_vec(),
                BUDGET,
            )?;
            Self::draw_glyphs(
                &mut image,
                &glyphs.glyphs,
                [self.name.origin[0] + x, self.name.origin[1]],
                self.border,
            );
            Some(image)
        };
        let layout = self.layout(text, self.text.width)?;
        let glyphs = layout.glyphs;
        let clipped = glyphs.iter().flatten().any(|g| {
            g.position[1] + self.text.origin[1] + g.fill.height() as i32
                > self.text.image.height() as i32
                || g.position[0] + self.text.origin[0] + g.fill.width() as i32
                    > self.text.image.width() as i32
        });
        let lines = layout
            .origins
            .iter()
            .map(|origin| (origin[1] / 30 + 1) as usize)
            .max()
            .unwrap_or(0);
        self.page = Some(Prepared {
            sequence,
            name,
            glyphs,
            visible: None,
            png: Vec::new(),
            lines,
            clipped,
            origins: layout.origins,
        });
        Ok(())
    }
    pub fn frame(&mut self, sequence: u64, visible: usize) -> Result<Vec<u8>> {
        let page = self.page.as_mut().ok_or("no prepared dialogue image")?;
        if page.sequence != sequence || visible > page.glyphs.len() {
            return Err("stale or invalid dialogue image frame".into());
        }
        if page.visible != Some(visible) {
            let mut image =
                RgbaImage::transparent(self.bounds[2] as u32, self.bounds[3] as u32, BUDGET)?;
            let mut text = RgbaImage::from_rgba(
                self.text.image.width(),
                self.text.image.height(),
                self.text.image.pixels().to_vec(),
                BUDGET,
            )?;
            Self::draw_glyphs(
                &mut text,
                &page.glyphs[..visible],
                self.text.origin,
                self.border,
            );
            if let Some(name) = &page.name {
                image.draw(
                    name,
                    [
                        self.name.position[0] - self.bounds[0],
                        self.name.position[1] - self.bounds[1],
                    ],
                );
            }
            image.draw(
                &text,
                [
                    self.text.position[0] - self.bounds[0],
                    self.text.position[1] - self.bounds[1],
                ],
            );
            page.png.clear();
            image.write_png(&mut page.png)?;
            page.visible = Some(visible);
        }
        Ok(page.png.clone())
    }
    pub fn metadata(&self) -> Result<String> {
        let p = self.page.as_ref().ok_or("no prepared dialogue image")?;
        Ok(format!("{{\"sequence\":{},\"size\":[{},{}],\"bounds\":[{},{},{},{}],\"font_size\":22,\"line_span\":8,\"lines\":{},\"clipped\":{},\"origins\":{:?},\"text_rect\":[{},{},{},{}],\"name_rect\":[{},{},{},{}]}}",p.sequence,self.size[0],self.size[1],self.bounds[0],self.bounds[1],self.bounds[2],self.bounds[3],p.lines,p.clipped,p.origins,self.text.position[0],self.text.position[1],self.text.image.width(),self.text.image.height(),self.name.position[0],self.name.position[1],self.name.image.width(),self.name.image.height()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn layout_for(text: &str, width: i32) -> Vec<[i32; 2]> {
        let chars: Vec<_> = text.chars().collect();
        let advances: Vec<_> = chars
            .iter()
            .map(|c| if *c == '\n' { None } else { Some(22) })
            .collect();
        positions(&chars, &advances, width, 30).unwrap()
    }
    #[test]
    fn native_hanging_stops_and_post_hanging_wrap() {
        assert_eq!(
            layout_for("甲乙丙。丁", 66),
            vec![[0, 0], [22, 0], [44, 0], [66, 0], [0, 30]]
        );
        // Once already past the limit, the original takes an ordinary break,
        // even for a closing mark. Do not substitute a general kinsoku policy.
        assert_eq!(layout_for("甲乙丙。）」", 66)[4], [0, 30]);
    }
    #[test]
    fn native_single_character_carry_and_positive_gap_justification() {
        let expected = vec![[0, 0], [44, 0], [0, 30], [22, 30]];
        for text in ["甲乙「丙", "甲乙丙」", "甲乙……", "甲乙――"] {
            assert_eq!(layout_for(text, 66), expected, "{text}");
        }
        assert_eq!(
            layout_for("甲乙丙丁", 70),
            vec![[0, 0], [24, 0], [48, 0], [0, 30]]
        );
        assert_eq!(
            layout_for("甲乙\n丙丁", 70),
            vec![[0, 0], [22, 0], [44, 0], [0, 30], [22, 30]]
        );
        assert_eq!(
            layout_for("甲乙丙っ", 66),
            vec![[0, 0], [22, 0], [44, 0], [0, 30]]
        );
    }
    #[test]
    fn impossible_carry_has_an_explicit_boundary() {
        assert!(positions(&['甲', '」'], &[Some(22), Some(22)], 22, 30).is_err());
    }
    #[test]
    fn wraps_at_boundary_and_retains_explicit_breaks() {
        assert_eq!(
            positions(
                &['A', 'B', 'C', 'D', '\n', 'E'],
                &[Some(22), Some(11), Some(11), Some(22), None, Some(11)],
                44,
                30
            )
            .unwrap(),
            vec![[0, 0], [22, 0], [33, 0], [0, 30], [22, 30], [0, 60]]
        );
        assert!(positions(&['A'], &[Some(45)], 44, 30).is_err());
    }
    #[test]
    fn rejects_invalid_font_and_input_budget() {
        assert!(DialogueRenderer::new(vec![0; 12], 0, b"", 191, true).is_err());
        assert_eq!(
            DialogueRenderer::new(vec![0; MAX_FONT_BYTES + 1], 0, b"", 191, true)
                .err()
                .unwrap()
                .to_string(),
            "font exceeds 40 MiB input budget"
        );
    }
    #[test]
    fn native_message_box_blt_scales_rgb_and_alpha() {
        let source = vec![241, 112, 126, 255, 200, 100, 50, 128];
        let mut image = RgbaImage::from_rgba(2, 1, source.clone(), BUDGET).unwrap();
        box_opacity(&mut image, 191);
        assert_eq!(image.pixels(), &[178, 83, 93, 189, 74, 37, 18, 47]);
        let mut image = RgbaImage::from_rgba(2, 1, source.clone(), BUDGET).unwrap();
        box_opacity(&mut image, 255);
        assert_eq!(image.pixels(), source);
        box_opacity(&mut image, 0);
        assert_eq!(image.pixels(), &[0; 8]);
    }
}
