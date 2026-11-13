//! Original NEXT skips presentation until selection; BACK returns to a confirmed selection.

use super::*;

impl<R: Read + Seek> StorySession<R> {
    pub(super) fn shortcut_navigation_state(&mut self) -> Result<(bool, bool), Box<dyn Error>> {
        self.system.eval_with_resources(
            "shortcut-next-enabled",
            "return ((\"IsEnableSkipNext\" in getroottable()) && IsEnableSkipNext()).tostring()",
            &mut self.machine.variables,
            &mut self.store,
        )?;

        Ok((
            self.system.result() == "true",
            !self.navigation_back.is_empty(),
        ))
    }

    fn navigation_boundary(&self, sequence: u64) -> Result<(), Box<dyn Error>> {
        if self.dialogue_modal
            || self.failed
            || self.return_title_requested
            || self.menu.is_some()
            || self.settings.is_some()
            || self.log_ui.is_some()
            || self.dialogue_hidden
            || self.pending_effect.is_some()
            || self.page.as_ref().map(|page| page.sequence) != Some(sequence)
            || self
                .text_reveal
                .as_ref()
                .is_none_or(|reveal| reveal.advance)
            || self.skip_mode
            || self.auto_mode
            || !self.labels.in_route
        {
            return Err("navigation requires a live unadvanced route dialogue".into());
        }

        Ok(())
    }

    pub fn begin_next(&mut self, sequence: u64) -> Result<Vec<u8>, Box<dyn Error>> {
        self.navigation_boundary(sequence)?;

        if !self.shortcut_navigation_state()?.0 {
            return Err("NEXT is disabled by the mounted system script".into());
        }

        self.next_skip = true;

        self.system.eval("shortcut-next", "MegaSkip <- true")?;

        self.machine.complete(None)?;

        self.page = None;

        self.text_reveal = None;

        self.side_menu = None;

        self.speaker.clear();

        self.voice.clear();

        self.log_face.clear();

        self.dialogue_scene_png = None;

        self.poll_packet()
    }

    /// Original RestoreSkipState restores BGM before presenting the next selection.

    pub(super) fn finish_next_at_selection(&mut self) -> Result<bool, Box<dyn Error>> {
        if !self.next_skip {
            return Ok(false);
        }

        let (kind, name, audio_loop, payload) =
            if let Some((name, looping)) = self.persistent_audio.get("bgm") {
                let payload = self.store.read("sound/bgm", &format!("{name}.ogg"))?;

                if !payload.starts_with(b"OggS") {
                    return Err("invalid NEXT restore BGM resource".into());
                }

                ("bgm", name.clone(), *looping, payload)
            } else {
                ("bgm_stop", String::new(), false, Vec::new())
            };

        let frame = self.render()?;

        self.next_skip = false;

        self.system
            .eval("shortcut-next-finished", "MegaSkip <- false")?;

        self.host_sequence += 1;

        self.pending_effect = Some(HostEffect {
            token: self.host_sequence,

            kind: kind.into(),

            name,

            duration_ms: 0,

            movie_left_skip: false,

            audio_loop,

            audio_volume: 50,
            voice_tag: String::new(),

            payload,
        });

        self.host_sequence += 1;

        self.restore_audio.push_back(HostEffect {
            token: self.host_sequence,

            kind: "fade".into(),

            name: "0".into(),

            duration_ms: 0,

            movie_left_skip: false,

            audio_loop: false,

            audio_volume: 100,
            voice_tag: String::new(),

            payload: frame,
        });

        self.restoring = true;

        Ok(true)
    }

    /// Called only after validating the selected option and before completing its VM request.

    pub(super) fn capture_selection_checkpoint(&mut self) -> Result<(), Box<dyn Error>> {
        let bytes = self.serialize_selection_save()?;

        self.navigation_back.push(bytes);

        Ok(())
    }

