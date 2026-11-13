//! BGV channels and token rules come from the mounted native executable.
use super::*;

pub(super) fn role_volume(vm: &mut SystemVm, tag: &str) -> Result<u32, Box<dyn Error>> {
    vm.eval(
        "bgv-role-volume",
        &format!(
            r#"
        local tag={};
        local mute=(tag in Config.VoiceMute) ? Config.VoiceMute[tag] : 0;
        local volume=(tag in Config.VoiceVolume) ? Config.VoiceVolume[tag] : 0;
        if(typeof mute!="integer" && typeof mute!="bool")throw "invalid BGV role mute";
        if(typeof volume!="integer" || volume<0 || volume>100)throw "invalid BGV role volume";
        return ((mute!=0 && mute!=false) ? 0 : volume).tostring();
    "#,
            quote(tag.as_bytes())?
        ),
    )?;
    Ok(vm.result().parse()?)
}

pub(super) fn kind(channel: u8) -> Result<&'static str, Box<dyn Error>> {
    match channel {
        2 => Ok("bgv1"),
        3 => Ok("bgv2"),
        _ => Err("invalid native BGV channel".into()),
    }
}

impl<R: Read + Seek> StorySession<R> {
    pub(super) fn bgv_audio_config(&mut self) -> Result<Vec<u8>, Box<dyn Error>> {
        let mut packet = crate::audio_config::packet(&mut self.system)?;
        if let Some(profile) = self.store.bgv_profile.as_ref() {
            let fields = profile
                .channels
                .iter()
                .map(|channel| {
                    Ok(format!(
                        "{}:{}",
                        quote(kind(channel.channel)?.as_bytes())?,
                        quote(channel.voice_suppression_substring.as_bytes())?
                    ))
                })
                .collect::<Result<Vec<_>, Box<dyn Error>>>()?;
            packet.pop();
            packet.extend(format!(",\"bgv_voice_tags\":{{{}}}}}", fields.join(",")).bytes());
        }
        Ok(packet)
    }

