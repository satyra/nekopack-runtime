use crate::dat::DatGame;
use nekopack_image::{Mng, RgbaImage};
use std::{
    collections::BTreeMap,
    error::Error,
    io::{Read, Seek},
};

const CLASSIC_DURATION: u64 = 130_000;
const PANNING_DURATION: u64 = 240_000;
const BUDGET: usize = 64 * 1024 * 1024;
type Result<T> = std::result::Result<T, Box<dyn Error>>;

struct Sprite {
    image: RgbaImage,
    position: [i32; 2],
}

struct ClassicCredits {
    base: Sprite,
    frame: Sprite,
    picture_origin: [i32; 2],
    pictures: Vec<Sprite>,
    finish: Sprite,
    text: Vec<Sprite>,
    scroll_limit: i32,
}

fn sprites<R: Read + Seek>(
    store: &mut DatGame<R>,
    name: &str,
    budget: &mut usize,
) -> Result<BTreeMap<Vec<u8>, Sprite>> {
    let bytes = store.read("system", name)?;
    let mng = Mng::parse(&bytes, 100_000)?;
    let mut result = BTreeMap::new();
    for sprite in mng.sprites()? {
        let image = mng.decode_image(sprite.ordinal, *budget)?;
        *budget -= image.pixels().len();
        result.insert(
            sprite.name.to_vec(),
            Sprite {
                image,
                position: sprite.position,
            },
        );
    }
    Ok(result)
}

fn take(map: &mut BTreeMap<Vec<u8>, Sprite>, key: &[u8]) -> Result<Sprite> {
    map.remove(key)
        .ok_or_else(|| format!("missing credits sprite {}", String::from_utf8_lossy(key)).into())
}

fn draw_alpha(
    canvas: &mut RgbaImage,
    sprite: &Sprite,
    position: [i32; 2],
    alpha: i32,
) -> Result<()> {
    let alpha = alpha.clamp(0, 256) as u32;
    if alpha == 0 {
        return Ok(());
    }
    if alpha == 256 {
        canvas.draw(&sprite.image, position);
        return Ok(());
    }
    let mut image = RgbaImage::from_rgba(
        sprite.image.width(),
        sprite.image.height(),
        sprite.image.pixels().to_vec(),
        BUDGET,
    )?;
    for pixel in image.pixels_mut().as_chunks_mut::<4>().0 {
        pixel[3] = (u32::from(pixel[3]) * alpha / 256) as u8;
    }
    canvas.draw(&image, position);
    Ok(())
}

impl ClassicCredits {
    pub(super) fn load<R: Read + Seek>(store: &mut DatGame<R>) -> Result<Self> {
        let mut budget = BUDGET;
        let mut layout = sprites(store, "staff_roll.mng", &mut budget)?;
        let base = take(&mut layout, b"BASE")?;
        let frame = take(&mut layout, b"FRAME")?;
        let picture_origin = take(&mut layout, b"IMAGE")?.position;
        let mut photos = sprites(store, "staff_roll_image.mng", &mut budget)?;
        let finish = take(&mut photos, b"finish")?;
        let mut pictures = Vec::new();
        for i in 0..30 {
            let Some(mut photo) = photos.remove(format!("{i:02}").as_bytes()) else {
                break;
            };
            for pixel in photo.image.pixels_mut().as_chunks_mut::<4>().0 {
                pixel[3] = 255;
            }
            pictures.push(photo);
        }
        if pictures.is_empty() {
            return Err("credits contain no pictures".into());
        }
        let (text, scroll_limit) = credit_text(store, &mut budget)?;
        Ok(Self {
            base,
            frame,
            picture_origin,
            pictures,
            finish,
            text,
            scroll_limit,
        })
    }

