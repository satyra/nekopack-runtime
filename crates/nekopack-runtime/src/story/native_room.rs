//! Native actions emitted by the original room on its retained command stack.
use super::*;
use nekopack_image::RgbaImage;
use nekopack_script::runtime::Scripts;
use std::collections::VecDeque;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Action {
    Play { name: String, looped: bool, volume: u32 },
    Stop,
    Volume(u32),
    Image(String),
    Commit,
    AssignImage(String),
    ScriptReset,
    Replay(String),
}

fn unhex(value: &str) -> Result<String> {
    if value.len() % 2 != 0 || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("invalid room action UTF-8 hex".into());
    }
    Ok(String::from_utf8((0..value.len()).step_by(2)
        .map(|i| u8::from_str_radix(&value[i..i + 2], 16))
        .collect::<std::result::Result<Vec<_>, _>>()?)?)
}

fn resource_name(value: String) -> Result<String> {
    if value.is_empty() || value == "." || value == ".." || value.contains(['/', '\\', ':', '\r', '\n', '\t']) {
        return Err("invalid original room resource name".into());
    }
    Ok(value)
}

fn volume(value: &str) -> Result<u32> {
    let value: u32 = value.parse()?;
    if value > 100 { return Err("original room volume is outside 0..100".into()); }
    Ok(value)
}

/// Fields after the U tag. Unknown non-room actions belong to the caller.
pub(crate) fn parse(fields: &[&str]) -> Result<Option<Action>> {
    Ok(Some(match fields {
        ["room_music_play", name, looping, gain] => Action::Play {
            name: resource_name(unhex(name)?)?,
            looped: match *looping { "0" => false, "1" => true, _ => return Err("invalid room music loop flag".into()) },
            volume: volume(gain)?,
        },
        ["room_music_stop"] => Action::Stop,
        ["room_music_volume", gain] => Action::Volume(volume(gain)?),
        ["room_image", words] => Action::Image(unhex(words)?),
        ["room_commit"] => Action::Commit,
        ["room_assign_image", name] => Action::AssignImage(unhex(name)?),
        ["room_script_reset"] => Action::ScriptReset,
        ["room_replay", name] => Action::Replay(resource_name(unhex(name)?)?),
        [name, ..] if name.starts_with("room_") => return Err("invalid original room action".into()),
        _ => return Ok(None),
    }))
}

struct ReplayReturn {
    machine: Machine,
    command: system_commands::Pending,
    replay: bool,
    return_id: u64,
}

#[derive(Default)]
pub(crate) struct NativeRoomState {
    audio: VecDeque<Vec<u8>>,
    replay_request: Option<String>,
    scene_dirty: bool,
    callers: Vec<ReplayReturn>,
    return_sequence: u64,
}