    pub(super) fn bgv_command(&mut self, args: &[Vec<u8>]) -> Result<(), Box<dyn Error>> {
        let resolved = if args.is_empty() {
            crate::engine_abi::BgvResolved {
                channel: 0,
                tag: String::new(),
                name: String::new(),
            }
        } else {
            self.store
                .bgv_profile
                .as_ref()
                .ok_or("unknown native BGV command contract")?
                .parse(args)?
        };
        let (kind, volume) = if resolved.channel == 0 {
            ("bgv_stop".to_owned(), 100)
        } else {
            let channel = kind(resolved.channel)?;
            if resolved.name.is_empty() {
                (format!("{channel}_stop"), 100)
            } else {
                let volume = role_volume(&mut self.system, &resolved.tag)?;
                (
                    if self.next_skip {
                        format!("{channel}_stop")
                    } else {
                        channel.to_owned()
                    },
                    volume,
                )
            }
        };
        self.effects.push(if resolved.name.is_empty() {
            kind.clone()
        } else {
            format!("{kind} {}", resolved.name)
        });
        if !self.host_mode {
            return Ok(());
        }
        let playing = matches!(kind.as_str(), "bgv1" | "bgv2");
        let payload = if playing {
            let bytes = self
                .store
                .read("sound/bgv", &format!("{}.ogg", resolved.name))?;
            if !bytes.starts_with(b"OggS") {
                return Err("BGV resource is not Ogg".into());
            }
            bytes
        } else {
            Vec::new()
        };
        self.host_sequence += 1;
        self.pending_effect = Some(HostEffect {
            token: self.host_sequence,
            kind,
            name: if playing {
                resolved.name
            } else {
                String::new()
            },
            duration_ms: 0,
            movie_left_skip: false,
            audio_loop: playing,
            audio_volume: volume,
            voice_tag: if playing { resolved.tag } else { String::new() },
            payload,
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine_abi::{BgvChannel, BgvProfile};
    use crate::story::tests::{resource_pack, session};

    fn profile() -> BgvProfile {
        BgvProfile {
            channels: [
                BgvChannel {
                    channel: 2,
                    tag: "LEFT".into(),
                    voice_suppression_substring: "speaker_a".into(),
                },
                BgvChannel {
                    channel: 3,
                    tag: "RIGHT".into(),
                    voice_suppression_substring: "speaker_b".into(),
                },
            ],
            name_prefix: "_clip_".into(),
        }
    }
    fn fixture(text: &[u8], payload: &[u8]) -> StorySession<std::io::Cursor<Vec<u8>>> {
        let mut s = session(text);
        s.store = DatGame::mount([
            resource_pack(b"script", b"test.txt", text),
            resource_pack(b"sound/bgv", b"LEFT_clip_001.ogg", payload),
        ])
        .unwrap();
        s.store.bgv_profile = Some(profile());
        s.system
            .eval(
                "roles",
                r#"
            Config <- {VoiceMute={LEFT=false},VoiceVolume={LEFT=73},SkipNoRead=0};
            StoryTitle <- ""; StoryRoute <- ""; Host <- {caption=""};
        "#,
            )
            .unwrap();
        s.enable_host_effects();
        s
    }
    #[test]
    fn bgv_play_stop_and_ordinary_skip_keep_independent_loop_state() {
        let mut s = fixture(b".bgv LEFT 001\n.bgv RIGHT\n.bgv\nSynthetic", b"OggS");
        s.skip_mode = true;
        s.persistent_audio
            .insert("env".into(), ("rain".into(), true));
        s.persistent_audio
            .insert("bgv2".into(), ("old".into(), true));
        s.persistent_bgv_tags.insert("bgv2".into(), "RIGHT".into());
        let packet = s.poll_packet().unwrap();
        let effect = s.pending_effect.as_ref().unwrap();
        assert_eq!(
            (
                &*effect.kind,
                &*effect.name,
                effect.audio_volume,
                effect.audio_loop
            ),
            ("bgv1", "LEFT_clip_001", 73, true)
        );
        assert!(
            String::from_utf8(packet)
                .unwrap()
                .contains("\"volume_percent\":73")
        );
        assert!(!s.persistent_audio.contains_key("bgv1"));
        s.acknowledge_effect(effect.token).unwrap();
        assert_eq!(s.persistent_bgv_tags["bgv1"], "LEFT");
        let effect = s.pending_effect.as_ref().unwrap();
        assert_eq!(effect.kind, "bgv2_stop");
        s.acknowledge_effect(effect.token).unwrap();
        assert!(!s.persistent_audio.contains_key("bgv2"));
        assert!(s.persistent_audio.contains_key("bgv1"));
        let effect = s.pending_effect.as_ref().unwrap();
        assert_eq!(effect.kind, "bgv_stop");
        s.acknowledge_effect(effect.token).unwrap();
        assert_eq!(s.persistent_audio.len(), 1);
        assert!(s.persistent_audio.contains_key("env"));
        assert!(s.persistent_bgv_tags.is_empty());
    }
    #[test]
    fn bgv_mega_skip_suppresses_load_and_missing_role_keys_mean_zero() {
        let mut s = fixture(b".bgv LEFT absent\nSynthetic", b"OggS");
        s.next_skip = true;
        s.persistent_audio
            .insert("bgv1".into(), ("old".into(), true));
        s.persistent_bgv_tags.insert("bgv1".into(), "LEFT".into());
        s.poll_packet().unwrap();
        let effect = s.pending_effect.as_ref().unwrap();
        assert_eq!(effect.kind, "bgv1_stop");
        assert!(effect.payload.is_empty());
        s.finish_effect(effect.token).unwrap();
        assert!(!s.persistent_audio.contains_key("bgv1"));
        let mut vm = SystemVm::new();
        vm.eval("roles", "Config <- {VoiceMute={},VoiceVolume={}} ")
            .unwrap();
        assert_eq!(role_volume(&mut vm, "missing").unwrap(), 0);
        vm.eval(
            "roles",
            "Config.VoiceVolume.LEFT <- 88;Config.VoiceMute.LEFT <- true",
        )
        .unwrap();
        assert_eq!(role_volume(&mut vm, "LEFT").unwrap(), 0);
        vm.eval("roles", "Config.VoiceMute.LEFT=false").unwrap();
        assert_eq!(role_volume(&mut vm, "LEFT").unwrap(), 88);
    }
    #[test]
    fn bgv_missing_contract_resource_and_corrupt_audio_are_explicit_errors() {
        let mut unknown = session(b".bgv LEFT 001");
        unknown.enable_host_effects();
        assert!(
            unknown
                .poll_packet()
                .unwrap_err()
                .to_string()
                .contains("unknown native BGV")
        );
        let mut missing = fixture(b".bgv LEFT missing", b"OggS");
        assert!(missing.poll_packet().is_err());
        assert!(missing.failed);
        let mut corrupt = fixture(b".bgv LEFT 001", b"broken");
        assert!(
            corrupt
                .poll_packet()
                .unwrap_err()
                .to_string()
                .contains("BGV resource is not Ogg")
        );
        assert!(corrupt.failed);
    }
    #[test]
    fn bgv_save_restores_loop_tag_and_recomputes_current_role_volume() {
        let mut s = fixture(b".bgv LEFT 001\nSynthetic", b"OggS");
        s.poll_packet().unwrap();
        let token = s.pending_effect.as_ref().unwrap().token;
        s.acknowledge_effect(token).unwrap();
        s.labels.in_route = true;
        s.labels.route = "route".into();
        s.host_mode = false;
        let bytes = s.export_route_preview().unwrap();
        assert_eq!(&bytes[..5], b"NKSV\x09");
        let version_eight = without_native_metadata(&s, &bytes);
        let mut version_seven = version_eight[..version_eight.len()-8].to_vec();
        version_seven[4] = 7;
        let checksum = nekopack_script::checkpoint::digest(&version_seven[13..]);
        version_seven[5..13].copy_from_slice(&checksum.to_le_bytes());
        saves::Saved::decode(&version_seven, &mut s.store, [1, 1], true).unwrap();
        let mut version_six = version_eight[..version_eight.len()-16].to_vec();
        version_six[4] = 6;
        let checksum = nekopack_script::checkpoint::digest(&version_six[13..]);
        version_six[5..13].copy_from_slice(&checksum.to_le_bytes());
        saves::Saved::decode(&version_six, &mut s.store, [1, 1], true).unwrap();
        let saved = saves::Saved::decode(&bytes, &mut s.store, [1, 1], true).unwrap();
        s.system
            .eval("changed-role", "Config.VoiceVolume.LEFT=42")
            .unwrap();
        s.host_mode = true;
        s.apply_saved(saved).unwrap();
        let effect = s.pending_effect.as_ref().unwrap();
        assert_eq!(
            (&*effect.kind, &*effect.voice_tag, effect.audio_volume),
            ("bgv1", "LEFT", 42)
        );
        assert!(effect.audio_loop);
        assert_eq!(s.persistent_bgv_tags["bgv1"], "LEFT");
    }
    #[test]
    fn audio_config_exports_only_profile_supplied_voice_substrings() {
        let mut s = fixture(b"Synthetic", b"OggS");
        s.system.eval("volumes", "Config.VolBGM<-75;Config.VolVO<-80;Config.VolSE<-50;Config.VolENV<-50;Config.VolSYS<-50").unwrap();
        let packet = String::from_utf8(s.audio_config().unwrap()).unwrap();
        assert!(
            packet.contains("\"bgv_voice_tags\":{\"bgv1\":\"speaker_a\",\"bgv2\":\"speaker_b\"}")
        );
    }
    #[test]
    fn version_five_saves_remain_readable_and_bad_bgv_tags_are_rejected() {
        let mut s = fixture(b"Synthetic", b"OggS");
        s.poll_packet().unwrap();
        s.labels.in_route = true;
        s.labels.route = "route".into();
        s.host_mode = false;
        let bytes = s.export_route_preview().unwrap();
        let mut old = without_native_metadata(&s, &bytes);
        // v6 adds BGV tags, v7 delay, v8 the system-state collection.
        old.truncate(old.len() - 24);
        old[4] = 5;
        let checksum = nekopack_script::checkpoint::digest(&old[13..]);
        old[5..13].copy_from_slice(&checksum.to_le_bytes());
        saves::Saved::decode(&old, &mut s.store, [1, 1], false).unwrap();
        s.persistent_audio
            .insert("bgv1".into(), ("LEFT_clip_001".into(), true));
        s.persistent_bgv_tags
            .insert("bgv1".into(), "unrecognized".into());
        let invalid = s.export_route_preview().unwrap();
        assert!(saves::Saved::decode(&invalid, &mut s.store, [1, 1], false).is_err());
        s.persistent_bgv_tags.clear();
        let missing = s.export_route_preview().unwrap();
        assert!(saves::Saved::decode(&missing, &mut s.store, [1, 1], false).is_err());
    }

    fn without_native_metadata(s: &StorySession<std::io::Cursor<Vec<u8>>>, bytes: &[u8]) -> Vec<u8> {
        let mut extension = nekopack_script::checkpoint::Writer::default();
        extension.number(s.native_load.messages.len() as u64);
        for message in &s.native_load.messages { message.save(&mut extension); }
        extension.number(s.navigation_back.len() as u64);
        for checkpoint in &s.navigation_back { extension.bytes(checkpoint); }
        assert!(bytes.ends_with(&extension.0));
        bytes[..bytes.len()-extension.0.len()].to_vec()
    }
}