    pub(super) fn frame(&self, elapsed_ms: u64) -> Result<RgbaImage> {
        let t = elapsed_ms.min(CLASSIC_DURATION) as i32;
        let mut canvas = RgbaImage::filled(1280, 720, [0, 0, 0, 255], BUDGET)?;
        canvas.draw(&self.base.image, self.base.position);
        let photo_time = t - 8000;
        let span = 120_000 / (self.pictures.len() as i32 + 1);
        if photo_time >= 0 {
            let index = (photo_time / span) as usize;
            let photo = &self.pictures[index.min(self.pictures.len() - 1)];
            canvas.draw(&photo.image, self.picture_origin);
            if index >= self.pictures.len() {
                draw_alpha(
                    &mut canvas,
                    &self.finish,
                    self.picture_origin,
                    (photo_time - self.pictures.len() as i32 * span) / 8,
                )?;
            } else {
                let alpha = (256 - (photo_time % span) / 8).clamp(0, 256);
                if alpha > 0 {
                    let white = RgbaImage::filled(
                        675,
                        380,
                        [255, 255, 255, (alpha * 255 / 256) as u8],
                        BUDGET,
                    )?;
                    canvas.draw(&white, self.picture_origin);
                }
            }
        }
        canvas.draw(&self.frame.image, self.frame.position);
        let scroll = ((t - 6000) / (120_000 / self.scroll_limit).max(1)).min(self.scroll_limit);
        for sprite in &self.text {
            canvas.draw(
                &sprite.image,
                [sprite.position[0], sprite.position[1] - scroll],
            );
        }
        if t < 10_000 {
            draw_alpha(
                &mut canvas,
                &self.base,
                self.base.position,
                (10_000 - t) / 16,
            )?;
        }
        let tint = if t < 4000 {
            Some((255, 256 - t / 8))
        } else if t > 126_000 {
            Some((0, 256 - (130_000 - t) / 16))
        } else {
            None
        };
        if let Some((value, alpha)) = tint {
            let fill = RgbaImage::filled(
                1280,
                720,
                [value, value, value, (alpha.clamp(0, 256) * 255 / 256) as u8],
                BUDGET,
            )?;
            canvas.draw(&fill, [0, 0]);
        }
        Ok(canvas)
    }
}

struct PanningPhoto {
    pixels: RgbaImage,
    origin: i32,
    travel: i32,
}

struct PanningCredits {
    base: Sprite,
    frame: Sprite,
    logo: Sprite,
    photos: Vec<PanningPhoto>,
    text: Vec<Sprite>,
    scroll_limit: i32,
}

pub(super) struct Credits {
    layout: Layout,
}

enum Layout {
    Classic(ClassicCredits),
    Panning(PanningCredits),
}

fn credit_text<R: Read + Seek>(
    store: &mut DatGame<R>,
    budget: &mut usize,
) -> Result<(Vec<Sprite>, i32)> {
    let mut names = sprites(store, "staff_roll_text.mng", budget)?;
    let mut text = Vec::new();
    let mut y = 720i32;
    for i in 0..7 {
        let mut sprite = take(&mut names, format!("{i:02}").as_bytes())?;
        y += sprite.position[1];
        sprite.position = [sprite.position[0] + 700, y];
        y += i32::try_from(sprite.image.height())?;
        text.push(sprite);
    }
    Ok((text, y - 400))
}

impl Credits {
    pub(super) fn load<R: Read + Seek>(store: &mut DatGame<R>) -> Result<Self> {
        let layout = if store.has_resource("system", "staff_roll_image.mng")? {
            Layout::Classic(ClassicCredits::load(store)?)
        } else {
            let table = store.read("system", "staff_roll.txt")?;
            Layout::Panning(PanningCredits::load(store, &crate::dat::decode(&table)?)?)
        };
        Ok(Self { layout })
    }

    pub(super) fn duration(&self) -> u64 {
        match self.layout {
            Layout::Classic(_) => CLASSIC_DURATION,
            Layout::Panning(_) => PANNING_DURATION,
        }
    }

