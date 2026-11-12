use super::{BUDGET, Result, contains, update_pointer};
use crate::{dat::DatGame, message_log, quote};
use nekopack_image::{Mng, RgbaImage, decode_png};
use nekopack_script::{runtime::Variables, system::SystemVm};
use std::{
    collections::BTreeMap,
    io::{Read, Seek},
};

// Room entries use 1-based IDs; the host cancel event must not share that range.
const CANCEL_EVENT: i32 = 1999;

pub(super) fn destinations(
    source: &[u8],
    map_line: usize,
) -> Result<(BTreeMap<i32, String>, Option<i32>)> {
    let source = std::str::from_utf8(source)?;
    let mut result = BTreeMap::new();
    let mut case = None;
    let mut quit = None;
    let mut nested = 0;
    for line in source.lines().skip(map_line) {
        let mut words = line.split_whitespace();
        match words.next() {
            Some(".end") if nested == 0 => break,
            Some(".end") => nested -= 1,
            Some(".map" | ".select") => nested += 1,
            Some(".case") if nested == 0 => case = words.next().map(str::parse).transpose()?,
            Some(".quit") if nested == 0 => quit = case,
            Some(".room") if nested == 0 => {
                if let (Some(id), Some(room)) = (case, words.next()) {
                    result.insert(id, room.to_owned());
                }
            }
            _ => {}
        }
    }
    Ok((result, quit))
}

struct Button {
    id: i32,
    group: i32,
    enabled: bool,
    position: [i32; 2],
    size: [u32; 2],
}
pub(super) struct Room {
    size: [u32; 2],
    buttons: Vec<Button>,
    visual: bool,
    pressed: Option<i32>,
    assets: BTreeMap<String, RgbaImage>,
    retained: usize,
}
pub(super) enum Frame {
    Packet(Vec<u8>),
    Replay(String),
    Closed,
}
pub(super) struct Outcome {
    pub frame: Frame,
    pub sounds: Vec<message_log::Cue>,
    pub stop_music: bool,
}

impl Outcome {
    fn silent(frame: Frame) -> Self {
        Self {
            frame,
            sounds: Vec::new(),
            stop_music: false,
        }
    }
}

impl Room {
    pub(super) fn open<R: Read + Seek>(
        system: &mut SystemVm,
        vars: &mut Variables,
        store: &mut DatGame<R>,
        name: &str,
        size: [u32; 2],
    ) -> Result<(Self, Outcome)> {
        system.eval("room-host", include_str!("../../host/room.nut"))?;
        system.eval_with_resources(
            "open-room",
            &format!(
                "RoomInstall({}, {CANCEL_EVENT})\nreturn RoomStep(0,-1,-1)",
                quote(name.as_bytes())?
            ),
            vars,
            store,
        )?;
        let mut room = Self {
            size,
            buttons: Vec::new(),
            visual: false,
            pressed: None,
            assets: BTreeMap::new(),
            retained: 0,
        };
        let frame = room.outcome(system, store)?;
        Ok((room, frame))
    }
    pub(super) fn input<R: Read + Seek>(
        &mut self,
        system: &mut SystemVm,
        vars: &mut Variables,
        store: &mut DatGame<R>,
        kind: i32,
        point: [i32; 2],
    ) -> Result<Outcome> {
        let (event, group, id) = if kind == 4 {
            self.pressed = None;
            (CANCEL_EVENT, -1, -1)
        } else {
            let hit = if self.visual {
                contains([0, 0], self.size, point).then_some(-1)
            } else {
                self.buttons
                    .iter()
                    .position(|b| b.enabled && contains(b.position, b.size, point))
                    .map(|n| n as i32)
            };
            let before = self.pressed;
            let clicked = update_pointer(&mut self.pressed, kind, hit)?;
            if clicked.is_none() && self.pressed == before || self.visual && clicked.is_none() {
                return Ok(Outcome::silent(Frame::Packet(
                    br#"{"kind":"room_unchanged"}"#.to_vec(),
                )));
            }
            let event = i32::from(clicked.is_some());
            match clicked.or(self.pressed) {
                Some(index) if index >= 0 => {
                    let button = &self.buttons[index as usize];
                    (event, button.group, button.id)
                }
                _ => (event, -1, -1),
            }
        };
        system.eval_with_resources(
            "room-input",
            &format!("return RoomStep({event},{group},{id})"),
            vars,
            store,
        )?;
        self.outcome(system, store)
    }

