use super::{StorySession, PIXELS};
use crate::scene::{Layer, Order};
use nekopack_image::{Metadata, Mng, RgbaImage};
use std::{
    error::Error,
    io::{Read, Seek},
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

#[derive(Clone, Default)]
pub(crate) struct Actor {
    pub containers: Vec<String>,
    pub expression: String,
    pub extras: Vec<(String, String)>,
    pub anchor: Option<[i32; 2]>,
}

fn matches(title: &[u8], pattern: &[u8]) -> bool {
    match pattern.split_first() {
        None => title.is_empty(),
        Some((&b'*', rest)) => (0..=title.len()).any(|n| matches(&title[n..], rest)),
        Some((&head, rest)) => title
            .split_first()
            .is_some_and(|(&ch, tail)| (head == b'?' || head == ch) && matches(tail, rest)),
    }
}

fn part<R: Read + Seek>(
    store: &mut crate::dat::DatGame<R>,
    container: &str,
    expression: &str,
) -> Result<Option<(RgbaImage, [i32; 2])>> {
    let bytes = store.read("image/actor", &format!("{container}.mng"))?;
    let mng = Mng::parse(&bytes, 100000)?;
    let mut found = None;
    for sprite in mng.sprites()? {
        let title = crate::dat::decode(sprite.name)?;
        if !expression
            .split(':')
            .any(|pattern| matches(title.as_bytes(), pattern.as_bytes()))
        {
            continue;
        }
        if found.is_some() {
            return Err(format!("ambiguous actor expression {container}/{expression}").into());
        }
        let mut pixels = mng.decode_image(sprite.ordinal, PIXELS)?;
        if sprite.ordinal != 0
            && mng.chunks.iter().any(|c| {
                matches!(
                    c.metadata,
                    Metadata::Termination {
                        repeat: Some((_, _, 9999)),
                        ..
                    }
                )
            })
        {
            let base = mng.decode_image(0, PIXELS)?;
            if (pixels.width(), pixels.height()) == (base.width(), base.height()) {
                for (sample, reference) in pixels.pixels_mut().iter_mut().zip(base.pixels()) {
                    *sample = sample.wrapping_add(*reference);
                }
            }
        }
        found = Some((
            pixels,
            [
                sprite.position[0] - i32::try_from(mng.header[0] / 2)?,
                sprite.position[1] - i32::try_from(mng.header[1])?,
            ],
        ));
    }
    Ok(found)
}

impl<R: Read + Seek> StorySession<R> {
    pub(super) fn mapped_actor(&mut self, args: &[Vec<u8>]) -> Result<()> {
        self.system.eval_with_resources(
            "actor-macro",
            &format!("Host.actor_action <- \"\"\ncom[\"@\"].call(getroottable(),{})\nreturn Host.actor_action", crate::array(args)?),
            &mut self.machine.variables,
            &mut self.store,
        )?;
        let expanded = self.system.result().to_owned();
        let mut words = expanded.split_whitespace();
        let Some(slot) = words.next() else {
            return Ok(());
        };
        let slot: u8 = slot.parse()?;
        let Some(first) = words.next() else {
            self.scene.actors.retain(|actor| actor.slot != slot);
            self.motions.retain(|motion| motion.slot != slot);
            return Ok(());
        };
        // The script-owned background map emits ImageAction("1 "+resource).
        // Actor definitions emit #container compositions in slots starting at 2.
        if slot == 1 && !first.starts_with('#') {
            if words.next().is_some() {
                return Err("unsupported mapped background modifiers".into());
            }
            let bytes = self.store.read("image/back", &format!("{first}.png"))?;
            self.scene
                .set_background(first.into(), nekopack_image::decode_png(&bytes, PIXELS)?)?;
            return Ok(());
        }
        let state = self.scene.mapped_actors.entry(slot).or_default();
        let center = i32::try_from(self.scene.dimensions[0] / 2)?;
        let mut anchor = state
            .anchor
            .unwrap_or([center, i32::try_from(self.scene.dimensions[1])? + 100]);
        let mut order = Order::Front;
        for word in std::iter::once(first).chain(words) {
            if let Some(books) = word.strip_prefix('#') {
                if let Some((container, parts)) = books.split_once('&') {
                    let mut parts = parts.split('&');
                    state.containers = vec![container.into()];
                    state.expression = parts
                        .next()
                        .ok_or("actor composition requires a body")?
                        .into();
                    state.extras = parts.map(|part| (container.into(), part.into())).collect();
                } else {
                    state.containers = books.split('#').map(str::to_owned).collect();
                }
            } else if let Some(extra) = word.strip_prefix('&') {
                let (book, name) = extra.split_once('#').ok_or("invalid actor attachment")?;
                state.extras.push((book.into(), name.into()));
            } else if word.starts_with("SG") || word.starts_with("VG") {
                state.expression = word.into();
                state.extras.clear();
            } else if let Some(tail) = word
                .strip_prefix("left")
                .or_else(|| word.strip_prefix("right"))
            {
                let distance: i32 = if tail.is_empty() { 0 } else { tail.parse()? };
                anchor[0] = center + if word.starts_with("left") { -160 } else { 160 } * distance;
            } else {
                match word {
                    "center" => anchor[0] = center,
                    "front" => order = Order::Front,
                    "back" => order = Order::Back,
                    "keep" => order = Order::Keep,
                    "to" => {}
                    _ if word.starts_with("slow") || word.starts_with("fast") => {}
                    _ => {
                        let mut terms = word.as_bytes().iter().copied().peekable();
                        while let Some(sign) = terms.next() {
                            let mut magnitude = 0i32;
                            while let Some(digit) = terms.next_if(u8::is_ascii_digit) {
                                magnitude = magnitude
                                    .checked_mul(10)
                                    .and_then(|n| n.checked_add(i32::from(digit - b'0')))
                                    .ok_or("actor motion overflow")?;
                            }
                            let distance = magnitude
                                .max(1)
                                .checked_mul(50)
                                .ok_or("actor motion overflow")?;
                            match sign {
                                b'<' => anchor[0] -= distance,
                                b'>' => anchor[0] += distance,
                                b'+' => anchor[1] -= distance,
                                b'-' => anchor[1] += distance,
                                b'[' | b']' | b'^' | b'_' | b'$' | b'@' | b'!' => {}
                                _ => return Err(format!("unsupported actor motion {word}").into()),
                            }
                        }
                    }
                }
            }
        }
        state.anchor = Some(anchor);
        if state.containers.is_empty() || state.expression.is_empty() {
            return Ok(());
        }
        let mut layers = Vec::new();
        for book in &state.containers {
            if let Some(image) = part(&mut self.store, book, &state.expression)? {
                layers.push(image);
            }
        }
        if layers.len() != 1 {
            return Err(format!(
                "actor expression {} has {} body layers",
                state.expression,
                layers.len()
            )
            .into());
        }
        for (book, name) in &state.extras {
            layers.push(part(&mut self.store, book, name)?.ok_or("missing actor attachment")?);
        }
        let x = layers.iter().map(|(_, p)| p[0]).min().unwrap();
        let y = layers.iter().map(|(_, p)| p[1]).min().unwrap();
        let right = layers
            .iter()
            .map(|(i, p)| p[0] + i.width() as i32)
            .max()
            .unwrap();
        let bottom = layers
            .iter()
            .map(|(i, p)| p[1] + i.height() as i32)
            .max()
            .unwrap();
        let mut pixels = RgbaImage::transparent((right - x) as u32, (bottom - y) as u32, PIXELS)?;
        for (image, [px, py]) in layers {
            pixels.draw(&image, [px - x, py - y]);
        }
        let name = state.expression.clone();
        self.scene.put(
            Layer {
                slot,
                name,
                folder: "image/actor".into(),
                image: pixels,
                offset: [x, y],
                anchor,
                filter_pending: true,
                filter: crate::filter::Filter::None,
            },
            order,
        )?;
        Ok(())
    }
}