    pub(super) fn frame(&self, elapsed_ms: u64) -> Result<RgbaImage> {
        match &self.layout {
            Layout::Classic(layout) => layout.frame(elapsed_ms),
            Layout::Panning(layout) => layout.frame(elapsed_ms),
        }
    }
}

impl PanningCredits {
    fn load<R: Read + Seek>(store: &mut DatGame<R>, table: &str) -> Result<Self> {
        let mut budget = 128 * 1024 * 1024;
        let mut layout = sprites(store, "staff_roll.mng", &mut budget)?;
        let base = take(&mut layout, b"BASE")?;
        let frame = take(&mut layout, b"FRAME")?;
        let logo = take(&mut layout, b"LOGO")?;
        let (text, scroll_limit) = credit_text(store, &mut budget)?;
        let mut photos = Vec::new();
        for row in table
            .lines()
            .map(str::trim)
            .filter(|row| !row.is_empty() && !row.starts_with(';'))
        {
            let fields = row.split_whitespace().collect::<Vec<_>>();
            let [name, origin, travel] = fields.as_slice() else {
                return Err("invalid panning credits row".into());
            };
            let bytes = store.read("image/visual", &format!("{name}.png"))?;
            let pixels = nekopack_image::decode_png(&bytes, budget)?;
            budget -= pixels.pixels().len();
            photos.push(PanningPhoto {
                pixels,
                origin: origin.parse()?,
                travel: travel.parse()?,
            });
        }
        if photos.is_empty() {
            return Err("credits contain no photos".into());
        }
        Ok(Self {
            base,
            frame,
            logo,
            photos,
            text,
            scroll_limit,
        })
    }

    fn frame(&self, elapsed_ms: u64) -> Result<RgbaImage> {
        let t = elapsed_ms.min(PANNING_DURATION) as i32;
        let mut canvas = RgbaImage::filled(1280, 720, [0, 0, 0, 255], BUDGET)?;
        let fill = |canvas: &mut RgbaImage, value, alpha: i32| -> Result<()> {
            let tint = RgbaImage::filled(
                1280,
                720,
                [value, value, value, alpha.clamp(0, 255) as u8],
                BUDGET,
            )?;
            canvas.draw(&tint, [0, 0]);
            Ok(())
        };
        canvas.draw(&self.base.image, self.base.position);
        let photo_time = t - 24_000;
        let span = 206_000 / self.photos.len() as i32;
        if photo_time >= 0 {
            let index = (photo_time / span) as usize;
            let photo = &self.photos[index.min(self.photos.len() - 1)];
            let offset = if index < self.photos.len() {
                photo_time % span
            } else {
                span
            };
            canvas.draw(
                &photo.pixels,
                [photo.travel * offset / span - photo.origin, 0],
            );
            let alpha = 256 - offset / 8;
            if alpha > 0 {
                fill(&mut canvas, 255, alpha)?;
            }
        }
        canvas.draw(&self.frame.image, self.frame.position);
        let scroll = (photo_time / (206_000 / self.scroll_limit).max(1)).min(self.scroll_limit);
        for sprite in &self.text {
            canvas.draw(
                &sprite.image,
                [sprite.position[0], sprite.position[1] - scroll],
            );
        }
        if t > 235_000 {
            fill(&mut canvas, 0, 256 - (280_000 - t) / 16)?;
        }
        if t < 26_000 {
            draw_alpha(
                &mut canvas,
                &self.base,
                self.base.position,
                (26_000 - t) / 16,
            )?;
            if t < 4000 {
                fill(&mut canvas, 255, 256 - t / 8)?;
            }
            let logo_alpha = if t < 16_000 {
                (t - 6000) / 32
            } else {
                (20_000 - t) / 16
            };
            draw_alpha(&mut canvas, &self.logo, self.logo.position, logo_alpha)?;
        }
        Ok(canvas)
    }
}
