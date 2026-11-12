//! Ordinary Unicode dialogue overlay. Resource geometry and font bytes are host-independent.
//! Includes the evidenced NekoPack punctuation wrapping rules, not a general kinsoku engine.
use ab_glyph::{Font, FontVec, point};
use nekopack_image::{Mng, RgbaImage};
use std::{error::Error, sync::Arc};
const BUDGET: usize = 16 * 1024 * 1024;
/// Bounded host font input; Android 15's CJK collection exceeds 30 MiB.
pub const MAX_FONT_BYTES: usize = 40 * 1024 * 1024;
pub(crate) type SharedFont = Arc<FontVec>;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TextStyle {
    pub opacity: u8,
    pub bold: bool,
    pub border: bool,
}
pub(crate) fn load_font(bytes: Vec<u8>, index: u32) -> Result<SharedFont> {
    if bytes.len() > MAX_FONT_BYTES {
        return Err("font exceeds 40 MiB input budget".into());
    }
    Ok(Arc::new(FontVec::try_from_vec_and_index(bytes, index)?))
}
// Host synthetic weight: one rightward pixel, without changing advances.
fn embolden(fill: &mut RgbaImage) {
    let w = fill.width() as usize;
    for y in 0..fill.height() as usize {
        for x in (1..w).rev() {
            let at = (y * w + x) * 4;
            let left = fill.pixels()[at - 1];
            let alpha = fill.pixels()[at + 3].max(left);
            fill.pixels_mut()[at..at + 4].copy_from_slice(&[255, 255, 255, alpha]);
        }
    }
}
type Result<T> = std::result::Result<T, Box<dyn Error>>;

pub(crate) fn draw_log_text(
    canvas: &mut RgbaImage,
    font: &FontVec,
    text: &str,
    padding: i32,
    center: bool,
) -> Result<()> {
    let factor = 22.0 / font.units_per_em().ok_or("invalid font em")?;
    let scale = font.height_unscaled() * factor;
    let baseline = font.ascent_unscaled() * factor;
    let chars: Vec<_> = text.chars().collect();
    let advances: Vec<_> = chars
        .iter()
        .map(|c| {
            if *c == '\n' {
                None
            } else {
                Some((font.h_advance_unscaled(font.glyph_id(*c)) * factor).round() as i32)
            }
        })
        .collect();
    let width = canvas.width() as i32 - padding;
    let locations = positions(&chars, &advances, width, 30)?;
    let shift = if center {
        (width - advances.iter().flatten().sum::<i32>()) / 2
    } else {
        0
    };
    for (c, [x, y]) in chars.iter().zip(locations) {
        if *c == '\n' {
            continue;
        }
        if let Some(outline) = font.outline_glyph(
            font.glyph_id(*c)
                .with_scale_and_position(scale, point((x + shift) as f32, y as f32 + baseline)),
        ) {
            let bounds = outline.px_bounds();
            outline.draw(|gx, gy, a| {
                let px = bounds.min.x as i32 + gx as i32;
                let py = bounds.min.y as i32 + gy as i32;
                if px >= 0
                    && py >= 0
                    && (px as u32) < canvas.width()
                    && (py as u32) < canvas.height()
                {
                    let i = ((py as u32 * canvas.width() + px as u32) * 4) as usize;
                    let alpha = canvas.pixels()[i + 3].max((a * 255.0).round() as u8);
                    canvas.pixels_mut()[i..i + 4].copy_from_slice(&[255, 255, 255, alpha]);
                }
            });
        }
    }
    Ok(())
}

