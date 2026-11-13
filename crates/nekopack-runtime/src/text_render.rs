//! Ordinary Unicode dialogue overlay. Resource geometry and font bytes are host-independent.
//! Includes the evidenced NekoPack punctuation wrapping rules, not a general kinsoku engine.
use ab_glyph::{point, Font, FontVec};
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
struct Ornament {
    image: RgbaImage,
    position: [i32; 2],
    after_name: bool,
    requires_name: bool,
}

fn load_message_asset<R:std::io::Read+std::io::Seek>(store:&mut crate::dat::DatGame<R>,asset:&str)->Result<RgbaImage> {
    if let Some((path,ordinal))=asset.rsplit_once(':') {
        let (directory,file)=path.rsplit_once('/').ok_or("invalid message sprite resource")?;
        Ok(Mng::parse(&store.read(directory,file)?,100000)?.decode_image(ordinal.parse()?,BUDGET)?)
    }else {
        let (directory,file)=asset.rsplit_once('/').ok_or("invalid message image resource")?;
        Ok(nekopack_image::decode_png(&store.read(directory,file)?,BUDGET)?)
    }
}

fn raster_message_sprite<R:std::io::Read+std::io::Seek>(
    store:&mut crate::dat::DatGame<R>,sprite:&crate::message_adapter::SpriteSnapshot,
)->Result<RgbaImage> {
    let mut image=RgbaImage::transparent(sprite.size[0],sprite.size[1],BUDGET)?;
    if let Some(asset)=&sprite.asset {image.draw(&load_message_asset(store,asset)?,[0,0]);}
    for layer in &sprite.layers {
        let mut source=load_message_asset(store,&layer.asset)?;
        for pixel in source.pixels_mut().chunks_exact_mut(4) {pixel[3]=(u16::from(pixel[3])*u16::from(layer.opacity)/255) as u8;}
        image.draw(&source,layer.position);
    }
    Ok(image)
}

pub(crate) fn raster_message_layers<R:std::io::Read+std::io::Seek>(
    store:&mut crate::dat::DatGame<R>,sprite:&crate::message_adapter::SpriteSnapshot,size:[u32;2],
)->Result<Option<RgbaImage>> {
    if sprite.asset.is_none() && sprite.layers.is_empty() {return Ok(None);}
    let mut canvas=RgbaImage::transparent(size[0],size[1],BUDGET)?;
    if let Some(asset)=&sprite.asset {canvas.draw(&load_message_asset(store,asset)?,sprite.position);}
    for layer in &sprite.layers {
        let mut source=load_message_asset(store,&layer.asset)?;
        for pixel in source.pixels_mut().chunks_exact_mut(4) {pixel[3]=(u16::from(pixel[3])*u16::from(layer.opacity)/255) as u8;}
        canvas.draw(&source,[sprite.position[0]+layer.position[0],sprite.position[1]+layer.position[1]]);
    }
    Ok(Some(canvas))
}

fn message_panels(
    snapshot:&crate::message_adapter::MessageBoxSnapshot,opacity:u8,
    mut raster:impl FnMut(&crate::message_adapter::SpriteSnapshot)->Result<RgbaImage>,
)->Result<(Panel,Panel,bool,Option<Ornament>)> {
    use crate::message_adapter::MessageBoxSnapshot;
    let panel=|image:RgbaImage,position:[i32;2],origin:[i32;2],width:i32|->Result<Panel> {
        if width<=0 || origin[0]<0 || origin[1]<0 || origin[0] as u32>=image.width() || origin[1] as u32>=image.height() {
            return Err("invalid current message-box anchor".into());
        }
        Ok(Panel {image,position,origin,width})
    };
    Ok(match snapshot {
        MessageBoxSnapshot::Classic {name,text,name_origin,name_width,text_origin,text_width} => {
            let mut name_image=raster(name)?;let mut text_image=raster(text)?;
            box_opacity(&mut name_image,opacity);box_opacity(&mut text_image,opacity);
            (panel(name_image,name.position,*name_origin,*name_width)?,panel(text_image,text.position,*text_origin,*text_width)?,false,None)
        }
        MessageBoxSnapshot::Shared {base,frame,name,text,frame_after_name,frame_requires_name} => {
            let mut base_image=raster(base)?;
            for pixel in base_image.pixels_mut().chunks_exact_mut(4) {pixel[3]=(u16::from(pixel[3])*u16::from(opacity)/255) as u8;}
            let name_image=RgbaImage::transparent(name.size[0],name.size[1],BUDGET)?;
            let origin=[text.position[0].checked_sub(base.position[0]).ok_or("message text x overflow")?,
                        text.position[1].checked_sub(base.position[1]).ok_or("message text y overflow")?];
            let ornament=if frame.asset.is_some() || !frame.layers.is_empty() {
                Some(Ornament {image:raster(frame)?,position:frame.position,after_name:*frame_after_name,requires_name:*frame_requires_name})
            }else {None};
            (panel(name_image,name.position,[0,0],i32::try_from(name.size[0])?)?,
             panel(base_image,base.position,origin,i32::try_from(text.size[0])?)?,true,ornament)
        }
    })
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
    message_bounds: [i32; 4],
    name: Panel,
    text: Panel,
    shared_box: bool,
    ornament: Option<Ornament>,
    box_snapshot: Option<crate::message_adapter::MessageBoxSnapshot>,
    box_style: Option<TextStyle>,
    overlay: Option<RgbaImage>,
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
    name_size: i32,
    name_position: [i32; 2],
    text_size: i32,
    line_span: i32,
    name_color: u32,
    name_border: Option<u32>,
    text_color: u32,
    text_border: Option<u32>,
}