    pub fn request_shortcut_back(&mut self, sequence: u64) -> Result<Vec<u8>, Box<dyn Error>> {
        self.navigation_boundary(sequence)?;

        let bytes = self
            .navigation_back
            .last()
            .ok_or("BACK has no confirmed selection")?;

        let saved = saves::Saved::decode_selection(bytes, &mut self.store, self.scene.dimensions)?;

        let packet = self.apply_saved(saved)?;

        self.navigation_back.pop();

        self.side_menu = None;

        self.log_ui = None;

        Ok(packet)
    }
}

#[cfg(test)]

mod tests {

    use super::*;

    use crate::story::tests::{resource_pack, session};

    #[test]

    fn next_executes_intermediate_commands_and_stops_at_selection() {
        let mut story = session(".var L1 flag\nstart\n? flag = 5\nmiddle\n.img 白\n.fade\n.flash\n.shake\n.wipe missing 100\n.movie op.wmv\n.bgm\n.select\none\ntwo\n.case 1\nchosen\n.case 2\nother\n.end".as_bytes());

        story
            .system
            .eval(
                "host",
                include_str!("../../../nekopack-script/host/headless.nut"),
            )
            .unwrap();

        story.system.eval("config", "Config <- {KeepSkip=0,KeepAuto=0,TextSpeed=0,KeepVoice=1,AutoSpeed=50,SkipNoRead=0,ReadedFast=0,AutoSelect=0}\nfunction IsEnableSkipNext(){return true}").unwrap();

        story.labels = StoryLabels {
            route: "synthetic".into(),
            in_route: true,
            ..Default::default()
        };

        story.enable_host_effects();

        story.enable_interactive_text();

        story.poll_packet().unwrap();

        story.text_frame(1, 0, false).unwrap();

        story.begin_next(1).unwrap();
        assert!(story.next_skip);
        let stop_token = story.pending_effect.as_ref().unwrap().token;
        story.acknowledge_effect(stop_token).unwrap();

        assert_eq!(story.machine.variables.get(b"flag").unwrap(), 5);

        assert_eq!(
            story
                .message_history
                .entries
                .iter()
                .map(|e| e.text.as_str())
                .collect::<Vec<_>>(),
            ["start", "middle"]
        );

        assert!(story.page.is_none());

        assert_eq!(story.pending_effect.as_ref().unwrap().kind, "bgm_stop");

        let token = story.pending_effect.as_ref().unwrap().token;

        story.acknowledge_effect(token).unwrap();

        let frame = story.pending_effect.as_ref().unwrap();

        assert_eq!(frame.kind, "fade");

        assert_eq!(
            nekopack_image::decode_png(&frame.payload, 4)
                .unwrap()
                .pixels(),
            &[255; 4]
        );

        let token = frame.token;

        story.acknowledge_effect(token).unwrap();

        assert_eq!(story.pending_effect.as_ref().unwrap().kind, "select");

        assert_eq!(story.machine.variables.get(b"flag").unwrap(), 5);

        assert_eq!(story.sequence, 1);
    }

    #[test]

    fn finishing_next_restores_final_scene_and_only_bgm() {
        let mut story = session(b".select\none\n.case 1\nchosen\n.end");

        story
            .system
            .eval("config", "Config <- {KeepSkip=0,KeepAuto=0,AutoSelect=0}")
            .unwrap();

        story.enable_host_effects();

        story.poll_packet().unwrap();

        story.pending_effect = None;

        story.next_skip = true;

        story.scene.background_color = Some([3, 6, 9, 255]);

        story
            .persistent_audio
            .insert("env".into(), ("wind".into(), true));

        assert!(story.finish_next_at_selection().unwrap());

        assert!(!story.next_skip);

        assert_eq!(story.pending_effect.as_ref().unwrap().kind, "bgm_stop");

        assert_eq!(story.restore_audio.len(), 1);

        assert_eq!(story.persistent_audio["env"], ("wind".into(), true));

        let token = story.pending_effect.as_ref().unwrap().token;

        story.acknowledge_effect(token).unwrap();

        let frame = story.pending_effect.as_ref().unwrap();

        assert_eq!(frame.kind, "fade");

        assert_eq!(
            nekopack_image::decode_png(&frame.payload, 4)
                .unwrap()
                .pixels(),
            &[3, 6, 9, 255]
        );

        let token = frame.token;

        story.acknowledge_effect(token).unwrap();

        assert_eq!(story.pending_effect.as_ref().unwrap().kind, "select");

        assert!(!story.finish_next_at_selection().unwrap());
    }