pub(crate) fn draw_save_label(
    canvas: &mut RgbaImage,
    font: &FontVec,
    text: &str,
    position: [i32; 2],
    size: f32,
    color: [u8; 3],
    width: i32,
) -> Result<()> {
    use ab_glyph::ScaleFont;
    let scaled = font.as_scaled(size);
    let mut x = position[0] as f32;
    for ch in text.chars() {
        let id = font.glyph_id(ch);
        let advance = scaled.h_advance(id);
        if x + advance > (position[0] + width) as f32 {
            break;
        }
        let glyph =
            id.with_scale_and_position(size, point(x, position[1] as f32 + scaled.ascent()));
        if let Some(outline) = font.outline_glyph(glyph) {
            let b = outline.px_bounds();
            let w = b.width() as u32 + 1;
            let h = b.height() as u32;
            if w > 0 && h > 0 {
                let mut image = RgbaImage::transparent(w, h, BUDGET)?;
                outline.draw(|gx, gy, a| {
                    let at = ((gy * w + gx) * 4) as usize;
                    image.pixels_mut()[at..at + 4].copy_from_slice(&[
                        color[0],
                        color[1],
                        color[2],
                        (a * 255.0).round() as u8,
                    ]);
                });
                for y in 0..h {
                    for x in (1..w).rev() {
                        let at = ((y * w + x) * 4) as usize;
                        let alpha = image.pixels()[at + 3].max(image.pixels()[at - 1]);
                        image.pixels_mut()[at..at + 4]
                            .copy_from_slice(&[color[0], color[1], color[2], alpha]);
                    }
                }
                canvas.draw(&image, [b.min.x as i32, b.min.y as i32]);
            }
        }
        x += advance;
    }
    Ok(())
}

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
    font: SharedFont,
    size: [u32; 2],
    bounds: [i32; 4],
    name: Panel,
    text: Panel,
    shared_box: bool,
    border: bool,
    bold: bool,
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
        Self::with_font(
            load_font(font, collection_index)?,
            mng,
            TextStyle {
                opacity,
                border,
                bold: false,
            },
        )
    }
    pub(crate) fn with_font(font: SharedFont, mng: &[u8], style: TextStyle) -> Result<Self> {
        let TextStyle {
            opacity,
            border,
            bold,
        } = style;
        let parsed = Mng::parse(mng, 4096)?;
        let size = [parsed.header[0], parsed.header[1]];
        RgbaImage::transparent(size[0], size[1], BUDGET)?;
        let sprites = parsed.sprites()?;
        let find = |key: &[u8]| -> Result<usize> {
            let mut matches = sprites.iter().enumerate().filter(|(_, s)| s.name == key);
            match (matches.next(), matches.next()) {
                (Some((i, _)), None) => Ok(i),
                _ => Err("missing or ambiguous dialogue sprite".into()),
            }
        };
        let decode = |i: usize| parsed.decode_image(sprites[i].ordinal, BUDGET);
        let panel = |image: RgbaImage, position: [i32; 2], anchor: &[u8]| -> Result<Panel> {
            let j = find(anchor)?;
            let area = decode(j)?;
            let origin = [
                sprites[j].position[0]
                    .checked_sub(position[0])
                    .ok_or("dialogue anchor x overflow")?,
                sprites[j].position[1]
                    .checked_sub(position[1])
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
                position,
                origin,
                width: area.width() as i32,
            })
        };
        let shared_box = sprites.iter().any(|sprite| sprite.name == b"BASE");
        let (name, text) = if shared_box {
            let base = find(b"BASE")?;
            let frame = find(b"FRAME")?;
            let name = find(b"*NAME")?;
            let [bx, by] = sprites[base].position;
            let [fx, fy] = sprites[frame].position;
            let [x, y] = [bx.min(fx), by.min(fy)];
            let mut base_image = decode(base)?;
            let frame_image = decode(frame)?;
            let right = (i64::from(bx) + i64::from(base_image.width()))
                .max(i64::from(fx) + i64::from(frame_image.width()))
                .min(i64::from(size[0]));
            let bottom = (i64::from(by) + i64::from(base_image.height()))
                .max(i64::from(fy) + i64::from(frame_image.height()))
                .min(i64::from(size[1]));
            if x < 0 || y < 0 || right <= i64::from(x) || bottom <= i64::from(y) {
                return Err("invalid shared dialogue box bounds".into());
            }
            let mut box_image =
                RgbaImage::transparent(right as u32 - x as u32, bottom as u32 - y as u32, BUDGET)?;
            box_opacity(&mut base_image, opacity);
            box_image.draw(&base_image, [bx - x, by - y]);
            box_image.draw(&frame_image, [fx - x, fy - y]);
            let area = decode(name)?;
            let name_image = RgbaImage::transparent(area.width(), area.height(), BUDGET)?;
            (
                panel(name_image, sprites[name].position, b"*NAME")?,
                panel(box_image, [x, y], b"*TEXT")?,
            )
        } else {
            let classic = |key: &[u8], anchor: &[u8]| {
                let i = find(key)?;
                let mut image = decode(i)?;
                box_opacity(&mut image, opacity);
                panel(image, sprites[i].position, anchor)
            };
            (classic(b"NAME", b"*NAME")?, classic(b"TEXT", b"*TEXT")?)
        };
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
            shared_box,
            border,
            bold,
            page: None,
        })
    }

    fn layout(font: &FontVec, text: &str, width: i32, bold: bool) -> Result<Layout> {
        if text.chars().count() > 4096 {
            return Err("dialogue exceeds 4096 scalar budget".into());
        }
        // 22 pixels per em; normalize ab_glyph's ascent/descent-based scale explicitly.
        let units = font.units_per_em().ok_or("font has invalid em units")?;
        let factor = 22.0 / units;
        let scale = font.height_unscaled() * factor;
        let baseline = font.ascent_unscaled() * factor;
        let mut advances = Vec::new();
        let display_chars: Vec<_> = text
            .chars()
            .map(|ch| if ch == '\t' { ' ' } else { ch })
            .collect();
        for &ch in &display_chars {
            if ch == '\n' {
                advances.push(None);
                continue;
            }
            if ch.is_control() {
                return Err("unsupported dialogue control character".into());
            }
            let id = font.glyph_id(ch);
            if id.0 == 0 {
                return Err(format!("font missing U+{:04X}", ch as u32).into());
            }
            advances.push(Some((font.h_advance_unscaled(id) * factor).round() as i32));
        }
        let locations = positions(&display_chars, &advances, width, 30)?;
        let mut used = 0usize;
        let glyphs = display_chars
            .into_iter()
            .zip(locations.iter().copied())
            .map(|(ch, [x, y])| {
                if ch == '\n' {
                    return Ok(None);
                }
                let glyph = font
                    .glyph_id(ch)
                    .with_scale_and_position(scale, point(x as f32, y as f32 + baseline));
                let Some(outline) = font.outline_glyph(glyph) else {
                    return Ok(None);
                };
                let bounds = outline.px_bounds();
                let w = (bounds.max.x - bounds.min.x) as u32;
                let h = (bounds.max.y - bounds.min.y) as u32;
                if w > 128 || h > 128 {
                    return Err("glyph raster exceeds 128px extent".into());
                }
                let padded_w = w + 2 + u32::from(bold);
                used += (padded_w as usize) * (h as usize + 2) * 8;
                if used > BUDGET {
                    return Err("dialogue glyph cache budget exceeded".into());
                }
                let mut fill = RgbaImage::transparent(padded_w, h + 2, BUDGET)?;
                outline.draw(|x, y, coverage| {
                    let i = (((y + 1) * (padded_w) + x + 1) * 4) as usize;
                    fill.pixels_mut()[i..i + 4].copy_from_slice(&[
                        255,
                        255,
                        255,
                        (coverage * 255.0).round() as u8,
                    ]);
                });
                if bold {
                    embolden(&mut fill);
                }
                let mut edge = RgbaImage::transparent(padded_w, h + 2, BUDGET)?;
                for y in 0..h + 2 {
                    for x in 0..padded_w {
                        let mut alpha = 0;
                        for dy in -1..=1 {
                            for dx in -1..=1 {
                                let xx = x as i32 + dx;
                                let yy = y as i32 + dy;
                                if xx >= 0
                                    && yy >= 0
                                    && xx < (padded_w) as i32
                                    && yy < (h + 2) as i32
                                {
                                    alpha = alpha.max(
                                        fill.pixels()[((yy as u32 * (padded_w) + xx as u32) * 4 + 3)
                                            as usize],
                                    );
                                }
                            }
                        }
                        edge.pixels_mut()[((y * (padded_w) + x) * 4 + 3) as usize] = alpha;
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
    pub(crate) fn set_style(&mut self, mng: &[u8], style: TextStyle) -> Result<()> {
        let updated = Self::with_font(self.font.clone(), mng, style)?;
        self.name = updated.name;
        self.text = updated.text;
        self.bold = style.bold;
        self.border = style.border;
        Ok(())
    }
    pub(crate) fn draw_preview(
        font: &SharedFont,
        text: &str,
        target: &mut RgbaImage,
        position: [i32; 2],
        bold: bool,
        border: bool,
    ) -> Result<()> {
        let layout = Self::layout(font, text, i32::MAX, bold)?;
        Self::draw_glyphs(target, &layout.glyphs, position, border);
        Ok(())
    }
    pub fn prepare(&mut self, sequence: u64, speaker: &str, speaker_byte_len: usize, text: &str) -> Result<()> {
        let name = if speaker.is_empty() {
            None
        } else {
            let x = (self.name.width - speaker_byte_len as i32 * 11) / 2;
            // Name Print occurs before message.nut assigns LineWidth for the body.
            // Long names remain on one line and are clipped by the NAME sprite.
            let glyphs = Self::layout(&self.font, speaker, i32::MAX, self.bold)?;
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
        let layout = Self::layout(&self.font, text, self.text.width, self.bold)?;
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
    pub(crate) fn frame_pixels(&self, sequence: u64, visible: usize) -> Result<RgbaImage> {
        let page = self.page.as_ref().ok_or("no prepared dialogue image")?;
        if page.sequence != sequence || visible > page.glyphs.len() {
            return Err("stale or invalid dialogue image frame".into());
        }
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
        let draw_name = |image: &mut RgbaImage| {
            if let Some(name) = &page.name {
                image.draw(
                    name,
                    [
                        self.name.position[0] - self.bounds[0],
                        self.name.position[1] - self.bounds[1],
                    ],
                );
            }
        };
        if !self.shared_box {
            draw_name(&mut image);
        }
        image.draw(
            &text,
            [
                self.text.position[0] - self.bounds[0],
                self.text.position[1] - self.bounds[1],
            ],
        );
        if self.shared_box {
            draw_name(&mut image);
        }
        Ok(image)
    }

    pub fn frame(&mut self, sequence: u64, visible: usize) -> Result<Vec<u8>> {
        let page = self.page.as_ref().ok_or("no prepared dialogue image")?;
        if page.sequence != sequence || page.visible != Some(visible) {
            let image = self.frame_pixels(sequence, visible)?;
            let page = self.page.as_mut().unwrap();
            page.png.clear();
            image.write_png(&mut page.png)?;
            page.visible = Some(visible);
        }
        Ok(self.page.as_ref().unwrap().png.clone())
    }
    pub fn metadata(&self) -> Result<String> {
        let p = self.page.as_ref().ok_or("no prepared dialogue image")?;
        Ok(format!(
            "{{\"sequence\":{},\"size\":[{},{}],\"bounds\":[{},{},{},{}],\"font_size\":22,\"line_span\":8,\"lines\":{},\"clipped\":{},\"origins\":{:?},\"text_rect\":[{},{},{},{}],\"name_rect\":[{},{},{},{}]}}",
            p.sequence,
            self.size[0],
            self.size[1],
            self.bounds[0],
            self.bounds[1],
            self.bounds[2],
            self.bounds[3],
            p.lines,
            p.clipped,
            p.origins,
            self.text.position[0],
            self.text.position[1],
            self.text.image.width(),
            self.text.image.height(),
            self.name.position[0],
            self.name.position[1],
            self.name.image.width(),
            self.name.image.height()
        ))
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
    fn synthetic_bold_expands_once_and_preserves_coverage() {
        let mut image = RgbaImage::transparent(6, 1, 24).unwrap();
        image.pixels_mut()[7] = 128;
        image.pixels_mut()[11] = 255;
        embolden(&mut image);
        let alpha: Vec<_> = image
            .pixels()
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| p[3])
            .collect();
        assert_eq!(alpha, [0, 128, 255, 255, 0, 0]);
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
    #[ignore = "requires NEKO_TEST_FONT and NEKO_TEST_MSGBOX local assets; synthetic text only"]
    fn chinese_name_renders_and_rebuilds_without_cp932_conversion() {
        let font = std::fs::read(std::env::var_os("NEKO_TEST_FONT").unwrap()).unwrap();
        let mng = std::fs::read(std::env::var_os("NEKO_TEST_MSGBOX").unwrap()).unwrap();
        let mut renderer = DialogueRenderer::new(font, 0, &mng, 191, true).unwrap();
        assert!(encoding_rs::SHIFT_JIS.encode("赵晓").2);
        renderer.prepare(1, "赵晓", 4, "probe").unwrap();
        let original = renderer.page.as_ref().unwrap().name.as_ref().unwrap().pixels().to_vec();
        assert_ne!(original, renderer.name.image.pixels());
        renderer.prepare(2, "赵晓", 6, "probe").unwrap();
        assert_ne!(original, renderer.page.as_ref().unwrap().name.as_ref().unwrap().pixels());
        renderer.set_style(&mng, TextStyle { opacity: 191, bold: true, border: true }).unwrap();
        renderer.prepare(3, "赵晓", 4, "probe").unwrap();
        assert!(renderer.page.as_ref().unwrap().name.is_some());
        renderer.prepare(4, "", 0, "probe").unwrap();
        assert!(renderer.page.as_ref().unwrap().name.is_none());
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