#[cfg(test)]
mod message_box_tests {
    use super::*;
    use crate::message_adapter::{MessageBoxSnapshot,SpriteSnapshot};
    fn sprite(position:[i32;2],size:[u32;2])->SpriteSnapshot {
        SpriteSnapshot {asset:None,position,size,layers:Vec::new()}
    }
    #[test]
    fn current_classic_box_controls_panel_geometry() {
        let snapshot=MessageBoxSnapshot::Classic {
            name:sprite([11,12],[90,25]),text:sprite([7,40],[230,80]),
            name_origin:[3,4],name_width:70,text_origin:[9,10],text_width:180,
        };
        let (name,text,shared,ornament)=message_panels(&snapshot,255,|s|Ok(RgbaImage::filled(s.size[0],s.size[1],[100,120,140,255],BUDGET)?)).unwrap();
        assert!(!shared);assert!(ornament.is_none());
        assert_eq!(name.position,[11,12]);assert_eq!(name.origin,[3,4]);
        assert_eq!(text.position,[7,40]);assert_eq!(text.origin,[9,10]);assert_eq!(text.width,180);
    }
    #[test]
    fn shared_box_preserves_live_ornament_policy_and_direct_draw_opacity() {
        let mut frame=sprite([10,18],[100,10]);frame.asset=Some("synthetic".into());
        let snapshot=MessageBoxSnapshot::Shared {
            base:sprite([10,20],[200,90]),frame,name:sprite([20,25],[80,20]),text:sprite([25,55],[160,45]),
            frame_after_name:false,frame_requires_name:true,
        };
        let (name,text,shared,ornament)=message_panels(&snapshot,128,|s|Ok(RgbaImage::filled(s.size[0],s.size[1],[90,120,150,255],BUDGET)?)).unwrap();
        assert!(shared);assert_eq!(name.position,[20,25]);assert_eq!(text.origin,[15,35]);assert_eq!(text.width,160);
        assert_eq!(&text.image.pixels()[..4],&[90,120,150,128]);
        let ornament=ornament.unwrap();assert!(!ornament.after_name);assert!(ornament.requires_name);
    }

