//! Retained original `com` coroutines and their native scene actions.
use super::*;
use crate::system_command::{ScriptReturn, StateAction, SystemCommand, SystemEvent, SystemFrame, SystemInput};
use nekopack_image::RgbaImage;
use nekopack_script::checkpoint::{Reader, Writer};

pub(super) fn register_commands(vm:&mut SystemVm,machine:&mut Machine)->Result<(),Box<dyn Error>> {
    vm.eval("system-command-contracts",r#"
        if(!("com" in getroottable()))return ""
        local commands=typeof com=="table"?com:com.entries,out=""
        foreach(name,handler in commands){
            if(typeof handler!="function")throw "invalid original command handler"
            local hex="";for(local i=0;i<name.len();++i)hex+=format("%02x",name[i]&255)
            out+=hex+"\t"+(handler.getinfos().parameters.len()>2?"1":"0")+"\n"
        }
        return out
    "#)?;
    for line in vm.result().lines(){
        let (hex,rows)=line.split_once('\t').ok_or("invalid original command registration")?;
        let name=(0..hex.len()).step_by(2).map(|i|u8::from_str_radix(&hex[i..i+2],16)).collect::<Result<Vec<_>,_>>()?;
        machine.register_system_command(&name,rows=="1");
    }
    Ok(())
}

pub(super) fn layer_bridge_available(vm: &mut SystemVm) -> Result<bool, Box<dyn Error>> {
    vm.eval("message-layer-contract", "return (\"NativeMessageLayerRun\" in getroottable()).tostring()")?;
    Ok(vm.result() == "true")
}

pub(super) fn capture_saved_system_state(vm: &mut SystemVm) -> Result<BTreeMap<String, i64>, Box<dyn Error>> {
    // The mounted SaveInfo.Save/Load pair persists this original message state.
    vm.eval("saved-message-state", r#"
        if(!("ClimaxCount" in getroottable()))return ""
        if(typeof ClimaxCount!="integer")throw "invalid original ClimaxCount type"
        return ClimaxCount.tostring()
    "#)?;
    let value = vm.result();
    Ok(if value.is_empty() { BTreeMap::new() } else { [("ClimaxCount".into(), value.parse()?)].into() })
}

pub(super) struct Pending {
    pub(super) command: SystemCommand,
    pub(super) token: u64,
    pub(super) name: String,
    pub(super) delay_ms: i32,
    pub(super) elapsed_ms: u64,
    pub(super) packet: Vec<u8>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::story::tests::{session, resource_pack};

    fn configured(script: &[u8], handler: &str) -> StorySession<std::io::Cursor<Vec<u8>>> {
        let mut story = session(script);
        story.system.eval("headless", include_str!("../../../nekopack-script/host/headless.nut")).unwrap();
        story.system.eval("labels", r#"
            StoryTitle <- "";StoryRoute <- "PROBE"
            function InRoute(){return StoryRoute.len()!=0}
            Config <- {VolBGM=50,VolVO=50,VolSE=50,VolENV=50,VolSYS=37,Effect=1,TextSpeed=100,KeepVoice=1,ReadedFast=0,AutoSpeed=50,SkipNoRead=1}
        "#).unwrap();
        story.system.eval("handler", handler).unwrap();
        story
    }

    #[test]
    fn original_cancel_modes_update_live_story_before_nested_wait() {
        let mut story=configured(b".cancel_modes\nSynthetic",r#"
            com["cancel_modes"] = function(args){CancelAuto();CancelSkip();wait(-1)}
        "#);
        story.auto_mode=true;story.skip_mode=true;story.next_skip=true;
        story.poll_packet().unwrap();
        assert!(!story.auto_mode && !story.skip_mode && !story.next_skip);
        assert!(story.system_command.is_some());
    }

    #[test]
    fn original_quit_retains_terminal_packet_without_advancing_old_story() {
        let mut story=configured(b".quit\nSynthetic",r#"
            com["quit"] = function(args){wait(-1);QuitGame();throw null}
        "#);
        story.poll_packet().unwrap();
        let terminal=story.system_command_step(1,1,1).unwrap();
        assert_eq!(terminal,b"{\"kind\":\"system_exit\",\"reason\":\"original_quit\"}\n");
        assert_eq!(story.poll_packet().unwrap(),terminal);
        assert!(!story.quit_autosave_ready().unwrap());
        assert_eq!(story.system_command_step(1,2,1).unwrap(),terminal);
    }

    #[test]
    fn native_message_wake_can_release_wait_without_a_game_event() {
        let mut story=configured(b".messages\nSynthetic",r#"
            Seen <- []
            com["messages"] = function(args){
                wait(-1);Seen.append(event());wait(1000);Seen.append(event())
            }
        "#);
        let first=story.poll_packet().unwrap();
        assert_eq!(first,story.system_command_step(1,50,0).unwrap());
        let second=story.system_command_pointer(1,60,0,Some([1,2])).unwrap();
        assert!(std::str::from_utf8(&second).unwrap().contains("\"delay_ms\":1000"));
        assert_eq!(second,story.system_command_step(1,70,0).unwrap());
        story.system_command_step(1,80,5).unwrap();
        story.system.eval("events","return Seen[0]+\",\"+Seen[1]").unwrap();
        assert_eq!(story.system.result(),"0,0");
    }

    #[test]
    fn original_config_save_waits_for_persistence_ack_before_returning() {
        let mut story=configured(b".config_probe\nSynthetic",r#"
            function SavePath(name){return name}
            function SaveConfig(){local f=file(SavePath("config.sav"),"wb");Save(f,Config)}
            Returned <- false
            com["config_probe"] = function(args){
                Config.TextSpeed=73;SaveConfig();Returned=true
            }
        "#);
        let packet=story.poll_packet().unwrap();
        assert!(std::str::from_utf8(&packet).unwrap().contains("\"save_config\":true"));
        story.system.eval("saved boundary","return Returned.tostring()").unwrap();
        assert_eq!(story.system.result(),"false");
        assert_eq!(packet,story.system_command_step(1,10,1).unwrap());
        assert_eq!(packet,story.system_command_step(1,20,0).unwrap());
        assert!(story.system_command.is_some());
        story.system_command_step(1,30,4).unwrap();
        story.system.eval("after acknowledgement","return Returned.tostring()+\":\"+Config.TextSpeed").unwrap();
        assert_eq!(story.system.result(),"true:73");
        assert!(story.system_command.is_none());
    }

    #[test]
    fn registered_waits_retain_one_token_and_require_explicit_continuation() {
        let mut story = configured(b".probe_wait\nSynthetic", r#"
            Count <- 0
            com["probe_wait"] = function(args){
                ++Count;wait(-1);if(event()!=ID_OK)throw "confirm required"
                ++Count;wait(-1);if(event()!=ID_CANCEL)throw "cancel required"
                ++Count
            }
        "#);
        let first = story.poll_packet().unwrap();
        assert!(std::str::from_utf8(&first).unwrap().contains("\"delay_ms\":-1"));
        assert_eq!(first, story.poll_packet().unwrap());
        assert_eq!(first, story.system_command_step(1, 10, 0).unwrap());
        assert!(story.system_command_step(2, 10, 1).is_err());
        assert!(story.export_quit_autosave().is_err());
        assert!(story.serialize_selection_save().unwrap_err().to_string().contains("suspended system command"));
        let second = story.system_command_step(1, 20, 1).unwrap();
        assert!(std::str::from_utf8(&second).unwrap().contains("\"token\":1"));
        assert!(story.system_command_step(1, 19, 2).is_err());
        let last = story.system_command_step(1, 30, 2).unwrap();
        assert!(!std::str::from_utf8(&last[..last.iter().position(|b| *b==b'\n').unwrap()]).unwrap().contains("system_command"));
        story.system.eval("count", "return Count.tostring()").unwrap();
        assert_eq!(story.system.result(), "3");
        assert!(story.system_command.is_none());
        assert!(story.system_command_step(1, 40, 1).is_err());
    }

    #[test]
    fn immediate_actions_and_closing_sound_are_consumed_once() {
        let mut story = configured(b".state\nSynthetic", r#"
            com["state"] = function(args){ClearImage();SwitchAuto();sound("CUSTOM")}
        "#);
        story.scene.background_color = Some([255, 0, 0, 255]);
        story.store = DatGame::mount([resource_pack(b"sound/se", b"se_CUSTOM.ogg", b"OggSprobe")]).unwrap();
        story.poll_packet().unwrap();
        assert!(story.auto_mode);
        assert!(story.scene.background_color.is_none());
        let sound = story.next_system_command_sound().unwrap();
        assert!(sound.ends_with(b"OggSprobe"));
        assert!(std::str::from_utf8(&sound).unwrap().contains("\"category_percent\":37"));
        assert_eq!(story.next_system_command_sound().unwrap(), b"{\"empty\":true}\n");
    }

    #[test]
    fn cumulative_command_clock_is_converted_to_deltas() {
        let mut story = configured(b".timed\nSynthetic", r#"
            Times <- []
            com["timed"] = function(args){local t=Timer();wait(25);Times.append(t.Get());wait(25);Times.append(t.Get())}
        "#);
        let first = story.poll_packet().unwrap();
        assert_eq!(first, story.system_command_step(1, 24, 0).unwrap());
        story.system_command_step(1, 30, 0).unwrap();
        story.system_command_step(1, 70, 0).unwrap();
        story.system.eval("times", "return Times[0]+\",\"+Times[1]").unwrap();
        assert_eq!(story.system.result(), "30,70");
    }

    #[test]
    fn delay_waits_for_voice_and_roundtrips_its_old_scene_and_clock() {
        let mut story = configured(b".delay\nSynthetic", r#"
            DelayTime <- 0
            com["delay"] = function(args){DelayTime=args.len()?args[0].tointeger():0;EnableDelay()}
        "#);
        let image = RgbaImage::filled(1, 1, [200, 0, 0, 255], PIXELS).unwrap();
        let mut png=Vec::new();image.write_png(&mut png).unwrap();story.presented=Some(png);
        story.scene.background_color=Some([0, 200, 0, 255]);
        story.poll_packet().unwrap();
        assert!(story.pending_effect.is_none());
        assert_eq!(story.retained_delay_frame().unwrap().pixels(), [200,0,0,255]);
        story.voice="VOICE".into();
        story.delayed_scene_tick(100, true, false).unwrap();
        assert_eq!(story.delayed_motion_time(100).unwrap(), None);
        let mut writer=Writer::default();story.delayed_scene.as_ref().unwrap().save(&mut writer,story.motion_elapsed).unwrap();
        let mut reader=Reader::new(&writer.0).unwrap();
        story.delayed_scene=Some(Delay::load(&mut reader,[1,1]).unwrap());reader.finish().unwrap();
        story.delayed_scene_tick(0, true, false).unwrap();
        assert_eq!(story.delayed_motion_time(0).unwrap(), None);
        story.delayed_scene_tick(20, true, true).unwrap();
        assert_eq!(story.delayed_motion_time(20).unwrap(), Some(0));
        assert_eq!(story.delayed_motion_time(70).unwrap(), Some(50));
        let frame=story.page_frame(1,70).unwrap();
        assert!(frame.ends_with(&[0,200,0,255]));
    }

    #[test]
    fn positive_delay_uses_strict_original_threshold() {
        let mut story=configured(b".delay 125\nSynthetic", r#"
            DelayTime <- 0
            com["delay"] = function(args){DelayTime=args[0].tointeger();EnableDelay()}
        "#);
        story.poll_packet().unwrap();
        story.delayed_scene_tick(125,false,false).unwrap();
        assert_eq!(story.delayed_motion_time(125).unwrap(),None);
        story.delayed_scene_tick(126,false,false).unwrap();
        assert_eq!(story.delayed_motion_time(126).unwrap(),Some(0));
    }

    #[test]
    fn ordinary_system_return_is_ignored_by_the_original_contract() {
        let mut story=configured(b".choose\nSynthetic", "com[\"choose\"] = function(args){return 2}");
        assert!(story.poll_packet().is_ok());
        assert!(!story.failed);
    }

    #[test]
    fn delayed_scene_full_save_restores_remaining_threshold_and_pixels() {
        let code=b".delay 125\nSynthetic";
        let handler="DelayTime <- 0\ncom[\"delay\"] = function(args){DelayTime=args[0].tointeger();EnableDelay()}";
        let mut story=configured(code,handler);
        story.store=DatGame::mount([resource_pack(b"script",b"test.txt",code)]).unwrap();
        let image=RgbaImage::filled(1,1,[200,0,0,255],PIXELS).unwrap();
        let mut png=Vec::new();image.write_png(&mut png).unwrap();story.presented=Some(png);
        story.scene.background_color=Some([0,200,0,255]);
        story.poll_packet().unwrap();story.delayed_scene_tick(50,false,false).unwrap();
        let saved=story.export_route_preview().unwrap();
        assert_eq!(&saved[..5],b"NKSV\x09");
        let mut restored=configured(code,handler);
        restored.store=DatGame::mount([resource_pack(b"script",b"test.txt",code)]).unwrap();
        restored.restore_route_preview(&saved).unwrap();
        assert_eq!(decode_png(&restored.page.as_ref().unwrap().png,PIXELS).unwrap().pixels(),[200,0,0,255]);
        restored.delayed_scene_tick(75,false,false).unwrap();
        assert_eq!(restored.delayed_motion_time(75).unwrap(),None);
        restored.delayed_scene_tick(76,false,false).unwrap();
        assert_eq!(restored.delayed_motion_time(76).unwrap(),Some(0));
        assert!(restored.page_frame(restored.page.as_ref().unwrap().sequence,76).unwrap().ends_with(&[0,200,0,255]));
    }

    #[test]
    fn message_hook_runs_once_and_original_saved_counter_survives_cold_restore() {
        let code=b".setclimax 6\nSynthetic";
        let make=|| {
            let mut story=configured(code, r#"
                ClimaxCount <- 0
                com["setclimax"] = function(args){ClimaxCount=args[0].tointeger()}
                function UpdateClimax(){if(!InRestore())++ClimaxCount}
            "#);
            story.store=DatGame::mount([resource_pack(b"script",b"test.txt",code)]).unwrap();
            let adapter=crate::message_adapter::layer_adapter("com[\"\"] = function(args){\nUpdateClimax()\n}\n").unwrap();
            story.system.eval("message-layer-adapter",&adapter).unwrap();
            story.message_layers_enabled=layer_bridge_available(&mut story.system).unwrap();
            story
        };
        let mut story=make();story.poll_packet().unwrap();story.poll_packet().unwrap();
        assert_eq!(story.system_save_state.get("ClimaxCount"),Some(&7));
        story.tick_message_layers(false,100).unwrap();
        assert_eq!(story.system_save_state.get("ClimaxCount"),Some(&7));
        let saved=story.export_route_preview().unwrap();
        let mut restored=make();restored.restore_route_preview(&saved).unwrap();
        assert_eq!(restored.system_save_state.get("ClimaxCount"),Some(&7));
        restored.system.eval("restored-count","return ClimaxCount.tostring()").unwrap();
        assert_eq!(restored.system.result(),"7");
        restored.tick_message_layers(false,100).unwrap();
        assert_eq!(restored.system_save_state.get("ClimaxCount"),Some(&7));
    }

    #[test]
    fn screen_captures_last_message_overlay_while_base_rebuilds_scene() {
        let mut story=configured(b".capture\nSynthetic", r#"
            com["capture"] = function(args){
                local sprite=Sprite(),screen=Screen()
                sprite.Screen();screen.Draw(sprite);screen.Update();wait(-1)
                sprite.Base();screen.Draw(sprite);screen.Update();wait(-1)
            }
        "#);
        story.scene.background_color=Some([0,0,200,255]);
        story.presented_raw=Some(RgbaImage::filled(1,1,[200,0,0,255],PIXELS).unwrap());
        story.last_message_overlay=Some((RgbaImage::filled(1,1,[0,200,0,255],PIXELS).unwrap(),[0,0]));
        let image=|packet:Vec<u8>| {
            let at=packet.iter().position(|value|*value==b'\n').unwrap()+1;
            decode_png(&packet[at..],PIXELS).unwrap()
        };
        assert_eq!(image(story.poll_packet().unwrap()).pixels(),[0,200,0,255]);
        assert!(story.last_message_overlay.is_none());
        assert_eq!(image(story.system_command_step(1,100,1).unwrap()).pixels(),[0,0,200,255]);
    }

    #[test]
    fn completed_text_keeps_delay_pending_until_the_strict_time_threshold() {
        let mut story=configured(b".delay 1000\nSynthetic", r#"
            DelayTime <- 0
            com["delay"] = function(args){DelayTime=args[0].tointeger();EnableDelay()}
        "#);
        story.scene.background_color=Some([0,200,0,255]);
        story.enable_interactive_text();
        story.poll_packet().unwrap();
        let sequence=story.page.as_ref().unwrap().sequence;
        for elapsed in [0,500,1000] {
            let packet=String::from_utf8(story.text_frame_with_voice(sequence,elapsed,false,true).unwrap()).unwrap();
            assert!(packet.contains("\"complete\":true"),"{packet}");
            assert!(packet.contains("\"delay_pending\":true"),"{packet}");
            assert!(packet.contains("\"auto\":false"),"{packet}");
            assert!(packet.contains("\"skip\":false"),"{packet}");
            assert!(packet.contains("\"message_animating\":false"),"{packet}");
            assert!(packet.contains("\"advance\":false"),"{packet}");
            assert!(story.page_frame(sequence,elapsed).unwrap().ends_with(&[0,0,0,255]));
        }
        let packet=String::from_utf8(story.text_frame_with_voice(sequence,1001,false,true).unwrap()).unwrap();
        assert!(packet.contains("\"delay_pending\":false"),"{packet}");
        assert!(packet.contains("\"advance\":false"),"{packet}");
        assert!(story.page_frame(sequence,1001).unwrap().ends_with(&[0,200,0,255]));
        assert_eq!(story.page.as_ref().unwrap().sequence,sequence);
    }

    #[test]
    fn completed_voiced_text_keeps_zero_delay_pending_until_voice_finishes() {
        let mut story=configured(b".delay\n:Probe:VOICE:XX\nSynthetic", r#"
            DelayTime <- 0
            com["delay"] = function(args){DelayTime=0;EnableDelay()}
        "#);
        story.store=DatGame::mount([resource_pack(b"voice",b"VOICE.ogg",b"OggSsynthetic")]).unwrap();
        story.scene.background_color=Some([0,0,200,255]);
        story.enable_interactive_text();
        story.poll_packet().unwrap();
        let sequence=story.page.as_ref().unwrap().sequence;
        assert_eq!(story.page.as_ref().unwrap().voice,"VOICE");
        for elapsed in [0,1000,10000] {
            let packet=String::from_utf8(story.text_frame_with_voice(sequence,elapsed,false,false).unwrap()).unwrap();
            assert!(packet.contains("\"complete\":true"),"{packet}");
            assert!(packet.contains("\"voice_finished\":false"),"{packet}");
            assert!(packet.contains("\"delay_pending\":true"),"{packet}");
            assert!(packet.contains("\"advance\":false"),"{packet}");
            assert!(story.page_frame(sequence,elapsed).unwrap().ends_with(&[0,0,0,255]));
        }
        let packet=String::from_utf8(story.text_frame_with_voice(sequence,10001,false,true).unwrap()).unwrap();
        assert!(packet.contains("\"delay_pending\":false"),"{packet}");
        assert!(packet.contains("\"advance\":false"),"{packet}");
        assert!(story.page_frame(sequence,10001).unwrap().ends_with(&[0,0,200,255]));
        assert_eq!(story.page.as_ref().unwrap().sequence,sequence);
    }
}

pub(super) struct Sound {
    name: String,
    volume: u32,
    payload: Vec<u8>,
}

/// EnableDelay retains the pre-action screen until the message's DelayAction.
pub(super) struct Delay {
    before: RgbaImage,
    time_ms: i32,
    released_at: Option<u64>,
    elapsed_ms: u64,
    restore_base: u64,
}

impl Delay {
    pub(super) fn pending(&self) -> bool { self.released_at.is_none() }
    pub(super) fn into_before(self) -> RgbaImage { self.before }
    pub(super) fn save(&self, w: &mut Writer, motion_elapsed: u64) -> Result<(), Box<dyn Error>> {
        let elapsed_ms = match self.released_at {
            Some(start) => self.elapsed_ms.max(start.checked_add(motion_elapsed).ok_or("saved delayed scene clock overflow")?),
            None => self.elapsed_ms,
        };
        w.number(self.time_ms as u32 as u64);
        w.number(elapsed_ms);
        w.number(u64::from(self.released_at.is_some()));
        if let Some(time) = self.released_at { w.number(time); }
        let mut png = Vec::new();
        self.before.write_png(&mut png)?;
        w.bytes(&png);
        Ok(())
    }

    pub(super) fn load(r: &mut Reader<'_>, dimensions: [u32; 2]) -> Result<Self, Box<dyn Error>> {
        let time_ms = u32::try_from(r.number()?)? as i32;
        let elapsed_ms = r.number()?;
        let released_at = match r.number()? {
            0 => None,
            1 => Some(r.number()?),
            _ => return Err("invalid saved delayed scene state".into()),
        };
        if released_at.is_some_and(|time| time > elapsed_ms) {
            return Err("invalid saved delayed scene clock".into());
        }
        let before = decode_png(r.bytes()?, PIXELS)?;
        if [before.width(), before.height()] != dimensions {
            return Err("saved delayed scene dimensions changed".into());
        }
        Ok(Self { before, time_ms, released_at, elapsed_ms, restore_base: elapsed_ms })
    }
}

impl<R: Read + Seek> StorySession<R> {
    pub(super) fn retain_message_overlay(&mut self, sequence: u64) -> Result<(), Box<dyn Error>> {
        self.last_message_overlay = match (&self.text_renderer, &self.text_reveal) {
            (Some(renderer), Some(reveal)) => Some((renderer.frame_pixels(sequence, reveal.count())?, renderer.frame_origin())),
            _ => None,
        };
        Ok(())
    }
    pub(super) fn refresh_message_box(&mut self) -> Result<bool, Box<dyn Error>> {
        if !self.message_layers_enabled || self.text_renderer.is_none() { return Ok(false); }
        let snapshot = crate::message_adapter::message_box(&mut self.system)?;
        let style = crate::preferences::style(&mut self.system)?;
        let changed = self.text_renderer.as_mut().unwrap().set_message_box(&mut self.store, snapshot, style, self.scene.dimensions)?;
        if changed { self.message_overlay_snapshot = None; }
        self.dialogue_style = Some(style);
        Ok(changed)
    }

    pub(super) fn tick_message_layers(&mut self, begin: bool, elapsed: u64) -> Result<(), Box<dyn Error>> {
        if !self.message_layers_enabled { return Ok(()); }
        let delta = elapsed.checked_sub(self.message_layers_elapsed).ok_or("message layer clock moved backwards")?;
        let delta = u32::try_from(delta).ok().filter(|value| *value <= i32::MAX as u32).ok_or("message layer clock exceeds VM range")?;
        let layers = crate::message_adapter::message_layers(&mut self.system, &mut self.store, &mut self.machine.variables,
            begin, self.message_layers_rest, delta)?;
        self.message_layers_active = layers.keep_animating;
        self.message_layers_elapsed = elapsed;
        self.system_save_state = capture_saved_system_state(&mut self.system)?;
        if self.message_overlay_snapshot.as_ref() != Some(&layers.overlay) {
            if let Some(renderer) = &mut self.text_renderer {
            let overlay = crate::text_render::raster_message_layers(&mut self.store, &layers.overlay, self.scene.dimensions)?;
            renderer.set_overlay(overlay)?;
                self.message_overlay_snapshot = Some(layers.overlay);
            }
        }
        Ok(())
    }

    pub(super) fn has_system_handler(&mut self, name: &str) -> Result<bool, Box<dyn Error>> {
        self.system.eval("story-system-handler", &format!(
            "return ((\"com\" in getroottable()) && ((typeof com==\"table\") ? com.rawin({0}) : com.entries.rawin({0}))).tostring()",
            quote(name.as_bytes())?,
        ))?;
        Ok(self.system.result() == "true")
    }

    /// Returns true when the Machine request is still retained by a coroutine.
    pub(super) fn start_system_command(&mut self, name: &str, args: &[Vec<u8>], rows: &[Vec<u8>]) -> Result<bool, Box<dyn Error>> {
        let mut motions = self.motions.clone();
        let positions = motions.iter_mut().map(|motion| (motion.slot, motion.position(self.motion_elapsed))).collect::<Vec<_>>();
        let scene = self.scene.render_pixels(&positions)?;
        let mut current = if let Some(frame) = &self.presented_raw {
            RgbaImage::from_rgba(frame.width(), frame.height(), frame.pixels().to_vec(), PIXELS)?
        } else { self.previous_frame()? };
        if let Some((overlay, origin)) = &self.last_message_overlay { current.draw(overlay, *origin); }
        let takes_rows=self.machine.pending_system_command_takes_rows()==Some(true);
        let (mut command, frame) = SystemCommand::open_with_screen_and_mask(
            &mut self.store, &mut self.system, &mut self.machine.variables,
            name, args, takes_rows.then_some(rows), scene, current,
            SystemInput { elapsed_ms: 0, event: SystemEvent::None, skip: self.skip_mode || self.next_skip, pointer: None },
            std::mem::take(&mut self.transition_mask), self.preview_font.clone(),self.system_snapshot.take(),
        )?;
        self.host_sequence += 1;
        let token = self.host_sequence;
        let result=if self.machine.pending_system_command_takes_rows()==Some(true) {match frame.return_value.as_ref(){Some(ScriptReturn::Integer(value))=>Some(*value as i32),Some(_)=>Some(0),None=>None}}else{None};
        let quitting = frame.state_actions.contains(&StateAction::Quit);
        let closed = frame.closed;
        let delay_ms = frame.delay_ms;
        let packet = self.consume_system_frame(token, name, 0, frame)?;
        if let Some(scene)=self.native_room_scene_frame()? {command.set_scene(scene);}
        if closed && !quitting {
            self.system_load_catalog_clear()?;
            let (mask,snapshot)=command.into_scene_state();self.transition_mask=mask;self.system_snapshot=Some(snapshot);
            self.machine.complete(result)?;
        } else {
            self.system_command = Some(Pending { command, token, name: name.into(), delay_ms, elapsed_ms: 0, packet });
        }
        Ok(!closed || quitting)
    }

    /// `elapsed_ms` is cumulative since the first packet for this token.
    /// Events: 0 = timer, 1 = confirm, 2 = cancel. Polling never resumes a yield.
    pub fn system_command_step(&mut self, token: u64, elapsed_ms: u64, event: u32) -> Result<Vec<u8>, Box<dyn Error>> {
        self.system_command_pointer(token,elapsed_ms,event,None)
    }

    pub fn system_command_pointer(&mut self, token:u64,elapsed_ms:u64,event:u32,pointer:Option<[i32;2]>) -> Result<Vec<u8>,Box<dyn Error>> {
        let message_wake=event==5 || pointer.is_some();
        let event = match event {
            0 | 5 => SystemEvent::None,
            1 => SystemEvent::Confirm,
            2 => SystemEvent::Cancel,
            3 => SystemEvent::Drag,
            4 => SystemEvent::ConfigSaved,
            6 => SystemEvent::NativeAck,
            _ => return Err("invalid system command event".into()),
        };
        let pending = self.system_command.as_ref().ok_or("no pending system command")?;
        if self.failed || pending.token != token { return Err("stale system command token".into()); }
        if pending.command.awaiting_native() && !matches!(event,SystemEvent::NativeAck) {return Ok(pending.packet.clone());}
        if pending.command.awaiting_config_save() && !matches!(event,SystemEvent::ConfigSaved) {return Ok(pending.packet.clone());}
        if pending.packet.starts_with(b"{\"kind\":\"system_exit\"") { return Ok(pending.packet.clone()); }
        let delta = elapsed_ms.checked_sub(pending.elapsed_ms).ok_or("system command clock moved backwards")?;
        if !message_wake && matches!(event, SystemEvent::None) && (pending.delay_ms < 0 || delta < pending.delay_ms as u64) {
            return Ok(pending.packet.clone());
        }
        let delta = u32::try_from(delta).ok().filter(|value| *value <= i32::MAX as u32)
            .ok_or("system command clock exceeds VM range")?;
        let pending = self.system_command.as_mut().unwrap();
        let name = pending.name.clone();
        let result = pending.command.step(
            &mut self.store, &mut self.system, &mut self.machine.variables,
            SystemInput {elapsed_ms: delta, event, skip: self.skip_mode || self.next_skip,pointer},
        );
        let frame = match result { Ok(frame) => frame, Err(error) => return Err(self.stop(error)) };
        if let Some(id)=frame.state_actions.iter().find_map(|action|match action {StateAction::LoadCommit(id)=>Some(id.clone()),_=>None}) {
            if !frame.closed {return Err(self.stop("native load committed before original coroutine unwind".into()));}
            return self.commit_system_load(&id);
        }
        let result=if self.machine.pending_system_command_takes_rows()==Some(true) {match frame.return_value.as_ref(){Some(ScriptReturn::Integer(value))=>Some(*value as i32),Some(_)=>Some(0),None=>None}}else{None};
        let quitting = frame.state_actions.contains(&StateAction::Quit);
        let closed = frame.closed;
        let delay_ms = frame.delay_ms;
        let packet = match self.consume_system_frame(token, &name, elapsed_ms, frame) {
            Ok(packet) => packet,
            Err(error) => return Err(self.stop(error)),
        };
        if let Some(scene)=self.native_room_scene_frame()? {self.system_command.as_mut().unwrap().command.set_scene(scene);}
        if closed && !quitting {
            self.system_load_catalog_clear()?;
            let (mask,snapshot)=self.system_command.take().unwrap().command.into_scene_state();self.transition_mask=mask;self.system_snapshot=Some(snapshot);
            self.machine.complete(result)?;
            self.poll_packet()
        } else {
            let pending = self.system_command.as_mut().unwrap();
            pending.elapsed_ms = elapsed_ms;
            pending.delay_ms = delay_ms;
            pending.packet = packet.clone();
            Ok(packet)
        }
    }

    pub(super) fn system_command_packet(&self) -> Option<Vec<u8>> {
        self.system_command.as_ref().map(|pending| pending.packet.clone())
    }

    fn consume_system_frame(&mut self, token: u64, name: &str, elapsed_ms: u64, frame: SystemFrame) -> Result<Vec<u8>, Box<dyn Error>> {
        let quitting=frame.state_actions.contains(&StateAction::Quit);
        let mut save_config=false;let mut restore_audio=false;let mut restore_screen=false;
        let mut load_catalog=false;let mut native_ack=frame.native_pending;
        let mut room_actions=Vec::new();
        for action in &frame.state_actions {
            if let StateAction::Room(action)=action {
                use super::native_room::Action;
                let detail=match action {
                    Action::Image(words)=>Some(("image",words.as_str())),
                    Action::Commit=>Some(("commit","")),
                    Action::AssignImage(name)=>Some(("assign_image",name.as_str())),
                    Action::ScriptReset=>Some(("script_reset","")),
                    Action::Replay(name)=>Some(("replay",name.as_str())),
                    _=>None,
                };
                if let Some((kind,value))=detail {room_actions.push(format!("{{\"kind\":\"{kind}\",\"value\":{}}}",quote(value.as_bytes())?));}
            }
        }
        let room_actions=room_actions.join(",");
        for action in frame.state_actions {
            match action {
                StateAction::Quit => {},
                StateAction::LoadCatalog => {self.system_load_catalog_clear()?;load_catalog=true;}
                StateAction::LoadState(id) => {self.prepare_system_load(&id)?;native_ack=true;}
                StateAction::LoadCommit(_) => return Err("load commit must replace the old machine at its boundary".into()),
                StateAction::Room(action) => {self.consume_native_room_action(action)?;} 
                StateAction::SaveConfig => save_config=true,
                StateAction::RestoreAudio => restore_audio=true,
                StateAction::RestoreScreen => restore_screen=true,
                StateAction::ClearScene => {
                    self.scene.clear();
                    self.scene.mapped_actors.clear();
                    self.motions.clear();
                    self.motion_elapsed = 0;
                    self.motion_restore_base = 0;
                    self.delayed_scene = None;
                    self.dialogue_scene_png = None;
                }
                StateAction::CancelAuto => {self.auto_mode=false;}
                StateAction::CancelSkip => {self.skip_mode=false;self.next_skip=false;}
                StateAction::ToggleAuto => {
                    self.auto_mode = !self.auto_mode;
                    if self.auto_mode { self.skip_mode = false; }
                }
                StateAction::EnableDelay(time_ms) => {
                    let before = self.previous_frame()?;
                    self.delayed_scene = Some(Delay {before, time_ms, released_at: None, elapsed_ms: 0, restore_base: 0});
                    self.dialogue_scene_png = None;
                }
            }
        }
        for sound in frame.sounds {
            let volume = u32::try_from(sound.volume).ok().filter(|value| *value <= 100)
                .ok_or("original system sound volume is outside 0..100")?;
            let name = format!("se_{}", sound.name);
            if let Some(payload) = self.store.read_optional("sound/se", &format!("{name}.ogg"))? {
                if !payload.starts_with(b"OggS") { return Err("system command sound is not Ogg".into()); }
                self.system_command_sounds.push_back(Sound {name, volume, payload});
            }
        }
        self.labels = StoryLabels::capture(&mut self.system)?;
        self.system_save_state = capture_saved_system_state(&mut self.system)?;
        let room_replay=self.native_room_replay_pending();
        if load_catalog || room_replay {native_ack=false;}
        let room_audio=self.native_room_audio_count();
        let has_frame = frame.frame.is_some();
        let buttons=frame.buttons.iter().map(|(name,b)|Ok(format!("{{\"name\":{},\"bounds\":[{},{},{},{}]}}",quote(name.as_bytes())?,b[0],b[1],b[2],b[3]))).collect::<Result<Vec<_>,Box<dyn Error>>>()?.join(",");
        let width=frame.size[0];let height=frame.size[1];
        let mut packet = format!("{{\"kind\":\"system_command\",\"token\":{token},\"command\":{},\"elapsed_ms\":{elapsed_ms},\"delay_ms\":{},\"closed\":{},\"has_frame\":{has_frame},\"buttons\":[{buttons}],\"size\":[{width},{height}],\"room_actions\":[{room_actions}],\"room_replay\":{room_replay},\"room_audio\":{room_audio},\"load_catalog\":{load_catalog},\"native_ack\":{native_ack},\"save_config\":{save_config},\"restore_audio\":{restore_audio},\"restore_screen\":{restore_screen}}}\n", quote(name.as_bytes())?, frame.delay_ms, frame.closed).into_bytes();
        if let Some(image) = frame.frame {
            self.last_message_overlay = None;
            let mut png = Vec::new();
            image.write_png(&mut png)?;
            packet.extend_from_slice(&png);
            self.presented = Some(png.clone());
            self.presented_raw = None;
            self.dialogue_scene_png = Some(png);
        }
        if quitting {return Ok(b"{\"kind\":\"system_exit\",\"reason\":\"original_quit\"}\n".to_vec());}
        Ok(packet)
    }

    /// Drain one original system sound. An empty queue is an explicit packet.
    pub fn next_system_command_sound(&mut self) -> Result<Vec<u8>, Box<dyn Error>> {
        let Some(sound) = self.system_command_sounds.pop_front() else {
            return Ok(b"{\"empty\":true}\n".to_vec());
        };
        let mut packet = format!("{{\"host_effect\":\"system\",\"name\":{},\"category_percent\":{},\"duration_ms\":0,\"loop\":false}}\n", quote(sound.name.as_bytes())?, sound.volume).into_bytes();
        packet.extend(sound.payload);
        Ok(packet)
    }

    pub(super) fn delayed_scene_png(&self) -> Result<Option<Vec<u8>>, Box<dyn Error>> {
        let Some(delay) = self.delayed_scene.as_ref().filter(|delay| delay.released_at.is_none()) else { return Ok(None); };
        let mut png = Vec::new();
        delay.before.write_png(&mut png)?;
        Ok(Some(png))
    }

    pub(super) fn delayed_scene_tick(&mut self, elapsed: u64, complete: bool, voice_finished: bool) -> Result<(), Box<dyn Error>> {
        let Some(delay) = &mut self.delayed_scene else { return Ok(()); };
        let elapsed = elapsed.checked_add(delay.restore_base).ok_or("delayed scene clock overflow")?;
        if elapsed < delay.elapsed_ms { return Err("delayed scene clock moved backwards".into()); }
        delay.elapsed_ms = elapsed;
        let due = if delay.time_ms != 0 && !self.skip_mode && !self.next_skip {
            i128::from(elapsed) > i128::from(delay.time_ms)
        } else {
            complete && (self.voice.is_empty() || voice_finished || self.skip_mode || self.next_skip)
        };
        if delay.released_at.is_none() && due {
            delay.released_at = Some(elapsed);
            self.motion_elapsed = 0;
            self.motion_restore_base = 0;
            self.dialogue_scene_png = None;
        }
        Ok(())
    }

    pub(super) fn delayed_motion_time(&self, elapsed: u64) -> Result<Option<u64>, Box<dyn Error>> {
        match &self.delayed_scene {
            None => Ok(Some(elapsed)),
            Some(delay) => match delay.released_at {
                None => Ok(None),
                Some(start) => Ok(Some(elapsed.checked_add(delay.restore_base).ok_or("delayed scene clock overflow")?.saturating_sub(start))),
            },
        }
    }

    pub(super) fn commit_delayed_scene(&mut self, elapsed: u64) -> Result<(), Box<dyn Error>> {
        if let Some(delay) = &mut self.delayed_scene {
            let elapsed = elapsed.checked_add(delay.restore_base).ok_or("delayed scene clock overflow")?;
            delay.released_at.get_or_insert(elapsed);
            delay.elapsed_ms = delay.elapsed_ms.max(elapsed);
            self.motions.clear();
            self.motion_elapsed = 0;
            self.motion_restore_base = 0;
            self.dialogue_scene_png = None;
        }
        Ok(())
    }

    pub(super) fn retained_delay_frame(&self) -> Option<&RgbaImage> {
        self.delayed_scene.as_ref().filter(|delay| delay.released_at.is_none()).map(|delay| &delay.before)
    }
}
