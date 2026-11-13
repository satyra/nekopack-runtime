//! Interactive presentation of the observed normal-map title profile.
//! The scenario remains pending until Start; the same machine/VM enters the story.
mod native;
mod room;
use room::{Frame as RoomFrame, Room};

use crate::{array, bootstrap_begin, bootstrap_continue, dat::DatGame, quote, story::StorySession};
use nekopack_image::{Mng, RgbaImage};
use nekopack_script::{
    runtime::{Machine, Request, Scripts, Step},
    system::SystemVm,
};
use std::{
    collections::{BTreeMap, VecDeque},
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
    id: i32,
    enabled: bool,
    normal: Option<usize>,
    hover: Option<usize>,
    area: usize,
}
pub struct TitleSession<R: Read + Seek> {
    store: DatGame<R>,
    machine: Machine,
    system: SystemVm,
    size: [u32; 2],
    images: BTreeMap<usize, Image>,
    bases: Vec<usize>,
    buttons: Vec<Button>,
    pressed: Option<i32>,
    room: Option<Room>,
    room_destinations: BTreeMap<i32, String>,
    quit_action: Option<i32>,
    settings_action: Option<i32>,
    start_action: i32,
    start_actions: std::collections::BTreeSet<i32>,
    load_action: Option<i32>,
    continue_action: Option<i32>,
    native_title: Option<native::NativeTitle>,
    room_replay: Option<String>,
    room_music: VecDeque<room::MusicCommand>,
    sound_cues: Vec<crate::message_log::Cue>,
    input_sequence: u64,
    settings: Option<crate::settings::Settings>,
    save_ui: Option<crate::save_ui::SaveUi>,
    prepared_save: Option<crate::story::saves::Saved>,
    preview_font: Option<crate::text_render::SharedFont>,
}
impl<R: Read + Seek> TitleSession<R> {
    pub fn ui_sound(&mut self, id: u64, volume: i32) -> Result<Vec<u8>> {
        crate::message_log::sound_at(&mut self.store, id, volume)
    }
    pub fn star_sound(&mut self, action: u32) -> Result<Vec<u8>> {
        crate::message_log::audio(
            &mut self.store,
            &mut self.system,
            crate::message_log::Audio::Save(action),
        )
    }
    pub fn open_save_ui(&mut self) -> Result<Vec<u8>> {
        if self.settings.is_some() {
            return Err("title settings are open".into());
        }
        self.save_ui = Some(crate::save_ui::SaveUi::open(&mut self.store, false, true)?);
        self.save_ui_packet(0, true)
    }
    pub fn save_ui_pointer(&mut self, kind: i32, x: i32, y: i32) -> Result<Vec<u8>> {
        let (action, refresh) = self
            .save_ui
            .as_mut()
            .ok_or("save UI is not open")?
            .pointer(kind, x, y)?;
        self.save_ui_packet(action, refresh)
    }
    pub fn save_ui_packet(&self, action: i32, refresh: bool) -> Result<Vec<u8>> {
        self.save_ui.as_ref().ok_or("save UI is not open")?.packet(
            self.preview_font
                .as_ref()
                .ok_or("save UI font not installed")?,
            action,
            refresh,
        )
    }
    pub fn save_ui_slot(&mut self, id: usize, bytes: Option<&[u8]>, stamp: String) -> Result<()> {
        let value = bytes
            .map(|b| {
                crate::story::saves::Saved::decode(b, &mut self.store, self.size, false)
                    .and_then(|s| s.preview(stamp))
            })
            .transpose()
            .map_err(|e| e.to_string());
        self.save_ui
            .as_mut()
            .ok_or("save UI is not open")?
            .set_slot(id, value)
    }
    pub fn prepare_save(&mut self, bytes: &[u8]) -> Result<Vec<u8>> {
        if self.save_ui.is_none() {
            return Err("title load UI is not open".into());
        }
        self.prepared_save = None;
        self.prepared_save = Some(crate::story::saves::Saved::decode(
            bytes,
            &mut self.store,
            self.size,
            true,
        )?);
        Ok(b"{\"prepared\":true}".to_vec())
    }
    pub fn validate_resume(&mut self, bytes: &[u8]) -> Result<()> {
        crate::story::saves::Saved::decode(bytes, &mut self.store, self.size, true)?;
        Ok(())
    }
    pub fn cancel_save(&mut self) -> Result<Vec<u8>> {
        self.save_ui = None;
        self.prepared_save = None;
        self.packet(None)
    }
    pub fn load(self) -> Result<StorySession<R>> {
        let saved = self.prepared_save.ok_or("no prepared title save")?;
        let mut story = StorySession::from_bootstrap(self.store, self.machine, self.system)?;
        story.enable_host_effects();
        story.enable_interactive_text();
        story.apply_saved(saved)?;
        Ok(story)
    }
    pub fn open(store: DatGame<R>) -> Result<Self> {
        Self::open_with_clear(store, false)
    }
    pub fn open_with_clear(store: DatGame<R>, cleared: bool) -> Result<Self> {
        Self::open_with_progress(store, cleared, &[])
    }
    pub fn open_with_progress(
        mut store: DatGame<R>,
        cleared: bool,
        progress: &[u8],
    ) -> Result<Self> {
        let (mut machine, mut system) = bootstrap_begin(&mut store, 1, &mut std::io::sink(), true)?;
        let Step::Request(Request::Command { name, args, rows }) = machine.advance(&mut store)?
        else {
            return Err("title requires pending map".into());
        };
        let native_title = name == b"title" && !rows.is_empty();
        if !native_title && (name != b"map" || args.first().is_none_or(|arg| arg != b"title")) {
            return Err(format!("unsupported title map profile: {:?} {:?}", name, args).into());
        }
        if cleared {
            let flag = if machine.variables.get("全クリア".as_bytes()).is_ok() {
                "全クリア"
            } else {
                "クリア"
            };
            machine.variables.set(flag.as_bytes(), 1)?;
        }
        if !progress.is_empty() {
            crate::unlocks::Unlocks::decode(progress)?.apply(&mut machine.variables);
        }
        let (source, map_line) = machine.location().ok_or("title location missing")?;
        let title_source = store.load(source)?.bytes;
        let destinations = room::destinations(&title_source, map_line)?;
        let native_title = if native_title {
            Some(native::NativeTitle::open(
                &mut store,
                &mut system,
                &mut machine.variables,
                args.clone(),
                rows.clone(),
            )?)
        } else {
            None
        };
        let plan = if native_title.is_none() {
            system
                .eval_with_resources(
                    "title-plan",
                    &format!(
                        "TitleName <- {}\nTitleRows <- {}\n{}",
                        quote(&args[0])?,
                        array(&rows)?,
                        include_str!("../host/title.nut")
                    ),
                    &mut machine.variables,
                    &mut store,
                )
                .map_err(|error| format!("title presentation script: {error}"))?;
            system.result().to_owned()
        } else {
            String::new()
        };
        let bytes = store.read("system", "title.mng")?;
        let mng = Mng::parse(&bytes, 100000)?;
        let sprites = mng.sprites()?;
        let size = [mng.header[0], mng.header[1]];
        let canvas = RgbaImage::transparent(size[0], size[1], BUDGET)?;
        let mut retained = canvas.pixels().len();
        drop(canvas);
        let mut images = BTreeMap::new();
        let mut bases = Vec::new();
        let mut buttons = Vec::new();
        for line in plan.lines() {
            let fields: Vec<_> = line.split('\t').collect();
            let numbers = fields[1..]
                .iter()
                .map(|s| s.parse::<i32>())
                .collect::<std::result::Result<Vec<_>, _>>()?;
            let ord = |n: i32| -> Result<Option<usize>> {
                if n == -1 {
                    Ok(None)
                } else {
                    Ok(Some(usize::try_from(n)?))
                }
            };
            let needed = match (fields[0], numbers.as_slice()) {
                ("base", [i]) => {
                    let i = ord(*i)?.ok_or("missing title base")?;
                    bases.push(i);
                    vec![i]
                }
                ("button", [id, enabled, normal, hover, area]) => {
                    let area = ord(*area)?.ok_or("missing title hit area")?;
                    let normal = ord(*normal)?;
                    let hover = ord(*hover)?;
                    buttons.push(Button {
                        id: *id,
                        enabled: *enabled != 0,
                        normal,
                        hover,
                        area,
                    });
                    normal.into_iter().chain(hover).chain(Some(area)).collect()
                }
                _ => return Err("invalid title presentation plan".into()),
            };
            for i in needed {
                if images.contains_key(&i) {
                    continue;
                }
                let sprite = sprites.get(i).ok_or("invalid title sprite ordinal")?;
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
        buttons.sort_by_key(|b| b.id);
        let start_action = if let Some(view) = &native_title {
            view.buttons()?.iter().find_map(|(id, enabled, _)| {
                (destinations.starts.contains(id) && *enabled).then_some(*id)
            })
        } else {
            buttons
                .iter()
                .find_map(|b| (destinations.starts.contains(&b.id) && b.enabled).then_some(b.id))
        };
        let start_action = start_action.ok_or("title Start is unavailable")?;
        Ok(Self {
            store,
            machine,
            system,
            size,
            images,
            bases,
            buttons,
            pressed: None,
            room: None,
            room_destinations: destinations.rooms,
            quit_action: destinations.quit,
            settings_action: destinations.settings,
            start_action,
            start_actions: destinations.starts,
            load_action: destinations.load,
            continue_action: destinations.resume,
            native_title,
            room_replay: None,
            room_music: VecDeque::new(),
            sound_cues: Vec::new(),
            input_sequence: 0,
            settings: None,
            save_ui: None,
            prepared_save: None,
            preview_font: None,
        })
    }
    fn hit(&self, x: i32, y: i32) -> Option<i32> {
        self.buttons
            .iter()
            .find(|b| {
                let image = &self.images[&b.area];
                b.enabled
                    && contains(
                        image.position,
                        [image.pixels.width(), image.pixels.height()],
                        [x, y],
                    )
            })
            .map(|b| b.id)
    }
    /// Pointer events: 0 down, 1 move, 2 up, 3 cancel. Dragging out cancels activation.
    pub fn pointer(&mut self, kind: i32, x: i32, y: i32) -> Result<Option<i32>> {
        self.input_sequence += 1;
        if let Some(settings) = self.settings.as_mut() {
            self.sound_cues =
                settings.script_input(&mut self.store, &mut self.system, kind, x, y)?;
            if settings.take_close_request() {
                self.settings = None;
                self.sound_cues = crate::message_log::page_cue(&mut self.system)?;
                return Ok(None);
            }
            return Ok(None);
        }
        let action = if let Some(view) = &mut self.native_title {
            let hit = view.hit(x, y)?;
            let confirmed = update_pointer(&mut self.pressed, kind, hit)?;
            let (action, cues) = view.step(
                &mut self.store,
                &mut self.system,
                &mut self.machine.variables,
                if confirmed.is_some() { "ID_OK" } else { "0" },
                x,
                y,
                0,
            )?;
            self.sound_cues = cues;
            action
        } else {
            let hit = self.hit(x, y);
            update_pointer(&mut self.pressed, kind, hit)?
        };
        if action.is_some() && action == self.settings_action {
            self.sound_cues = crate::message_log::page_cue(&mut self.system)?;
            self.settings = Some(crate::settings::Settings::open(
                &mut self.store,
                &mut self.system,
                1,
            )?);
            return Ok(None);
        }
        Ok(action)
    }
    pub fn packet(&mut self, action: Option<i32>) -> Result<Vec<u8>> {
        self.packet_with_settings_format(action, false)
    }

    pub fn packet_with_settings_format(
        &mut self,
        action: Option<i32>,
        raw: bool,
    ) -> Result<Vec<u8>> {
        let cues = std::mem::take(&mut self.sound_cues);
        if let Some(settings) = &self.settings {
            let packet = settings.packet(
                &mut self.system,
                self.input_sequence,
                self.preview_font.as_ref(),
                raw,
            )?;
            return crate::message_log::with_available_sounds(&mut self.store, packet, &cues);
        }
        if action.is_none() {
            if let Some(view) = self.native_title.as_mut().filter(|view| view.closed) {
                view.restart(
                    &mut self.store,
                    &mut self.system,
                    &mut self.machine.variables,
                )?;
            }
        }
        let mut canvas = if let Some(view) = &self.native_title {
            view.render(self.size)?
        } else {
            RgbaImage::filled(self.size[0], self.size[1], [0, 0, 0, 255], BUDGET)?
        };
        for &i in &self.bases {
            let image = &self.images[&i];
            canvas.draw(&image.pixels, image.position)
        }
        for b in &self.buttons {
            if !b.enabled {
                continue;
            }
            let sprite = if self.pressed == Some(b.id) {
                b.hover.or(b.normal)
            } else {
                b.normal
            };
            if let Some(i) = sprite {
                let image = &self.images[&i];
                canvas.draw(&image.pixels, image.position)
            }
        }
        let buttons = if let Some(view) = &self.native_title {
            view.buttons_json()?
        } else {
            self.buttons
                .iter()
                .map(|b| {
                    let image = &self.images[&b.area];
                    format!(
                        "{{\"id\":{},\"enabled\":{},\"bounds\":[{},{},{},{}]}}",
                        b.id,
                        b.enabled,
                        image.position[0],
                        image.position[1],
                        image.pixels.width(),
                        image.pixels.height()
                    )
                })
                .collect::<Vec<_>>()
                .join(",")
        };
        let (source, line) = self.machine.location().ok_or("title location missing")?;
        let presentation = format!(
            "{},\"tick_ms\":{}",
            crate::preferences::presentation(&mut self.system)?,
            self.native_title.as_ref().map_or(0, |view| view.delay())
        );
        let mut result=format!("{{\"kind\":\"title\",\"input_sequence\":{},\"width\":{},\"height\":{},\"source\":{},\"line\":{},\"steps\":{},\"pressed\":{},\"action\":{},\"chinese_scenario\":{},\"start_action\":{},\"load_action\":{},\"continue_action\":{},\"settings_action\":{},\"quit_action\":{},\"room_action\":{},\"replay_action\":{},\"buttons\":[{}]{presentation}}}\n",self.input_sequence,self.size[0],self.size[1],quote(source)?,line,self.machine.steps(),self.pressed.unwrap_or(0),action.unwrap_or(0),
            self.store.has_chinese_scenario_patch(),
            self.start_action,
            self.load_action.unwrap_or(0),
            self.continue_action.unwrap_or(0),
            self.settings_action.unwrap_or(0),
            self.quit_action.unwrap_or(0),
            action.is_some_and(|id| self.room_destinations.contains_key(&id)),
            self.room_destinations.iter().find_map(|(id, name)| (name == "scene").then_some(*id)).unwrap_or(0),
            buttons).into_bytes();
        canvas.write_png(&mut result)?;
        Ok(crate::message_log::with_sounds(result, &cues))
    }
    pub fn settings_command(&mut self, command: u64) -> Result<Vec<u8>> {
        let settings = self.settings.as_mut().ok_or("settings are not open")?;
        self.sound_cues = settings.script_command(&mut self.store, &mut self.system, command)?;
        if settings.take_close_request() {
            self.sound_cues = crate::message_log::page_cue(&mut self.system)?;
            self.settings = None;
        }
        self.input_sequence += 1;
        self.packet(None)
    }
    pub fn settings_tick(&mut self, elapsed: u64) -> Result<Vec<u8>> {
        if self.settings.is_none() {
            if let Some(view) = self.native_title.as_mut().filter(|view| view.delay() > 0) {
                let (action, cues) = view.step(
                    &mut self.store,
                    &mut self.system,
                    &mut self.machine.variables,
                    "0",
                    -1,
                    -1,
                    elapsed,
                )?;
                if action.is_some() {
                    return Err("title timer returned an action without input".into());
                }
                self.sound_cues = cues;
                return self.packet(None);
            }
        }
        let Some(settings) = self
            .settings
            .as_mut()
            .filter(|settings| settings.tick_delay() > 0)
        else {
            return Ok(Vec::new());
        };
        self.sound_cues = settings.tick(&mut self.store, &mut self.system, elapsed)?;
        self.packet(None)
    }
    pub fn install_settings_font(&mut self, bytes: Vec<u8>, index: u32) -> Result<()> {
        self.preview_font = Some(crate::text_render::load_font(bytes, index)?);
        Ok(())
    }
    pub fn export_preferences(&mut self) -> Result<Vec<u8>> {
        crate::preferences::export(&mut self.system)
    }
    pub fn import_preferences(&mut self, bytes: &[u8]) -> Result<()> {
        crate::preferences::import(&mut self.system, bytes)
    }
    fn require_room(&self, action: i32) -> Result<()> {
        if self
            .buttons
            .iter()
            .any(|button| button.id == action && button.enabled)
        {
            Ok(())
        } else {
            Err("room is locked".into())
        }
    }
    pub fn open_room(&mut self, id: u64) -> Result<Vec<u8>> {
        let id = i32::try_from(id)?;
        self.require_room(id)?;
        let name = self
            .room_destinations
            .get(&id)
            .ok_or("missing room destination")?;
        let (room, frame) = Room::open(
            &mut self.system,
            &mut self.machine.variables,
            &mut self.store,
            name,
            self.size,
        )?;
        self.room = Some(room);
        Ok(self.room_frame(frame))
    }
    pub fn room_input(&mut self, kind: i32, x: i32, y: i32) -> Result<Vec<u8>> {
        let frame = self.room.as_mut().ok_or("no open room")?.input(
            &mut self.system,
            &mut self.machine.variables,
            &mut self.store,
            kind,
            [x, y],
        )?;
        Ok(self.room_frame(frame))
    }
    fn room_frame(&mut self, outcome: room::Outcome) -> Vec<u8> {
        let music_count = outcome.music.len();
        self.room_music.extend(outcome.music);
        let mut packet = match outcome.frame {
            RoomFrame::Packet(packet) => packet,
            RoomFrame::Closed => {
                self.room = None;
                br#"{"kind":"room_closed"}"#.to_vec()
            }
            RoomFrame::Replay(name) => {
                self.room = None;
                self.room_replay = Some(name);
                br#"{"kind":"room_replay"}"#.to_vec()
            }
        };
        if music_count > 0 {
            let mut header = format!("{{\"room_music_count\":{music_count},").into_bytes();
            header.extend_from_slice(&packet[1..]);
            packet = header;
        }
        if outcome.sounds.is_empty() {
            packet
        } else {
            crate::message_log::with_sounds(packet, &outcome.sounds)
        }
    }
    /// Drain original room music requests in script order on the VM worker.
    pub fn next_room_music(&mut self) -> Result<Vec<u8>> {
        let command = self.room_music.pop_front().ok_or("no pending room music request")?;
        Ok(match command {
            room::MusicCommand::Play { name, looped, volume } => {
                let bytes = self.store.read("sound/bgm", &format!("{name}.ogg"))?;
                if !bytes.starts_with(b"OggS") { return Err("invalid room music resource".into()); }
                let category = crate::message_log::audio_volume(&mut self.system, "VolBGM")?;
                let mut packet = format!("{{\"host_effect\":\"bgm\",\"name\":{},\"duration_ms\":0,\"loop\":{looped},\"volume_percent\":{volume},\"category_percent\":{category}}}\n", quote(name.as_bytes())?).into_bytes();
                packet.extend(bytes);
                packet
            }
            room::MusicCommand::Volume(volume) => format!("{{\"host_effect\":\"bgm_volume\",\"duration_ms\":0,\"volume_percent\":{volume}}}\n").into_bytes(),
            room::MusicCommand::Stop => b"{\"host_effect\":\"bgm_stop\",\"duration_ms\":0}\n".to_vec(),
        })
    }
    pub fn replay(mut self) -> Result<StorySession<R>> {
        let name = self.room_replay.take().ok_or("no pending room replay")?;
        let source = self.store.load(name.as_bytes())?;
        let mut machine = Machine::new(source, 100_000, 32);
        machine.variables = self.machine.variables;
        let mut story = StorySession::from_bootstrap(self.store, machine, self.system)?;
        story.mark_replay();
        Ok(story)
    }
    pub fn start(self) -> Result<StorySession<R>> {
        let action = self.start_action;
        self.start_action(action)
    }
    pub fn is_start_action(&self, id: i32) -> bool {
        self.start_actions.contains(&id)
    }
    pub fn start_action(mut self, action: i32) -> Result<StorySession<R>> {
        if !self.is_start_action(action) {
            return Err("not a title Start action".into());
        }
        if self.native_title.is_some() {
            self.machine.complete(Some(action))?;
            return StorySession::from_bootstrap(self.store, self.machine, self.system);
        }
        // Dispatch the selected branch through the real .map handler.
        self.system
            .eval("title-choice", &format!("InputChoice = {action}"))?;
        let (machine, system) = bootstrap_continue(
            &mut self.store,
            self.machine,
            self.system,
            &mut std::io::sink(),
            false,
        )?;
        StorySession::from_bootstrap(self.store, machine, system)
    }
}
pub(crate) fn update_pointer(
    pressed: &mut Option<i32>,
    kind: i32,
    hit: Option<i32>,
) -> Result<Option<i32>> {
    match kind {
        0 => *pressed = hit,
        1 => {
            if hit != *pressed {
                *pressed = None
            }
        }
        2 => return Ok(pressed.take().filter(|id| Some(*id) == hit)),
        3 => *pressed = None,
        _ => return Err("invalid title pointer event".into()),
    }
    Ok(None)
}
pub(crate) fn contains(position: [i32; 2], size: [u32; 2], point: [i32; 2]) -> bool {
    (0..2).all(|i| {
        let d = i64::from(point[i]) - i64::from(position[i]);
        d >= 0 && d < i64::from(size[i])
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn touch_requires_matching_down_up_and_cancels_irrevocably() {
        let mut pressed = None;
        assert_eq!(update_pointer(&mut pressed, 2, Some(1)).unwrap(), None);
        update_pointer(&mut pressed, 0, Some(1)).unwrap();
        update_pointer(&mut pressed, 1, None).unwrap();
        update_pointer(&mut pressed, 1, Some(1)).unwrap();
        assert_eq!(update_pointer(&mut pressed, 2, Some(1)).unwrap(), None);
        update_pointer(&mut pressed, 0, Some(1)).unwrap();
        update_pointer(&mut pressed, 3, Some(1)).unwrap();
        assert_eq!(update_pointer(&mut pressed, 2, Some(1)).unwrap(), None);
        update_pointer(&mut pressed, 0, Some(1)).unwrap();
        assert_eq!(update_pointer(&mut pressed, 2, Some(2)).unwrap(), None);
        update_pointer(&mut pressed, 0, Some(1)).unwrap();
        assert_eq!(update_pointer(&mut pressed, 2, Some(1)).unwrap(), Some(1));
        assert_eq!(update_pointer(&mut pressed, 2, Some(1)).unwrap(), None);
    }
    #[test]
    fn hit_bounds_are_half_open_and_overflow_safe() {
        assert!(contains([-4, 1], [10, 4], [0, 2]));
        assert!(!contains([-4, 1], [10, 4], [6, 2]));
        assert!(!contains([i32::MIN, 0], [1, 1], [i32::MAX, 0]));
    }
}