    #[test]
    #[ignore="requires six original game directories in NEKO_MESSAGE_BOX_GAME_DIRS and NEKO_TEST_FONT; renders only in memory"]
    fn six_original_msgbox_objects_drive_render_panels() {
        use std::io::Read;
        let directories=std::env::var_os("NEKO_MESSAGE_BOX_GAME_DIRS").unwrap();
        let directories:Vec<_>=std::env::split_paths(&directories).collect();
        assert_eq!(directories.len(),6);
        let font=load_font(std::fs::read(std::env::var_os("NEKO_TEST_FONT").unwrap()).unwrap(),0).unwrap();
        for directory in directories {
            let mut paths=Vec::new();
            for entry in std::fs::read_dir(&directory).unwrap() {
                let path=entry.unwrap().path();
                if path.extension().is_some_and(|ext|ext.to_string_lossy().eq_ignore_ascii_case("dat")) {
                    let mut file=std::fs::File::open(&path).unwrap();let mut magic=[0;8];
                    if file.read_exact(&mut magic).is_ok() && &magic==b"NEKOPACK" {paths.push(path);}
                }
            }
            paths.sort();
            let mut store=crate::dat::DatGame::mount_version(paths.iter().map(|p|std::fs::File::open(p).unwrap()),true).unwrap();
            let (mut machine,mut vm)=crate::bootstrap_begin(&mut store,1,&mut std::io::sink(),true).unwrap();
            let snapshot=crate::message_adapter::message_box(&mut vm).unwrap();
            let source=match &snapshot {MessageBoxSnapshot::Classic{name,..}=>name,MessageBoxSnapshot::Shared{base,..}=>base};
            let path=source.asset.as_ref().unwrap().rsplit_once(':').unwrap().0;
            let (dir,file)=path.rsplit_once('/').unwrap();
            let raw=store.read(dir,file).unwrap();let mng=Mng::parse(&raw,100000).unwrap();
            let size=[mng.header[0],mng.header[1]];
            let style=crate::preferences::style(&mut vm).unwrap();
            let mut renderer=DialogueRenderer::from_message_box(font.clone(),&mut store,snapshot.clone(),style,size)
                .unwrap_or_else(|error|panic!("{}: {error}; box={snapshot:?}; size={size:?}",directory.display()));
            match &snapshot {
                MessageBoxSnapshot::Classic{name,text,name_origin,text_origin,text_width,..}=>{
                    assert_eq!(renderer.name.position,name.position);assert_eq!(renderer.name.origin,*name_origin);
                    assert_eq!(renderer.text.position,text.position);assert_eq!(renderer.text.origin,*text_origin);
                    assert_eq!(renderer.text.width,*text_width);
                }
                MessageBoxSnapshot::Shared{base,name,text,..}=>{
                    assert_eq!(renderer.name.position,name.position);assert_eq!(renderer.text.position,base.position);
                    assert_eq!(renderer.text.origin,[text.position[0]-base.position[0],text.position[1]-base.position[1]]);
                    assert_eq!(renderer.text.width,text.size[0] as i32);
                }
            }
            let layout=crate::message_adapter::evaluate_layout(&mut vm,&[":Synthetic::XX".into(),"probe".into()],9,false).unwrap();
            renderer.prepare_layout(1,&layout).unwrap();
            let layers=crate::message_adapter::message_layers(&mut vm,&mut store,&mut machine.variables,true,false,0).unwrap();
            renderer.set_overlay(raster_message_layers(&mut store,&layers.overlay,size).unwrap()).unwrap();
            let image=renderer.frame_pixels(1,5).unwrap();
            assert!(image.width()>0 && image.height()>0);
        }
    }
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
    pub(crate) fn frame_origin(&self)->[i32;2] {[self.bounds[0],self.bounds[1]]}

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
        let (size, name, text, shared_box) = Self::panels(mng, opacity)?;
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
            message_bounds: [x, y, right - x, bottom - y],
            name,
            text,
            shared_box,
            ornament: None,
            box_snapshot: None,
            box_style: None,
            overlay: None,
            border,
            bold,
            page: None,
        })
    }

    fn panels(mng: &[u8], opacity: u8) -> Result<([u32; 2], Panel, Panel, bool)> {
        let parsed = Mng::parse(mng, 4096)?;
        let size = [parsed.header[0], parsed.header[1]];
        RgbaImage::transparent(size[0], size[1], BUDGET)?;
        let sprites = parsed.sprites()?;
        let optional = |key: &[u8]| -> Result<Option<usize>> {
            let mut matches = sprites.iter().enumerate().filter(|(_, s)| s.name == key);
            match (matches.next(), matches.next()) {
                (Some((i, _)), None) => Ok(Some(i)),
                (None, None) => Ok(None),
                _ => Err("ambiguous dialogue sprite".into()),
            }
        };
        let find = |key: &[u8]| -> Result<usize> {
            optional(key)?.ok_or_else(|| "missing dialogue sprite".into())
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
            // Images returns an empty Sprite for a missing optional ornament.
            // BASE and both text anchors remain required for this layout.
            let frame = optional(b"FRAME")?;
            let name = find(b"*NAME")?;
            let [bx, by] = sprites[base].position;
            let [fx, fy] = frame.map_or([bx, by], |frame| sprites[frame].position);
            let [x, y] = [bx.min(fx), by.min(fy)];
            let mut base_image = decode(base)?;
            let frame_image = frame.map(decode).transpose()?;
            let right = (i64::from(bx) + i64::from(base_image.width()))
                .max(i64::from(fx) + i64::from(frame_image.as_ref().map_or(0, RgbaImage::width)))
                .min(i64::from(size[0]));
            let bottom = (i64::from(by) + i64::from(base_image.height()))
                .max(i64::from(fy) + i64::from(frame_image.as_ref().map_or(0, RgbaImage::height)))
                .min(i64::from(size[1]));
            if x < 0 || y < 0 || right <= i64::from(x) || bottom <= i64::from(y) {
                return Err("invalid shared dialogue box bounds".into());
            }
            let mut box_image =
                RgbaImage::transparent(right as u32 - x as u32, bottom as u32 - y as u32, BUDGET)?;
            box_opacity(&mut base_image, opacity);
            box_image.draw(&base_image, [bx - x, by - y]);
            if let Some(frame_image) = frame_image {
                box_image.draw(&frame_image, [fx - x, fy - y]);
            }
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
        Ok((size, name, text, shared_box))
    }

    fn layout(font: &FontVec, text: &str, width: i32, bold: bool) -> Result<Layout> {
        if text.chars().count() > 4096 {
            return Err("dialogue exceeds 4096 scalar budget".into());
        }
        Self::layout_sized(font, text, width, bold, 22, 8)
    }

    fn layout_sized(
        font: &FontVec,
        text: &str,
        width: i32,
        bold: bool,
        size: i32,
        line_span: i32,
    ) -> Result<Layout> {
        // Glyph.Size is pixels per em, rather than ascent/descent-based scale.
        let units = font.units_per_em().ok_or("font has invalid em units")?;
        let factor = size as f32 / units;
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
        let locations = positions(&display_chars, &advances, width, size + line_span)?;
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

    fn color_glyphs(glyphs: &mut [Option<Glyph>], fill: u32, border: Option<u32>) {
        let rgb = |color: u32| [(color >> 16) as u8, (color >> 8) as u8, color as u8];
        for glyph in glyphs.iter_mut().flatten() {
            for pixel in glyph.fill.pixels_mut().chunks_exact_mut(4) {
                pixel[..3].copy_from_slice(&rgb(fill));
            }
            if let Some(border) = border {
                for pixel in glyph.edge.pixels_mut().chunks_exact_mut(4) {
                    pixel[..3].copy_from_slice(&rgb(border));
                }
            }
        }
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
        self.box_snapshot = None;
        self.box_style = None;
        Ok(())
    }

    /// Rebuild from the original VM's current MessageBox, including its own
    /// sprite handles and anchors. No fixed resource filename is inferred.
    pub(crate) fn set_message_box<R: std::io::Read+std::io::Seek>(
        &mut self,store:&mut crate::dat::DatGame<R>,snapshot:crate::message_adapter::MessageBoxSnapshot,
        style:TextStyle,size:[u32;2],
    )->Result<bool> {
        if self.box_snapshot.as_ref()==Some(&snapshot) && self.box_style==Some(style) && self.size==size {return Ok(false);}
        *self=Self::from_message_box(self.font.clone(),store,snapshot,style,size)?;
        Ok(true)
    }

    pub(crate) fn from_message_box<R:std::io::Read+std::io::Seek>(
        font:SharedFont,store:&mut crate::dat::DatGame<R>,snapshot:crate::message_adapter::MessageBoxSnapshot,
        style:TextStyle,size:[u32;2],
    )->Result<Self> {
        let (name,text,shared_box,ornament)=message_panels(&snapshot,style.opacity,|sprite|raster_message_sprite(store,sprite))?;
        let mut bounds=[name.position[0].min(text.position[0]),name.position[1].min(text.position[1]),0,0];
        let mut right=0i64;let mut bottom=0i64;
        for (position,image) in [(&name.position,&name.image),(&text.position,&text.image)]
            .into_iter().chain(ornament.as_ref().map(|v|(&v.position,&v.image))) {
            bounds[0]=bounds[0].min(position[0]);bounds[1]=bounds[1].min(position[1]);
            right=right.max(i64::from(position[0])+i64::from(image.width()));
            bottom=bottom.max(i64::from(position[1])+i64::from(image.height()));
        }
        // Native Screen.Draw clips at the scene edge; ED's original panel
        // extends two pixels past its 1280x720 canvas.
        bounds[0]=bounds[0].max(0);bounds[1]=bounds[1].max(0);
        right=right.min(i64::from(size[0]));bottom=bottom.min(i64::from(size[1]));
        if right<=i64::from(bounds[0]) || bottom<=i64::from(bounds[1]) {return Err("message box outside scene canvas".into());}
        bounds[2]=i32::try_from(right-i64::from(bounds[0]))?;
        bounds[3]=i32::try_from(bottom-i64::from(bounds[1]))?;
        Ok(Self {font,size,bounds,message_bounds:bounds,name,text,shared_box,ornament,
                 bold:style.bold,border:style.border,page:None,box_snapshot:Some(snapshot),box_style:Some(style),overlay:None})
    }

    pub(crate) fn set_overlay(&mut self,overlay:Option<RgbaImage>)->Result<()> {
        if let Some(image)=&overlay {
            if [image.width(),image.height()]!=self.size {return Err("message overlay dimensions differ".into());}
            self.bounds=[0,0,i32::try_from(self.size[0])?,i32::try_from(self.size[1])?];
        }else {self.bounds=self.message_bounds;}
        self.overlay=overlay;
        if let Some(page)=&mut self.page {page.visible=None;page.png.clear();}
        Ok(())
    }
    pub(crate) fn draw_styled(font:&SharedFont,text:&str,target:&mut RgbaImage,position:[i32;2],size:i32,color:u32,bold:bool,border:Option<u32>)->Result<()> {
        if !(1..=512).contains(&size) {return Err("invalid system glyph size".into());}
        let mut layout=Self::layout_sized(font,text,i32::MAX,bold,size,0)?;
        Self::color_glyphs(&mut layout.glyphs,color,border);
        Self::draw_glyphs(target,&layout.glyphs,position,border.is_some());
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
    pub fn prepare(
        &mut self,
        sequence: u64,
        speaker: &str,
        speaker_byte_len: usize,
        text: &str,
    ) -> Result<()> {
        use crate::message_adapter::{MessageLayout, ParsedMessage};
        self.prepare_layout(
            sequence,
            &MessageLayout {
                message: ParsedMessage {
                    name: speaker.into(),
                    voice: String::new(),
                    mask: String::new(),
                    text: text.into(),
                },
                name_color: 0xffffff,
                name_border: self.border.then_some(0),
                text_color: 0xffffff,
                text_border: self.border.then_some(0),
                name_text: speaker.into(),
                name_size: 22,
                name_position: [
                    self.name.origin[0] + (self.name.width - speaker_byte_len as i32 * 11) / 2,
                    self.name.origin[1],
                ],
                text_size: 22,
                line_width: self.text.width,
                line_span: 8,
            },
        )
    }
    pub(crate) fn prepare_layout(
        &mut self,
        sequence: u64,
        layout: &crate::message_adapter::MessageLayout,
    ) -> Result<()> {
        let (name_size, name_position, text_size, line_span) = (
            layout.name_size,
            layout.name_position,
            layout.text_size,
            layout.line_span,
        );
        let name = if layout.name_text.is_empty() {
            None
        } else {
            // Name Print occurs before message.nut assigns LineWidth for the body.
            // Long names remain on one line and are clipped by the NAME sprite.
            let mut glyphs = Self::layout_sized(
                &self.font,
                &layout.name_text,
                i32::MAX,
                self.bold,
                layout.name_size,
                0,
            )?;
            Self::color_glyphs(&mut glyphs.glyphs, layout.name_color, layout.name_border);
            let mut image = RgbaImage::from_rgba(
                self.name.image.width(),
                self.name.image.height(),
                self.name.image.pixels().to_vec(),
                BUDGET,
            )?;
            Self::draw_glyphs(
                &mut image,
                &glyphs.glyphs,
                layout.name_position,
                self.border && layout.name_border.is_some(),
            );
            Some(image)
        };
        let line_height = layout.text_size + layout.line_span;
        let mut raster = Self::layout_sized(
            &self.font,
            &layout.message.text,
            layout.line_width,
            self.bold,
            layout.text_size,
            layout.line_span,
        )?;
        Self::color_glyphs(&mut raster.glyphs, layout.text_color, layout.text_border);
        let glyphs = raster.glyphs;
        let clipped = glyphs.iter().flatten().any(|g| {
            g.position[1] + self.text.origin[1] + g.fill.height() as i32
                > self.text.image.height() as i32
                || g.position[0] + self.text.origin[0] + g.fill.width() as i32
                    > self.text.image.width() as i32
        });
        let lines = raster
            .origins
            .iter()
            .map(|origin| (origin[1] / line_height + 1) as usize)
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
            origins: raster.origins,
            name_color: layout.name_color,
            name_border: layout.name_border,
            text_color: layout.text_color,
            text_border: layout.text_border,
            name_size,
            name_position,
            text_size,
            line_span,
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
            self.border && page.text_border.is_some(),
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
            if self.ornament.as_ref().is_some_and(|v|v.after_name) {draw_name(&mut image);}
            if let Some(ornament)=&self.ornament {
                if !ornament.requires_name || page.name.is_some() {
                    image.draw(&ornament.image,[ornament.position[0]-self.bounds[0],ornament.position[1]-self.bounds[1]]);
                }
            }
            if self.ornament.as_ref().is_none_or(|v|!v.after_name) {draw_name(&mut image);}
        }
        if let Some(overlay)=&self.overlay {image.draw(overlay,[-self.bounds[0],-self.bounds[1]]);}
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
            "{{\"sequence\":{},\"size\":[{},{}],\"bounds\":[{},{},{},{}],\"font_size\":{},\"text_size\":{},\"line_span\":{},\"name_size\":{},\"name_position\":{:?},\"name_color\":{},\"name_border\":{},\"text_color\":{},\"text_border\":{},\"lines\":{},\"clipped\":{},\"origins\":{:?},\"text_rect\":[{},{},{},{}],\"name_rect\":[{},{},{},{}]}}",
            p.sequence,
            self.size[0],
            self.size[1],
            self.bounds[0],
            self.bounds[1],
            self.bounds[2],
            self.bounds[3],
            p.text_size,
            p.text_size,
            p.line_span,
            p.name_size,
            p.name_position,
            p.name_color,
            p.name_border.map_or("null".to_owned(), |v| v.to_string()),
            p.text_color,
            p.text_border.map_or("null".to_owned(), |v| v.to_string()),
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
    fn shared_box_fixture(labels: &[&str]) -> Vec<u8> {
        fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], body: &[u8]) {
            out.extend((body.len() as u32).to_be_bytes());
            out.extend(kind);
            out.extend(body);
            let mut crc = !0u32;
            for byte in kind.iter().chain(body) {
                crc ^= u32::from(*byte);
                for _ in 0..8 {
                    crc = (crc >> 1) ^ if crc & 1 != 0 { 0xedb88320 } else { 0 };
                }
            }
            out.extend((!crc).to_be_bytes());
        }
        let mut out = b"\x8aMNG\r\n\x1a\n".to_vec();
        chunk(
            &mut out,
            b"MHDR",
            &[100u32, 100, 0, 0, 0, 0, 0]
                .into_iter()
                .flat_map(u32::to_be_bytes)
                .collect::<Vec<_>>(),
        );
        for label in labels {
            let (position, size) = match *label {
                "BASE" => ([0i32, 40], [100, 60]),
                "*NAME" => ([10, 45], [30, 15]),
                "*TEXT" => ([10, 70], [80, 20]),
                "FRAME" => ([0, 35], [100, 5]),
                _ => unreachable!(),
            };
            let mut frame = vec![2];
            frame.extend(label.as_bytes());
            chunk(&mut out, b"FRAM", &frame);
            let mut definition = vec![0; 4];
            definition.extend(position.into_iter().flat_map(i32::to_be_bytes));
            chunk(&mut out, b"DEFI", &definition);
            let mut png = Vec::new();
            RgbaImage::filled(size[0], size[1], [100, 100, 100, 255], BUDGET)
                .unwrap()
                .write_png(&mut png)
                .unwrap();
            out.extend(&png[8..]);
        }
        chunk(&mut out, b"MEND", &[]);
        out
    }
    #[test]
    #[ignore = "requires NEKO_TEST_FONT; validates synthetic glyph RGBA values without viewing images"]
    fn script_fill_and_border_colors_reach_actual_glyph_pixels() {
        let font = load_font(std::fs::read(std::env::var_os("NEKO_TEST_FONT").unwrap()).unwrap(), 0).unwrap();
        for (fill, border) in [(0x123456, 0x654321), (0xabcdef, 0xfedcba), (0xffffff, 0)] {
            let mut layout = DialogueRenderer::layout_sized(&font, "AB", 128, false, 48, 8).unwrap();
            let alpha: Vec<_> = layout.glyphs.iter().flatten().flat_map(|g| g.fill.pixels().chunks_exact(4).map(|p| p[3])).collect();
            DialogueRenderer::color_glyphs(&mut layout.glyphs, fill, Some(border));
            let actual: Vec<_> = layout.glyphs.iter().flatten().flat_map(|g| g.fill.pixels().chunks_exact(4).map(|p| p[3])).collect();
            assert_eq!(actual, alpha);
            let mut image = RgbaImage::transparent(128, 64, BUDGET).unwrap();
            DialogueRenderer::draw_glyphs(&mut image, &layout.glyphs, [2, 0], true);
            for color in [fill, border] {
                let rgba = [(color >> 16) as u8, (color >> 8) as u8, color as u8, 255];
                assert!(image.pixels().chunks_exact(4).any(|pixel| pixel == rgba));
            }
        }
        let mut renderer = DialogueRenderer::with_font(
            font, &shared_box_fixture(&["BASE", "*NAME", "*TEXT"]),
            TextStyle { opacity:255, bold:false, border:true },
        ).unwrap();
        renderer.prepare_layout(9, &crate::message_adapter::MessageLayout {
            message:crate::message_adapter::ParsedMessage { name:"█".into(), voice:String::new(), mask:String::new(), text:"█".into() },
            name_text:"█".into(), name_size:14, name_position:[0,0],
            text_size:14,line_width:80,line_span:8,
            name_color:0x123456,name_border:Some(0x654321),
            text_color:0xabcdef,text_border:Some(0xfedcba),
        }).unwrap();
        let frame = renderer.frame_pixels(9, 1).unwrap();
        for rgb in [[0x12,0x34,0x56], [0xab,0xcd,0xef]] {
            assert!(frame.pixels().chunks_exact(4).any(|p| p[..3]==rgb && p[3]==255));
        }
    }

    #[test]
    fn shared_dialogue_base_does_not_require_optional_frame() {
        let mng = shared_box_fixture(&["BASE", "*NAME", "*TEXT"]);
        let (size, name, text, shared) = DialogueRenderer::panels(&mng, 255).unwrap();
        assert_eq!(size, [100, 100]);
        assert!(shared);
        assert_eq!(name.position, [10, 45]);
        assert_eq!(name.origin, [0, 0]);
        assert_eq!(text.position, [0, 40]);
        assert_eq!(text.origin, [10, 30]);
        assert_eq!([text.image.width(), text.image.height()], [100, 60]);
        let framed = shared_box_fixture(&["BASE", "FRAME", "*NAME", "*TEXT"]);
        let (_, _, text, _) = DialogueRenderer::panels(&framed, 255).unwrap();
        assert_eq!(text.position, [0, 35]);
        assert_eq!(text.origin, [10, 35]);
    }
    #[test]
    fn missing_required_anchors_and_duplicate_ornaments_are_rejected() {
        for labels in [
            &["BASE", "*NAME"][..],
            &["BASE", "FRAME", "FRAME", "*NAME", "*TEXT"][..],
        ] {
            assert!(DialogueRenderer::panels(&shared_box_fixture(labels), 255).is_err());
        }
    }
    #[test]
    #[ignore = "requires NEKO_FONT_GAME and NEKO_TEST_FONT; renders privately without exporting story or art"]
    fn local_original_font_reaches_first_rendered_page() {
        use std::{fs::File, io::Read};
        let root = std::path::PathBuf::from(std::env::var_os("NEKO_FONT_GAME").unwrap());
        let files = std::fs::read_dir(root)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "dat"))
            .filter(|path| {
                let mut signature = [0; 8];
                File::open(path)
                    .unwrap()
                    .read_exact(&mut signature)
                    .unwrap();
                signature == *b"NEKOPACK"
            })
            .map(|path| {
                (
                    path.file_name().unwrap().to_str().unwrap().to_owned(),
                    File::open(path).unwrap(),
                )
            });
        let store = crate::dat::DatGame::mount_named(files, true).unwrap();
        let mut story = crate::title::TitleSession::open(store)
            .unwrap()
            .start()
            .unwrap();
        let font = std::fs::read(std::env::var_os("NEKO_TEST_FONT").unwrap()).unwrap();
        story.install_dialogue_font(font, 0).unwrap();
        story.enable_interactive_text();
        story.enable_host_effects();
        let mut packet = story.poll_packet().unwrap();
        for _ in 0..64 {
            if let Ok(page) = story.page() {
                let sequence = page.sequence;
                story.text_frame(sequence, 0, false).unwrap();
                story.text_frame(sequence, 10_000, true).unwrap();
                let rendered = story.dialogue_image(sequence, false).unwrap();
                let split = rendered.iter().position(|b| *b == b'\n').unwrap();
                assert!(rendered[split + 1..].starts_with(b"\x89PNG\r\n\x1a\n"));
                return;
            }
            let header =
                std::str::from_utf8(packet.split(|b| *b == b'\n').next().unwrap()).unwrap();
            let token = header
                .split("\"token\":")
                .nth(1)
                .unwrap()
                .split([',', '}'])
                .next()
                .unwrap()
                .parse()
                .unwrap();
            packet = story.acknowledge_effect(token).unwrap();
        }
        panic!("first rendered page did not arrive within 64 host effects");
    }
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
    #[ignore = "requires NEKO_TEST_FONT; synthetic text and private glyph geometry only"]
    fn script_size_and_line_span_reach_the_rasterizer() {
        let font = load_font(
            std::fs::read(std::env::var_os("NEKO_TEST_FONT").unwrap()).unwrap(),
            0,
        )
        .unwrap();
        let regular = DialogueRenderer::layout_sized(&font, "甲\n乙", 600, false, 30, 8).unwrap();
        assert_eq!(regular.origins, [[0, 0], [30, 0], [0, 38]]);
        let larger = DialogueRenderer::layout_sized(&font, "甲\n乙", 600, false, 86, 11).unwrap();
        assert_eq!(larger.origins, [[0, 0], [86, 0], [0, 97]]);
        assert!(
            larger.glyphs[0].as_ref().unwrap().fill.height()
                > regular.glyphs[0].as_ref().unwrap().fill.height()
        );
        let legacy = DialogueRenderer::layout(&font, "甲\n乙", 600, false).unwrap();
        assert_eq!(legacy.origins, [[0, 0], [22, 0], [0, 30]]);
        let mut renderer = DialogueRenderer::with_font(
            font,
            &shared_box_fixture(&["BASE", "*NAME", "*TEXT"]),
            TextStyle {
                opacity: 255,
                bold: false,
                border: false,
            },
        )
        .unwrap();
        renderer
            .prepare_layout(
                7,
                &crate::message_adapter::MessageLayout {
                    message: crate::message_adapter::ParsedMessage {
                        name: "Unit".into(),
                        voice: String::new(),
                        mask: String::new(),
                        text: "A\nB".into(),
                    },
                    name_color: 0xffffff,
                    name_border: None,
                    text_color: 0xffffff,
                    text_border: None,
                    name_text: "Unit".into(),
                    name_size: 31,
                    name_position: [2, 3],
                    text_size: 42,
                    line_width: 80,
                    line_span: 11,
                },
            )
            .unwrap();
        let metadata = renderer.metadata().unwrap();
        assert!(metadata.contains("\"name_size\":31,\"name_position\":[2, 3]"));
        assert!(metadata.contains("\"font_size\":42,\"text_size\":42,\"line_span\":11"));
        assert!(metadata.contains("\"text_color\":16777215,\"text_border\":null"));
        assert!(!metadata.contains("Unit"));
    }
    #[test]
    #[ignore = "requires NEKO_TEST_FONT and NEKO_TEST_MSGBOX local assets; synthetic text only"]
    fn chinese_name_renders_and_rebuilds_without_cp932_conversion() {
        let font = std::fs::read(std::env::var_os("NEKO_TEST_FONT").unwrap()).unwrap();
        let mng = std::fs::read(std::env::var_os("NEKO_TEST_MSGBOX").unwrap()).unwrap();
        let mut renderer = DialogueRenderer::new(font, 0, &mng, 191, true).unwrap();
        assert!(encoding_rs::SHIFT_JIS.encode("赵晓").2);
        renderer.prepare(1, "赵晓", 4, "probe").unwrap();
        let original = renderer
            .page
            .as_ref()
            .unwrap()
            .name
            .as_ref()
            .unwrap()
            .pixels()
            .to_vec();
        assert_ne!(original, renderer.name.image.pixels());
        renderer.prepare(2, "赵晓", 6, "probe").unwrap();
        assert_ne!(
            original,
            renderer
                .page
                .as_ref()
                .unwrap()
                .name
                .as_ref()
                .unwrap()
                .pixels()
        );
        renderer
            .set_style(
                &mng,
                TextStyle {
                    opacity: 191,
                    bold: true,
                    border: true,
                },
            )
            .unwrap();
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