impl<R: Read + Seek> StorySession<R> {
    pub(super) fn consume_native_room_action(&mut self, action: Action) -> Result<()> {
        match action {
            Action::Play { name, looped, volume } => {
                let payload = self.store.read("sound/bgm", &format!("{name}.ogg"))?;
                if !payload.starts_with(b"OggS") { return Err("invalid original room music resource".into()); }
                let category = crate::message_log::audio_volume(&mut self.system, "VolBGM")?;
                let mut packet = format!("{{\"host_effect\":\"bgm\",\"name\":{},\"duration_ms\":0,\"loop\":{looped},\"volume_percent\":{volume},\"category_percent\":{category}}}\n", quote(name.as_bytes())?).into_bytes();
                packet.extend(payload);
                self.native_room.audio.push_back(packet);
                if looped { self.persistent_audio.insert("bgm".into(), (name, true)); }
                else { self.persistent_audio.remove("bgm"); }
            }
            Action::Stop => {
                self.native_room.audio.push_back(b"{\"host_effect\":\"bgm_stop\",\"duration_ms\":0}\n".to_vec());
                self.persistent_audio.remove("bgm");
            }
            Action::Volume(volume) => self.native_room.audio.push_back(format!("{{\"host_effect\":\"bgm_volume\",\"duration_ms\":0,\"volume_percent\":{volume}}}\n").into_bytes()),
            Action::Image(words) => {
                let fields = words.split_whitespace().collect::<Vec<_>>();
                match fields.as_slice() {
                    ["1", name] => {
                        if let Some(color) = match *name { "white" => Some([255;4]), "black" => Some([0,0,0,255]), _ => solid(name) } {
                            self.scene.background = None;
                            self.scene.background_name = None;
                            self.scene.background_color = Some(color);
                        } else {
                            let bytes = self.store.read("image/back", &format!("{name}.png"))?;
                            self.scene.set_background((*name).into(), decode_png(&bytes, PIXELS)?)?;
                        }
                    }
                    ["0", name] => {
                        let image = crate::scene::read_visual(&mut self.store, name, PIXELS)?;
                        self.scene.set_visual((*name).into(), image)?;
                    }
                    _ => return Err("unsupported original room ImageAction".into()),
                }
                self.native_room.scene_dirty = true;
                self.dialogue_scene_png = None;
            }
            Action::Commit => { self.native_room.scene_dirty = true; }
            Action::AssignImage(name) => {
                if name.is_empty() { self.scene.clear(); self.scene.mapped_actors.clear(); }
                else {
                    let image = crate::scene::read_visual(&mut self.store, &name, PIXELS)?;
                    self.scene.set_visual(name, image)?;
                }
                self.native_room.scene_dirty = true;
                self.dialogue_scene_png = None;
            }
            Action::ScriptReset => {
                // This resets the scenario presentation, never the suspended room
                // coroutine or its caller's Machine request.
                self.scene.clear(); self.scene.mapped_actors.clear();
                self.motions.clear(); self.motion_elapsed = 0; self.motion_restore_base = 0;
                self.delayed_scene = None; self.dialogue_scene_png = None;
                self.native_room.scene_dirty = true;
            }
            Action::Replay(name) => {
                if self.native_room.replay_request.is_some() { return Err("original room replay already pending".into()); }
                self.native_room.replay_request = Some(name);
            }
        }
        Ok(())
    }

    /// Refresh the command's C scene after a native image boundary, before ACK.
    pub(super) fn native_room_scene_frame(&mut self) -> Result<Option<RgbaImage>> {
        if !self.native_room.scene_dirty { return Ok(None); }
        let image = self.scene.render_pixels(&[])?;
        self.native_room.scene_dirty = false;
        Ok(Some(image))
    }

    pub(super) fn native_room_replay_pending(&self) -> bool { self.native_room.replay_request.is_some() }
    pub(super) fn native_room_audio_count(&self) -> usize { self.native_room.audio.len() }

    /// Drain ordered, actual resource packets on the VM worker.
    pub fn next_system_room_audio(&mut self) -> Result<Vec<u8>> {
        Ok(self.native_room.audio.pop_front().unwrap_or_else(|| b"{\"empty\":true}\n".to_vec()))
    }

    /// Platform state must be sampled before the next original wait iteration.
    pub fn system_room_music_state(&mut self, playing: bool) -> Result<()> {
        self.system.eval("original-room-music-state", &format!("SystemRoomMusicState({playing})"))?;
        Ok(())
    }