    fn outcome<R: Read + Seek>(
        &mut self,
        system: &mut SystemVm,
        store: &mut DatGame<R>,
    ) -> Result<Outcome> {
        let plan = system.result();
        let mut body = plan.as_str();
        let mut sound_ids = Vec::new();
        let mut stop_music = false;
        while let Some((line, rest)) = body.split_once('\n') {
            if line == "M" {
                stop_music = true;
            } else if let Some(name) = line.strip_prefix("S\t") {
                let id = match name {
                    "PAGE" => 1,
                    "CLICK" => 2,
                    "OK" => 3,
                    "SELECT" => 4,
                    _ => return Err(format!("unknown room sound {name}").into()),
                };
                if let Some(id) = message_log::available_ui_sound(store, id)? {
                    sound_ids.push(id);
                }
            } else {
                break;
            }
            body = rest;
        }
        let mut outcome = Outcome::silent(self.frame(store, body)?);
        outcome.stop_music = stop_music;
        if !sound_ids.is_empty() {
            let volume = message_log::audio_volume(system, "VolSYS")? as i32;
            outcome.sounds = sound_ids.into_iter().map(|id| (id, volume)).collect();
        }
        Ok(outcome)
    }

    fn frame<R: Read + Seek>(&mut self, store: &mut DatGame<R>, plan: &str) -> Result<Frame> {
        if plan == "closed" {
            return Ok(Frame::Closed);
        }
        if let Some(name) = plan.strip_prefix("replay\t") {
            return Ok(Frame::Replay(name.to_owned()));
        }
        self.cache_sprites(store, plan)?;
        let mut lines = plan.lines();
        let mut header = lines.next().ok_or("empty room plan")?.split('\t');
        if header.next() != Some("frame") {
            return Err("invalid room plan".into());
        }
        let room = header.next().ok_or("missing room name")?;
        self.visual = header.next() == Some("1");
        self.buttons.clear();
        let [width, height] = self.size;
        let canvas = RgbaImage::filled(width, height, [0, 0, 0, 255], BUDGET)?;
        let mut layers = vec![(canvas, [0, 0])];
        let mut layer_budget = BUDGET - layers[0].0.pixels().len();
        for line in lines {
            let fields: Vec<_> = line.split('\t').collect();
            match fields.as_slice() {
                ["B", group, id, enabled, x, y, w, h] => self.buttons.push(Button {
                    group: group.parse()?,
                    id: id.parse()?,
                    enabled: *enabled == "1",
                    position: [x.parse()?, y.parse()?],
                    size: [w.parse()?, h.parse()?],
                }),
                ["A", asset, x, y] => {
                    let position = [x.parse()?, y.parse()?];
                    let layer = &mut layers.last_mut().ok_or("no room layer")?.0;
                    if asset.starts_with("image/visual/") {
                        // Full-screen CGs change independently from the room's reusable sprites.
                        layer.draw(&load_asset(store, asset, BUDGET)?, position);
                    } else {
                        layer.draw(&self.assets[*asset], position);
                    }
                }
                ["L", w, h, x, y] => {
                    let layer = RgbaImage::transparent(w.parse()?, h.parse()?, layer_budget)?;
                    layer_budget -= layer.pixels().len();
                    layers.push((layer, [x.parse()?, y.parse()?]));
                }
                ["F", color, alpha] => {
                    let color: u32 = color.parse()?;
                    let layer = &mut layers.last_mut().ok_or("no room layer")?.0;
                    let fill = RgbaImage::filled(
                        layer.width(),
                        layer.height(),
                        [
                            (color >> 16) as u8,
                            (color >> 8) as u8,
                            color as u8,
                            alpha.parse()?,
                        ],
                        layer_budget,
                    )?;
                    layer.draw(&fill, [0, 0]);
                }
                ["E"] if layers.len() > 1 => {
                    let (layer, position) = layers.pop().unwrap();
                    layer_budget += layer.pixels().len();
                    layers.last_mut().unwrap().0.draw(&layer, position);
                }
                _ => return Err("invalid room draw command".into()),
            }
        }
        if layers.len() != 1 {
            return Err("unclosed room layer".into());
        }
        let buttons = self
            .buttons
            .iter()
            .map(|b| {
                let [x, y] = b.position;
                let [w, h] = b.size;
                format!(
                    r#"{{"group":{},"id":{},"enabled":{},"bounds":[{x},{y},{w},{h}]}}"#,
                    b.group, b.id, b.enabled
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let canvas = layers.pop().unwrap().0;
        let digest = nekopack_script::checkpoint::digest(canvas.pixels());
        let mut packet = format!(
            concat!(r#"{{"kind":"room","room":{},"visual":{},"width":{},"height":{},"digest":"{:016x}","buttons":[{}]}}"#, "\n"),
            quote(room.as_bytes())?, self.visual, width, height, digest, buttons,
        ).into_bytes();
        canvas.write_png(&mut packet)?;
        Ok(Frame::Packet(packet))
    }
    fn cache_sprites<R: Read + Seek>(&mut self, store: &mut DatGame<R>, plan: &str) -> Result<()> {
        self.assets.retain(|key, pixels| {
            if key.starts_with("image/actor/")
                && !plan
                    .lines()
                    .any(|line| line.starts_with(&format!("A\t{key}\t")))
            {
                self.retained -= pixels.pixels().len();
                false
            } else {
                true
            }
        });
        let mut packs = BTreeMap::<&str, Vec<(&str, usize)>>::new();
        for asset in plan
            .lines()
            .filter_map(|line| line.strip_prefix("A\t"))
            .filter_map(|line| line.split('\t').next())
        {
            if self.assets.contains_key(asset) || asset.starts_with("image/visual/") {
                continue;
            }
            if let Some((path, ordinal)) = asset.rsplit_once(':') {
                packs
                    .entry(path)
                    .or_default()
                    .push((asset, ordinal.parse()?));
            } else {
                let pixels = load_asset(store, asset, BUDGET - self.retained)?;
                self.retained += pixels.pixels().len();
                self.assets.insert(asset.to_owned(), pixels);
            }
        }
        for (path, sprites) in packs {
            let (dir, file) = path.rsplit_once('/').ok_or("invalid room asset")?;
            let bytes = store.read(dir, file)?;
            let pack = Mng::parse(&bytes, 100_000)?;
            for (asset, ordinal) in sprites {
                if self.assets.contains_key(asset) {
                    continue;
                }
                let pixels = pack.decode_image(ordinal, BUDGET - self.retained)?;
                self.retained += pixels.pixels().len();
                self.assets.insert(asset.to_owned(), pixels);
            }
        }
        Ok(())
    }
}

fn load_asset<R: Read + Seek>(
    store: &mut DatGame<R>,
    asset: &str,
    budget: usize,
) -> Result<RgbaImage> {
    let (dir, file) = asset.rsplit_once('/').ok_or("invalid room asset")?;
    Ok(decode_png(&store.read(dir, file)?, budget)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::title::TitleSession;

    #[test]
    #[ignore = "requires an original game and a route ledger supplied by the device verification"]
    fn restored_gallery_progress_opens_original_rooms() {
        let paths = std::env::var_os("NEKO_ROOM_TEST_DAT").unwrap();
        let progress_path = std::env::var_os("NEKO_ROOM_TEST_PROGRESS").unwrap();
        let progress = std::fs::read(progress_path).unwrap();
        let mount = || {
            DatGame::mount(std::env::split_paths(&paths).map(|p| std::fs::File::open(p).unwrap()))
                .unwrap()
        };
        for name in ["cg", "scene"] {
            let mut title = TitleSession::open_with_progress(mount(), false, &progress).unwrap();
            let id = *title
                .room_destinations
                .iter()
                .find(|(_, room)| *room == name)
                .unwrap()
                .0;
            let packet = title.open_room(id as u64).unwrap();
            let header =
                std::str::from_utf8(packet.split(|&b| b == b'\n').next().unwrap()).unwrap();
            assert!(header.contains(&format!(r#""room":"{name}""#)));
            let buttons = &title.room.as_ref().unwrap().buttons;
            assert!(buttons.iter().any(|button| {
                button.enabled
                    && (1..=100).contains(&button.id)
                    && buttons
                        .iter()
                        .filter(|peer| peer.group == button.group)
                        .count()
                        > 5
            }));
            title.room_input(4, 0, 0).unwrap();
        }
    }

    #[test]
    #[ignore = "requires original game DAT files supplied by NEKO_ROOM_TEST_DAT"]
    fn original_room_scripts_accept_input() {
        let paths = std::env::var_os("NEKO_ROOM_TEST_DAT").expect("DAT paths");
        let paths: Vec<_> = std::env::split_paths(&paths).collect();
        let mount =
            || DatGame::mount(paths.iter().map(|path| std::fs::File::open(path).unwrap())).unwrap();
        let mut title = TitleSession::open_with_clear(mount(), true).unwrap();
        let has_gallery_flags = [(b'F', 300), (b'G', 200)].into_iter().all(|key| {
            title
                .machine
                .variables
                .unlock_slots()
                .any(|slot| slot == key)
        });
        let first_entry_enabled = |room: &Room| {
            room.buttons.iter().any(|button| {
                button.id == 1
                    && button.enabled
                    && room
                        .buttons
                        .iter()
                        .filter(|peer| peer.group == button.group)
                        .count()
                        > 5
            })
        };
        for name in ["cg", "scene", "actor"] {
            let id = title
                .room_destinations
                .iter()
                .find(|(_, value)| *value == name)
                .unwrap()
                .0;
            let packet = title
                .open_room(*id as u64)
                .unwrap_or_else(|error| panic!("{name}: {error}"));
            let header =
                std::str::from_utf8(packet.split(|&b| b == b'\n').next().unwrap()).unwrap();
            assert!(header.contains(&format!(r#""room":"{name}""#)), "{header}");
            if name != "actor" {
                assert!(
                    header.contains(r#""system_sounds":[{"id":1,"category_percent":"#),
                    "{header}"
                );
            }
            let room = title.room.as_ref().unwrap();
            assert!(room.buttons.iter().any(|button| button.enabled));
            if has_gallery_flags && name != "actor" {
                assert!(!first_entry_enabled(room));
            }
            if name == "cg" {
                let entry = room
                    .buttons
                    .iter()
                    .find(|button| {
                        button.id == 2
                            && button.enabled
                            && room
                                .buttons
                                .iter()
                                .filter(|other| other.group == button.group)
                                .count()
                                > 5
                    })
                    .map(|button| {
                        [
                            button.position[0] + button.size[0] as i32 / 2,
                            button.position[1] + button.size[1] as i32 / 2,
                        ]
                    });
                if let Some([x, y]) = entry {
                    title.room_input(0, x, y).unwrap();
                    let image = title.room_input(2, x, y).unwrap();
                    let header =
                        std::str::from_utf8(image.split(|&b| b == b'\n').next().unwrap()).unwrap();
                    assert!(header.contains(r#""visual":true"#), "{header}");
                    assert!(
                        header.contains(r#""system_sounds":[{"id":2,"category_percent":"#),
                        "{header}"
                    );
                    title.room_input(4, 0, 0).unwrap();
                }
                let buttons = &title.room.as_ref().unwrap().buttons;
                let page = buttons
                    .iter()
                    .find(|button| {
                        button.id == 2
                            && button.enabled
                            && (4..=5).contains(
                                &buttons
                                    .iter()
                                    .filter(|peer| peer.group == button.group)
                                    .count(),
                            )
                    })
                    .unwrap();
                let [x, y] = [
                    page.position[0] + page.size[0] as i32 / 2,
                    page.position[1] + page.size[1] as i32 / 2,
                ];
                for (event, sound) in [(0, 4), (2, 3)] {
                    let file = format!("{}.ogg", message_log::ui_sound_name(sound).unwrap());
                    let expected = if title.store.has_resource("sound/se", &file).unwrap() {
                        sound
                    } else {
                        2
                    };
                    let packet = title.room_input(event, x, y).unwrap();
                    let header =
                        std::str::from_utf8(packet.split(|&b| b == b'\n').next().unwrap()).unwrap();
                    assert!(
                        header.contains(&format!(r#""id":{expected},"category_percent""#)),
                        "{header}"
                    );
                }
            }
            if name == "actor" {
                let room = title.room.as_ref().unwrap();
                assert!(
                    room.assets
                        .keys()
                        .any(|path| path.starts_with("image/actor/"))
                );
            }
            if name == "cg" {
                let button = title
                    .room
                    .as_ref()
                    .unwrap()
                    .buttons
                    .iter()
                    .find(|button| button.id == 1003 && button.enabled)
                    .unwrap();
                let x = button.position[0] + button.size[0] as i32 / 2;
                let y = button.position[1] + button.size[1] as i32 / 2;
                title.room_input(0, x, y).unwrap();
                let scene = title.room_input(2, x, y).unwrap();
                let header =
                    std::str::from_utf8(scene.split(|&b| b == b'\n').next().unwrap()).unwrap();
                assert!(header.contains(r#""room":"scene""#), "{header}");
            }
            let close = String::from_utf8(title.room_input(4, 0, 0).unwrap()).unwrap();
            assert!(close.contains(r#""kind":"room_closed""#), "{close}");
        }
        if has_gallery_flags {
            for (bank, index) in [(b'F', 300), (b'G', 200)] {
                title.machine.variables.restore_unlock(bank, index);
            }
            let mut ledger = crate::unlocks::Unlocks::default();
            ledger.capture(&title.machine.variables);
            let mut restored =
                TitleSession::open_with_progress(mount(), true, &ledger.encode()).unwrap();
            for name in ["cg", "scene"] {
                let id = restored
                    .room_destinations
                    .iter()
                    .find(|(_, value)| *value == name)
                    .unwrap()
                    .0;
                restored.open_room(*id as u64).unwrap();
                assert!(
                    first_entry_enabled(restored.room.as_ref().unwrap()),
                    "{name}"
                );
                restored.room_input(4, 0, 0).unwrap();
            }
        }
    }
}