    #[test]

    fn back_restores_confirmed_selection_variables_scene_history_and_audio() {
        let script =
            b".var L1 flag\n.select\none\ntwo\n.case 1\n? flag = 9\nchosen\n.case 2\nother\n.end";

        let mut story = session(script);

        story.store = DatGame::mount([
            resource_pack(b"script", b"test.txt", script),
            resource_pack(b"sound/bgm", b"before.ogg", b"OggSsynthetic"),
        ])
        .unwrap();

        story
            .system
            .eval(
                "host",
                include_str!("../../../nekopack-script/host/headless.nut"),
            )
            .unwrap();

        story
            .system
            .eval(
                "labels",
                "StoryTitle <- \"\"; StoryRoute <- \"synthetic\"; function InRoute(){return true}",
            )
            .unwrap();

        story.system.eval("config", "Config <- {KeepSkip=0,KeepAuto=0,TextSpeed=0,KeepVoice=1,AutoSpeed=50,SkipNoRead=0,ReadedFast=0,AutoSelect=0,VolBGM=50,VolVO=50,VolENV=50,VolSE=50,VolSYS=50}").unwrap();

        story.labels = StoryLabels {
            route: "synthetic".into(),

            in_route: true,

            ..Default::default()
        };

        story.scene.background_color = Some([12, 34, 56, 255]);

        story
            .persistent_audio
            .insert("bgm".into(), ("before".into(), true));

        story
            .message_history
            .push(crate::message_log::Entry {
                name: "".into(),

                voice: "".into(),

                face: "".into(),

                text: "before".into(),
            })
            .unwrap();

        story.enable_host_effects();

        story.enable_interactive_text();

        story.poll_packet().unwrap();

        story.capture_selection_checkpoint().unwrap();

        let bytes = story.navigation_back.last().unwrap();

        assert!(saves::Saved::decode(bytes, &mut story.store, [1, 1], false).is_err());

        story.machine.complete(Some(1)).unwrap();

        story.pending_effect = None;

        story.poll_packet().unwrap();

        story.text_frame(1, 0, false).unwrap();

        assert_eq!(story.machine.variables.get(b"flag").unwrap(), 9);

        assert_eq!(story.message_history.entries.len(), 2);

        story.scene.background_color = Some([99; 4]);

        story.persistent_audio.clear();

        let result = story.request_shortcut_back(1).unwrap();

        assert!(
            std::str::from_utf8(result.split(|b| *b == b'\n').next().unwrap())
                .unwrap()
                .contains("\"host_effect\":\"bgm\"")
        );

        assert_eq!(story.machine.variables.get(b"flag").unwrap(), 0);

        assert_eq!(story.scene.background_color, Some([12, 34, 56, 255]));

        assert_eq!(story.message_history.entries.len(), 1);

        assert_eq!(story.persistent_audio["bgm"], ("before".into(), true));

        assert!(story.navigation_back.is_empty());

        let token = story.pending_effect.as_ref().unwrap().token;

        story.acknowledge_effect(token).unwrap();

        assert_eq!(story.pending_effect.as_ref().unwrap().kind, "fade");

        let fade = story.pending_effect.as_ref().unwrap();

        assert_eq!(
            nekopack_image::decode_png(&fade.payload, 4)
                .unwrap()
                .pixels(),
            &[12, 34, 56, 255]
        );

        let token = fade.token;

        story.acknowledge_effect(token).unwrap();

        assert_eq!(story.pending_effect.as_ref().unwrap().kind, "select");

        assert_eq!(story.machine.variables.get(b"flag").unwrap(), 0);

        assert!(story.page.is_none());
    }
}