    /// Called only for the user's production replay request. Metadata inspection
    /// stops before this method and therefore never loads replay text.
    pub fn start_system_room_replay(&mut self) -> Result<Vec<u8>> {
        let name = self.native_room.replay_request.as_ref().ok_or("no original room replay request")?;
        let source = self.store.load(name.as_bytes())?;
        let return_id=self.native_room.return_sequence.checked_add(1).ok_or("native child return id exhausted")?;
        self.native_room.return_sequence=return_id;
        let command = self.system_command.take().ok_or("original room coroutine is not retained")?;
        self.system.eval("original-room-save-stack", r#"
            if (!("NativeRoomSuspendedHosts" in getroottable())) NativeRoomSuspendedHosts <- []
            NativeRoomSuspendedHosts.append([SystemCommandHost,SystemGuiState,SystemRoomHost])
        "#)?;
        let mut replay = Machine::new(source, STORY_STEP_BUDGET, 32);
        replay.variables = std::mem::take(&mut self.machine.variables);
        let machine = std::mem::replace(&mut self.machine, replay);
        self.native_room.callers.push(ReplayReturn { machine, command, replay: self.replay, return_id });
        self.native_room.replay_request = None;
        self.replay = true;
        self.return_title_requested = false;
        self.page = None; self.text_reveal = None; self.pending_effect = None;
        self.voice.clear(); self.speaker.clear(); self.effects.clear();
        self.poll_packet()
    }

    /// The existing replay completion path returns to this exact suspended room.
    pub(super) fn finish_system_room_replay(&mut self) -> Result<Option<Vec<u8>>> {
        let Some(mut caller) = self.native_room.callers.pop() else { return Ok(None); };
        if self.system_command.is_some() { return Err("replay command still active at return".into()); }
        caller.machine.variables = std::mem::take(&mut self.machine.variables);
        self.machine = caller.machine;
        self.replay = caller.replay;
        self.return_title_requested = false;
        self.page = None; self.text_reveal = None; self.pending_effect = None;
        self.voice.clear(); self.speaker.clear(); self.effects.clear();
        self.system.eval("original-room-restore-stack", r#"
            local saved=NativeRoomSuspendedHosts.pop()
            SystemCommandHost=saved[0];SystemGuiState=saved[1];SystemRoomHost=saved[2]
            SystemRoomReplayComplete()
        "#)?;
        let token=caller.command.token;
        let elapsed=caller.command.elapsed_ms;
        let return_id=caller.return_id;
        self.system_command = Some(caller.command);
        let mut packet=self.system_command_step(token,elapsed,6)?;
        // Also annotate a dialogue/next-command packet when the parent handler
        // returns immediately: platform replay mode still has to be restored.
        let marker=format!("\"native_child_return\":true,\"native_child_return_id\":{return_id},\"native_child_return_token\":{token},");
        packet.splice(1..1,marker.bytes());
        if let Some(pending)=self.system_command.as_mut().filter(|pending|pending.token==token) {
            // Returning a child presentation exposes its parent window again.
            // The platform may deliver one event=0 window-message wake after
            // this boundary; ordinary polling/timer ticks remain unchanged.
            pending.packet=packet.clone();
        }
        Ok(Some(packet))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::story::tests::{resource_pack, session};

    #[test]
    fn resource_audio_packets_keep_order_and_real_payload() {
        let mut story=session(b"Synthetic");
        story.system.eval("config", "Config <- {VolBGM=37,VolVO=100,VolSE=100,VolENV=100,VolSYS=100}").unwrap();
        story.store=DatGame::mount([resource_pack(b"sound/bgm", b"fixture.ogg", b"OggSsynthetic")]).unwrap();
        for fields in [vec!["room_music_stop"],vec!["room_music_play","66697874757265","0","80"],vec!["room_music_volume","65"]] {
            story.consume_native_room_action(parse(&fields).unwrap().unwrap()).unwrap();
        }
        assert!(String::from_utf8(story.next_system_room_audio().unwrap()).unwrap().contains("bgm_stop"));
        let packet=story.next_system_room_audio().unwrap();
        assert!(packet.ends_with(b"OggSsynthetic"));
        assert!(String::from_utf8_lossy(&packet).contains("\"category_percent\":37"));
        assert!(String::from_utf8_lossy(&packet).contains("\"loop\":false,\"volume_percent\":80"));
        assert!(String::from_utf8(story.next_system_room_audio().unwrap()).unwrap().contains("\"volume_percent\":65"));
        assert_eq!(story.next_system_room_audio().unwrap(),b"{\"empty\":true}\n");
        assert!(!story.persistent_audio.contains_key("bgm"));
        story.consume_native_room_action(Action::Play {name:"fixture".into(),looped:true,volume:100}).unwrap();
        assert_eq!(story.persistent_audio.get("bgm"),Some(&("fixture".to_owned(),true)));
        story.consume_native_room_action(Action::Stop).unwrap();
        assert!(!story.persistent_audio.contains_key("bgm"));
    }

    #[test]
    fn cg_scene_is_applied_before_command_render_and_replay_is_metadata_only() {
        let mut story=session(b"Synthetic");
        story.consume_native_room_action(Action::Image("1 white".into())).unwrap();
        assert_eq!(story.native_room_scene_frame().unwrap().unwrap().pixels(), &[255,255,255,255]);
        assert!(story.native_room_scene_frame().unwrap().is_none());
        // session has no replay resource. Recording metadata must not load it.
        story.consume_native_room_action(Action::Replay("unread_fixture".into())).unwrap();
        assert!(story.native_room_replay_pending());
        assert!(parse(&["room_music_play","2e2e2f66697874757265","1","100"]).is_err());
        assert!(parse(&["room_music_volume","101"]).is_err());
        assert!(parse(&["room_replay","0"]).is_err());
    }

    #[test]
    fn synthetic_replay_runs_commands_and_returns_to_the_original_room_stack() {
        let mut story=session(b".room_probe\nSynthetic caller returned");
        story.store=DatGame::mount([resource_pack(b"script",b"synthetic_replay.txt",b".replay_wait\n")]).unwrap();
        story.system.eval("headless",include_str!("../../../nekopack-script/host/headless.nut")).unwrap();
        story.system.eval("fixture",r#"
            StoryTitle <- "";StoryRoute <- "PROBE"
            function InRoute(){return StoryRoute.len()!=0}
            Config <- {VolBGM=50,VolVO=50,VolSE=50,VolENV=50,VolSYS=50,Effect=1}
            Returned <- 0;ReplayWaitReturned <- 0
            com["room_probe"] = function(args){
                ScriptReset();AssignImage("");ScriptEval("synthetic_replay")
                Returned++;wait(-1)
            }
            com["replay_wait"] = function(args){wait(-1);ReplayWaitReturned++}
        "#).unwrap();
        let packet=story.poll_packet().unwrap();
        assert!(String::from_utf8_lossy(&packet).contains("room_probe"));
        assert!(story.native_room_replay_pending());
        let room_token=story.system_command.as_ref().unwrap().token;
        let replay_packet=story.start_system_room_replay().unwrap();
        assert!(String::from_utf8_lossy(&replay_packet).contains("replay_wait"));
        let replay_token=story.system_command.as_ref().unwrap().token;
        assert_ne!(room_token,replay_token);
        let returned=story.system_command_step(replay_token,50,1).unwrap();
        assert!(String::from_utf8_lossy(&returned).contains("room_probe"));
        assert!(String::from_utf8_lossy(&returned).contains("\"native_child_return\":true"));
        assert_eq!(story.poll_packet().unwrap(),returned);
        assert_eq!(story.system_command.as_ref().unwrap().token,room_token);
        story.system.eval("verify original stack","return Returned+\":\"+ReplayWaitReturned").unwrap();
        assert_eq!(story.system.result(),"1:1");
        assert!(!story.replay);
        assert!(!story.return_title_requested);
    }

    #[test]
    fn child_return_marks_the_next_command_when_parent_returns_immediately() {
        let mut story=session(b".room_probe\n.next_probe\n");
        story.store=DatGame::mount([resource_pack(b"script",b"synthetic_replay.txt",b".replay_wait\n")]).unwrap();
        story.system.eval("headless",include_str!("../../../nekopack-script/host/headless.nut")).unwrap();
        story.system.eval("fixture",r#"
            StoryTitle <- "";StoryRoute <- "PROBE"
            function InRoute(){return true}
            Config <- {VolBGM=50,VolVO=50,VolSE=50,VolENV=50,VolSYS=50,Effect=1}
            com["room_probe"] = function(args){ScriptEval("synthetic_replay")}
            com["replay_wait"] = function(args){wait(10)}
            com["next_probe"] = function(args){wait(-1)}
        "#).unwrap();
        story.poll_packet().unwrap();
        let parent=story.system_command.as_ref().unwrap().token;
        story.start_system_room_replay().unwrap();
        let child=story.system_command.as_ref().unwrap().token;
        let returned=story.system_command_step(child,10,0).unwrap();
        let text=String::from_utf8(returned).unwrap();
        assert!(text.contains("\"native_child_return\":true"));
        assert!(text.contains(&format!("\"native_child_return_token\":{parent}")));
        assert!(text.contains("\"command\":\"next_probe\""));
        assert_ne!(story.system_command.as_ref().unwrap().token,parent);
    }

    #[test]
    #[ignore="requires isolated nested-room-replay-fixture-v2; executes only packaged synthetic replay bytes"]
    fn original_scene_child_return_requires_one_window_message_to_redraw() {
        type Story=StorySession<std::fs::File>;
        fn header(packet:&[u8])->&str {std::str::from_utf8(packet.split(|b|*b==b'\n').next().unwrap()).unwrap()}
        fn step(story:&mut Story,event:u32,point:Option<[i32;2]>)->Vec<u8> {
            let pending=story.system_command.as_ref().unwrap();
            story.system_command_pointer(pending.token,pending.elapsed_ms+50,event,point).unwrap()
        }
        fn point(packet:&[u8],name:&str)->[i32;2] {
            let block=header(packet).split("{\"name\":\"").find(|part|part.split('"').next().unwrap().contains(name)).unwrap();
            let raw=block.split("\"bounds\":[").nth(1).unwrap().split(']').next().unwrap();
            let xy=raw.split(',').map(|s|s.parse::<i32>().unwrap()).collect::<Vec<_>>();
            [xy[0]+xy[2]/2,xy[1]+xy[3]/2]
        }
        fn settled(story:&mut Story,mut packet:Vec<u8>,ready:impl Fn(&str)->bool)->Vec<u8> {
            for _ in 0..20 {
                let text=header(&packet);
                if ready(text){return packet;}
                let event=if text.contains("\"load_catalog\":true") {
                    story.system_load_catalog_clear().unwrap();story.system_load_catalog_install().unwrap();6
                }else if text.contains("\"native_ack\":true") {6}else{0};
                assert!(event!=0 || !text.contains("\"delay_ms\":-1"),"unexpected input wait: {text}");
                packet=step(story,event,None);
            }
            panic!("original room did not settle");
        }
        let directory=std::path::Path::new("../../.local/system-integration/nested-room-replay-fixture-v2");
        let mut paths=std::fs::read_dir(directory).unwrap().map(|entry|entry.unwrap().path()).filter(|path|
            path.extension().is_some_and(|extension|extension=="dat"||extension=="exe")).collect::<Vec<_>>();paths.sort();
        assert!(!paths.iter().any(|path|path.file_name().unwrap()=="script.dat"));
        let sources=paths.iter().map(|path|(path.file_name().unwrap().to_str().unwrap().to_owned(),std::fs::File::open(path).unwrap())).collect::<Vec<_>>();
        let mut store=DatGame::mount_named(sources,true).unwrap();
        // This exact generated resource was packed by the fixture builder.
        // Assert bytes before permitting production replay loading in the test.
        assert_eq!(store.read("script","CS01_H_11H.txt").unwrap(),b".native_replay_probe\n");
        let title=crate::title::TitleSession::open(store).unwrap();
        let mut story=title.start().unwrap();story.enable_host_effects();
        story.install_dialogue_font(std::fs::read("C:/Windows/Fonts/msgothic.ttc").unwrap(),0).unwrap();
        let packet=story.poll_packet().unwrap();
        let music=settled(&mut story,packet,|text|text.contains("\"has_frame\":true")&&text.contains(":PLAY:"));
        let token=story.system_command.as_ref().unwrap().token;
        let next=step(&mut story,1,Some(point(&music,":SCENE:")));
        let scene=settled(&mut story,next,|text|text.contains("\"has_frame\":true")&&text.contains(":SCENE:")&&!text.contains(":PLAY:"));
        let scene_buttons=header(&scene).split("\"buttons\":").nth(1).unwrap().split(",\"size\"").next().unwrap().to_owned();
        let replay_boundary=step(&mut story,1,Some(point(&scene,"::1")));
        assert!(header(&replay_boundary).contains("\"room_replay\":true"));
        let child=story.start_system_room_replay().unwrap();
        let _child=settled(&mut story,child,|text|text.contains("\"command\":\"native_replay_probe\"")&&text.contains("\"delay_ms\":250"));
        let child_pending=story.system_command.as_ref().unwrap();
        let returned=story.system_command_step(child_pending.token,child_pending.elapsed_ms+250,0).unwrap();
        assert!(header(&returned).contains("\"native_child_return\":true"));
        assert!(header(&returned).contains("\"has_frame\":false"));
        assert!(header(&returned).contains("\"delay_ms\":-1"));
        assert_eq!(story.system_command.as_ref().unwrap().token,token);
        assert_eq!(story.poll_packet().unwrap(),returned);
        let elapsed=story.system_command.as_ref().unwrap().elapsed_ms;
        assert_eq!(story.system_command_step(token,elapsed+1000,0).unwrap(),returned,"ordinary timer must not release the original wait");
        let redrawn=story.system_command_step(token,elapsed,5).unwrap();
        assert!(header(&redrawn).contains("\"has_frame\":true"));
        assert!(!header(&redrawn).contains("\"native_child_return\":true"));
        assert_eq!(header(&redrawn).split("\"buttons\":").nth(1).unwrap().split(",\"size\"").next().unwrap(),scene_buttons);
        let caller=story.system_command_step(token,elapsed+50,2).unwrap();
        assert!(!header(&caller).contains("\"kind\":\"system_command\""));
        assert!(story.system_command.is_none());
    }
}
