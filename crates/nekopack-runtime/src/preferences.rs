//! Portable, bounded runtime preferences. Not the original engine's config.sav.
use nekopack_script::system::SystemVm;
use std::{collections::BTreeMap, error::Error};
pub(crate) type Result<T> = std::result::Result<T, Box<dyn Error>>;
pub(crate) const FIELDS: [&str; 12] = [
    "TextSpeed",
    "KeepVoice",
    "VolBGM",
    "VolVO",
    "VolSE",
    "VolENV",
    "VolSYS",
    "BoxOpacity",
    "Bold",
    "Border",
    "AutoSpeed",
    "SkipNoRead",
];
const MAGIC: &[u8] = b"NKCF\x06";
pub(crate) const EXTENDED: [&str; 3] = ["KeepSkip", "KeepAuto", "Movie"];
pub(crate) const READING: [&str; 3] = ["MuteBGM", "AutoSave", "AutoSelect"];
pub(crate) const PRESENTATION: [&str; 6] = ["Screen", "Effect", "SysEffect", "Popup", "Dialog", "KeepIcon"];
pub(crate) const INPUT: [&str; 2] = ["CenterKey", "SpaceKey"];
pub(crate) const VOICE_TAGS: [&str; 18] = [
    "LI", "NI", "RR", "MK", "VE", "NO", "UR", "KO", "FO", "AM", "OP", "DE", "RA", "KA", "SY", "MY",
    "BA", "XX",
];

pub(crate) fn enabled(vm: &mut SystemVm, name: &str) -> Result<bool> {
    vm.eval("runtime-option", &format!("return (\"Config\" in getroottable() && \"{name}\" in Config && Config.{name}!=0).tostring()"))?;
    Ok(vm.result() == "true")
}
pub(crate) const ACTIONS: [&str; 11] = [
    "", "AUTO", "SKIP", "LOG", "HIDE", "SAVE", "LOAD", "QSAVE", "QLOAD", "RESET", "QUIT",
];
pub(crate) fn shortcut_action(vm: &mut SystemVm, slot: u64) -> Result<String> {
    if !(1..=12).contains(&slot) {
        return Err("shortcut must be F1..F12".into());
    }
    vm.eval(
        "shortcut-action",
        &format!("return Config.ShortCuts[{}]", slot - 1),
    )?;
    let action = vm.result();
    if action != "CONFIG" && action != "MENU" && !ACTIONS.contains(&action.as_str()) {
        return Err("unknown shortcut action".into());
    }
    Ok(action)
}
pub(crate) fn shortcut_ids(vm: &mut SystemVm) -> Result<Vec<u8>> {
    vm.eval(
        "shortcut-preferences",
        "local s=\"\";for(local i=2;i<12;++i)s+=Config.ShortCuts[i]+\"\\n\";return s",
    )?;
    let result = vm.result();
    let ids = result
        .lines()
        .map(|name| {
            ACTIONS
                .iter()
                .position(|a| *a == name)
                .map(|i| i as u8)
                .ok_or_else(|| "unsupported shortcut action".into())
        })
        .collect::<Result<Vec<_>>>()?;
    if ids.len() != 10 {
        return Err("invalid shortcut preference count".into());
    }
    Ok(ids)
}
#[cfg(test)]
pub(crate) fn set_shortcut(vm: &mut SystemVm, slot: usize, action: usize) -> Result<()> {
    if !(3..=12).contains(&slot) {
        return Err("shortcut slot must be F3..F12".into());
    }
    let name = ACTIONS.get(action).ok_or("unknown shortcut action")?;
    vm.eval(
        "edit-shortcut",
        &format!(
            "Config.ShortCuts[{}]={}",
            slot - 1,
            crate::quote(name.as_bytes())?
        ),
    )?;
    Ok(())
}

#[cfg(test)]
pub(crate) fn set_second_shortcut(vm: &mut SystemVm, action: usize) -> Result<()> {
    let name = ACTIONS.get(action).ok_or("unknown shortcut action")?;
    vm.eval(
        "edit-second-shortcut",
        &format!("Config.ShortCuts[1]={}", crate::quote(name.as_bytes())?),
    )?;
    Ok(())
}
pub(crate) fn values(vm: &mut SystemVm) -> Result<BTreeMap<String, i32>> {
    vm.eval(
        "preferences",
        r#"local s=""; foreach(k,v in Config)if(typeof v=="integer")s+=k+"\t"+v+"\n";
        foreach(k,v in Config.VoiceCheck)s+=k+"\t"+(v?1:0)+"\n";return s"#,
    )?;
    vm.result()
        .lines()
        .map(|l| {
            let (k, v) = l.split_once('\t').ok_or("invalid configuration entry")?;
            Ok((k.into(), v.parse()?))
        })
        .collect()
}
pub(crate) fn maximum(index: usize) -> i32 {
    match index {
        1 | 8 | 9 | 11 => 1,
        7 => 255,
        _ => 100,
    }
}
fn validate(v: &[u8]) -> Result<()> {
    if !matches!(v.len(), 7 | 8 | 10 | 11 | 12)
        || v.iter()
            .enumerate()
            .any(|(i, &v)| i32::from(v) > maximum(i))
    {
        return Err("invalid preference values".into());
    }
    Ok(())
}
pub(crate) fn style(vm: &mut SystemVm) -> Result<crate::text_render::TextStyle> {
    let values = values(vm)?;
    let get = |key: &str| -> Result<u8> {
        Ok(u8::try_from(*values.get(key).ok_or("missing text style")?)?)
    };
    let (opacity, bold, border) = (get("BoxOpacity")?, get("Bold")?, get("Border")?);
    if bold > 1 || border > 1 {
        return Err("invalid text style switch".into());
    }
    Ok(crate::text_render::TextStyle {
        opacity,
        bold: bold == 1,
        border: border == 1,
    })
}
pub(crate) fn export(vm: &mut SystemVm) -> Result<Vec<u8>> {
    let map = values(vm)?;
    let v = FIELDS
        .iter()
        .map(|k| Ok(u8::try_from(*map.get(*k).ok_or("missing preference")?)?))
        .collect::<Result<Vec<_>>>()?;
    validate(&v)?;
    let mut bytes = [MAGIC, v.as_slice(), shortcut_ids(vm)?.as_slice()].concat();
    if let Some(options) = EXTENDED
        .iter()
        .map(|name| map.get(*name).copied())
        .collect::<Option<Vec<_>>>()
    {
        if options.iter().any(|v| !(0..=1).contains(v)) {
            return Err("invalid extended preference values".into());
        }
        bytes[4] = 7;
        bytes.extend(options.into_iter().map(|v| v as u8));
        if READING
            .iter()
            .chain(VOICE_TAGS.iter())
            .all(|name| map.contains_key(*name))
        {
            let second = shortcut_action(vm, 2)?;
            if let Some(id) = ACTIONS.iter().position(|name| *name == second) {
                bytes[4] = 8;
                bytes.push(id as u8);
                for name in READING.iter().chain(VOICE_TAGS.iter()) {
                    bytes.push(u8::from(map[*name] != 0));
                }
            }
        }
    }
    let voices = voice_values(vm)?;
    if INPUT.iter().chain(READING[..1].iter()).chain(PRESENTATION.iter()).any(|name| map.contains_key(*name)) || !voices.is_empty() {
        let mut snapshot = b"NKCF\x0a".to_vec();
        snapshot.push(bytes.len().try_into()?);
        snapshot.extend(bytes);
        for name in INPUT.iter().chain(READING[..1].iter()) {
            snapshot.push(map.get(*name).copied().unwrap_or(0).try_into()?);
        }
        for (name, default) in PRESENTATION.into_iter().zip([0, 1, 1, 1, 1, 0]) {
            snapshot.push(map.get(name).copied().unwrap_or(default).try_into()?);
        }
        for (tag, enabled) in voices {
            if !valid_voice_tag(tag.as_bytes()) {
                return Err("invalid voice preference tag".into());
            }
            snapshot.extend(tag.bytes());
            snapshot.push(enabled as u8);
        }
        bytes = snapshot;
    }
    Ok(bytes)
}

fn valid_voice_tag(tag: &[u8]) -> bool {
    tag.len() == 2 && tag.iter().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
}

pub(crate) fn voice_values(vm: &mut SystemVm) -> Result<BTreeMap<String, i32>> {
    vm.eval("voice-preferences", r#"local s=""; foreach(k,v in Config.VoiceCheck)s+=k+"\t"+(v?1:0)+"\n";return s"#)?;
    vm.result().lines().map(|line| {
        let (tag, value) = line.split_once('\t').ok_or("invalid voice preference")?;
        Ok((tag.to_owned(), value.parse()?))
    }).collect()
}

pub(crate) fn import(vm: &mut SystemVm, bytes: &[u8]) -> Result<()> {
    let (base, input, presentation, voices) = if matches!(bytes.get(..5), Some(b"NKCF\x09" | b"NKCF\x0a")) {
        let payload = &bytes[5..];
        let (&length, body) = payload.split_first().ok_or("missing preference base length")?;
        let (base, tail) = body.split_at_checked(length.into()).ok_or("missing preference base")?;
        let (input, tail) = tail.split_at_checked(3).ok_or("missing input preferences")?;
        let (presentation, voices) = tail.split_at_checked(if bytes[4] == 10 { PRESENTATION.len() } else { 0 }).ok_or("missing presentation preferences")?;
        if input[0] > 2 || input[1] > 2 || input[2] > 1 || presentation.iter().enumerate().any(|(i, v)| *v > if i == 0 { 2 } else { 1 })
            || voices.len() % 3 != 0
            || voices.chunks_exact(3).any(|row| !valid_voice_tag(&row[..2]) || row[2] > 1)
        {
            return Err("invalid input or voice preferences".into());
        }
        (base, input, presentation, voices)
    } else {
        (bytes, &[][..], &[][..], &[][..])
    };
    let (v, shortcuts, extended) = decode(base)?;
    let mut assignments = FIELDS
        .iter()
        .zip(v)
        .map(|(k, v)| format!("Config.{k}={v};"))
        .collect::<String>();
    if let Some(shortcuts) = shortcuts {
        for (i, id) in shortcuts.iter().enumerate() {
            assignments += &format!(
                "Config.ShortCuts[{}]={};",
                i + 2,
                crate::quote(ACTIONS[*id as usize].as_bytes())?
            );
        }
    }
    for (name, value) in EXTENDED.iter().zip(extended) {
        assignments += &format!("if(\"{name}\" in Config){{Config.{name}={value};}}\n");
    }
    if extended.len() > 3 {
        let second = ACTIONS[extended[3] as usize];
        assignments += &format!(
            "Config.ShortCuts[1]={};\n",
            crate::quote(second.as_bytes())?
        );
        for (name, value) in READING.iter().zip(&extended[4..7]) {
            assignments += &format!("if(\"{name}\" in Config){{Config.{name}={value};}}\n");
        }
        for (name, value) in VOICE_TAGS.iter().zip(&extended[7..]) {
            assignments += &format!(
                "if(\"{name}\" in Config.VoiceCheck){{Config.VoiceCheck.{name}={};}}\n",
                *value != 0
            );
        }
    }
    for (name, value) in INPUT.iter().chain(READING[..1].iter()).zip(input).chain(PRESENTATION.iter().zip(presentation)) {
        assignments += &format!("if(\"{name}\" in Config){{Config.{name}={value};}}\n");
    }
    for row in voices.chunks_exact(3) {
        let tag = std::str::from_utf8(&row[..2])?;
        assignments += &format!("if(\"{tag}\" in Config.VoiceCheck){{Config.VoiceCheck[\"{tag}\"]={};}}\n", row[2] != 0);
    }
    vm.eval("restore-preferences", &(assignments + "RestoreVolume()"))?;
    Ok(())
}
fn decode(bytes: &[u8]) -> Result<(&[u8], Option<&[u8]>, &[u8])> {
    let (version, payload) = bytes
        .strip_prefix(b"NKCF")
        .and_then(|v| v.split_first())
        .ok_or("unknown runtime preferences format/version")?;
    let (fields, length) = match version {
        1 => (7, 7),
        2 => (7, 17),
        3 => (8, 18),
        4 => (10, 20),
        5 => (11, 21),
        6 => (12, 22),
        7 => (12, 25),
        8 => (12, 47),
        _ => return Err("unknown runtime preferences format/version".into()),
    };
    if payload.len() != length {
        return Err("invalid preferences length".into());
    }
    let (v, rest) = payload.split_at(fields);
    let (shortcuts, extended) = rest.split_at(rest.len().min(10));
    validate(v)?;
    if shortcuts.iter().any(|id| usize::from(*id) >= ACTIONS.len()) {
        return Err("unknown stored shortcut action".into());
    }
    if extended.iter().enumerate().any(|(i, &v)| {
        if i == 3 {
            usize::from(v) >= ACTIONS.len()
        } else {
            v > 1
        }
    }) {
        return Err("invalid extended preference values".into());
    }
    Ok((
        v,
        if *version == 1 { None } else { Some(shortcuts) },
        extended,
    ))
}
#[cfg(test)]
pub(crate) fn set(vm: &mut SystemVm, index: usize, value: i32) -> Result<()> {
    if index >= FIELDS.len() {
        return Err("unknown preference".into());
    }
    let mut snapshot = export(vm)?;
    let base = if snapshot[4] >= 9 { 6 } else { 0 };
    *snapshot
        .get_mut(base + MAGIC.len() + index)
        .ok_or("unknown preference")? = u8::try_from(value)?;
    import(vm, &snapshot)
}
pub(crate) fn presentation(vm: &mut SystemVm) -> Result<String> {
    let fields = PRESENTATION.map(|name| format!(r#"("\"{name}\":" + Config.{name}.tostring())"#));
    let expressions = PRESENTATION.iter().zip(fields).map(|(name, field)| {
        format!(r#"if("{name}" in Config) parts.push({field});"#)
    }).collect::<Vec<_>>().join("\n");
    vm.eval("presentation-preferences", &format!(r#"local parts=[]; {expressions}
        local result=""; foreach(field in parts) result += ","+field; return result"#))?;
    Ok(vm.result())
}

#[cfg(test)]
pub(crate) fn reset_shortcuts(vm: &mut SystemVm) -> Result<()> {
    vm.eval("reset-shortcuts", "Config.ShortCuts=clone ConfigDefault.ShortCuts")?;
    Ok(())
}

#[cfg(test)]
pub(crate) fn reset(vm: &mut SystemVm, preserve_screen: bool) -> Result<()> {
    let screen = if preserve_screen {
        ("local screen=Config.Screen;", "Config.Screen=screen;")
    } else {
        ("", "")
    };
    vm.eval("reset-preferences", &format!("{} Config=clone ConfigDefault; Config.VoiceCheck=clone ConfigDefault.VoiceCheck; Config.ShortCuts=clone ConfigDefault.ShortCuts; {} RestoreVolume()", screen.0, screen.1))?;
    Ok(())
}

#[cfg(test)]
pub(crate) fn set_extended(vm: &mut SystemVm, name: &'static str, value: i32) -> Result<()> {
    vm.eval("extended-preference", &format!("Config.{name}={value}"))?;
    Ok(())
}

#[cfg(test)]
pub(crate) fn set_voice(vm: &mut SystemVm, tag: &str, value: i32) -> Result<()> {
    let tag = crate::quote(tag.as_bytes())?;
    vm.eval(
        "character-voice",
        &format!("Config.VoiceCheck[{tag}]={}", value != 0),
    )?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn import_is_validated_before_mutation_and_reset_uses_game_defaults() {
        let mut vm = SystemVm::try_new().unwrap();
        vm.eval("setup", "Config <- {TextSpeed=50,KeepVoice=1,VolBGM=50,VolVO=75,VolSE=50,VolENV=50,VolSYS=50,BoxOpacity=191,Bold=0,Border=1,AutoSpeed=50,SkipNoRead=0,VoiceCheck={},ShortCuts=[\"CONFIG\",\"MENU\",\"RESET\",\"QUIT\",\"SAVE\",\"LOAD\",\"QSAVE\",\"QLOAD\",\"LOG\",\"HIDE\",\"AUTO\",\"SKIP\"]}\nConfigDefault <- clone Config\nConfigDefault.ShortCuts = clone Config.ShortCuts\nApplies <- 0\nfunction RestoreVolume(){Applies++}").unwrap();
        let original = export(&mut vm).unwrap();
        assert_eq!(original.len(), 27);
        set_shortcut(&mut vm, 3, 1).unwrap();
        set_shortcut(&mut vm, 12, 0).unwrap();
        let edited = export(&mut vm).unwrap();
        assert_eq!(edited[17], 1);
        assert_eq!(edited[26], 0);
        set(&mut vm, 2, 20).unwrap();
        assert_eq!(shortcut_ids(&mut vm).unwrap()[0], 1);
        assert_eq!(shortcut_ids(&mut vm).unwrap()[9], 0);
        let legacy = [b"NKCF\x01".as_slice(), &original[5..12]].concat();
        import(&mut vm, &legacy).unwrap();
        assert_eq!(export(&mut vm).unwrap(), edited);
        // v2 imports its own shortcuts but preserves the new opacity field.
        let legacy2 = [b"NKCF\x02".as_slice(), &edited[5..12], &edited[17..]].concat();
        set(&mut vm, 7, 127).unwrap();
        import(&mut vm, &legacy2).unwrap();
        assert_eq!(values(&mut vm).unwrap()["BoxOpacity"], 127);
        import(&mut vm, &legacy).unwrap();
        assert_eq!(values(&mut vm).unwrap()["BoxOpacity"], 127);
        assert_eq!(shortcut_ids(&mut vm).unwrap()[9], 0);
        for value in [0, 255, 191] {
            set(&mut vm, 7, value).unwrap();
        }
        assert!(set(&mut vm, 7, 256).is_err());
        assert!(set(&mut vm, 7, -1).is_err());
        assert_eq!(export(&mut vm).unwrap(), edited);
        let legacy3 = [b"NKCF\x03".as_slice(), &edited[5..13], &edited[17..]].concat();
        set(&mut vm, 8, 1).unwrap();
        set(&mut vm, 9, 0).unwrap();
        import(&mut vm, &legacy3).unwrap();
        assert!(style(&mut vm).unwrap().bold);
        assert!(!style(&mut vm).unwrap().border);
        let legacy5 = [b"NKCF\x05".as_slice(), &edited[5..16], &edited[17..]].concat();
        set(&mut vm, 11, 1).unwrap();
        import(&mut vm, &legacy5).unwrap();
        assert_eq!(values(&mut vm).unwrap()["SkipNoRead"], 1);
        set(&mut vm, 11, 0).unwrap();
        let legacy4 = [b"NKCF\x04".as_slice(), &edited[5..15], &edited[17..]].concat();
        set(&mut vm, 10, 73).unwrap();
        import(&mut vm, &legacy4).unwrap();
        assert_eq!(values(&mut vm).unwrap()["AutoSpeed"], 73);
        set(&mut vm, 10, 50).unwrap();
        // Restore style changed above after testing v4 migration.
        set(&mut vm, 8, 1).unwrap();
        set(&mut vm, 9, 0).unwrap();
        let bold = export(&mut vm).unwrap();
        reset(&mut vm, false).unwrap();
        import(&mut vm, &bold).unwrap();
        assert!(style(&mut vm).unwrap().bold);
        assert!(set(&mut vm, 8, 2).is_err());
        assert!(set(&mut vm, 9, 2).is_err());
        set(&mut vm, 8, 0).unwrap();
        set(&mut vm, 9, 1).unwrap();
        let mut corrupt = edited.clone();
        corrupt[26] = 11;
        assert!(import(&mut vm, &corrupt).is_err());
        assert_eq!(export(&mut vm).unwrap(), edited);
        assert!(set_shortcut(&mut vm, 2, 1).is_err());
        assert!(set_shortcut(&mut vm, 13, 1).is_err());
        reset(&mut vm, false).unwrap();
        assert_eq!(export(&mut vm).unwrap(), original);
        vm.eval("reset-test-counter", "Applies=0").unwrap();
        let mut invalid = original.clone();
        invalid[6] = 2;
        assert!(import(&mut vm, &invalid).is_err());
        assert_eq!(export(&mut vm).unwrap(), original);
        vm.eval("count", "return Applies.tostring()").unwrap();
        assert_eq!(vm.result(), "0");
        set(&mut vm, 0, 0).unwrap();
        set(&mut vm, 1, 0).unwrap();
        set(&mut vm, 2, 23).unwrap();
        let changed = export(&mut vm).unwrap();
        assert_eq!(&changed[5..8], &[0, 0, 23]);
        reset(&mut vm, false).unwrap();
        assert_eq!(export(&mut vm).unwrap(), original);
        import(&mut vm, &changed).unwrap();
        assert_eq!(export(&mut vm).unwrap(), changed);

        vm.eval("extended-defaults", "ConfigDefault.KeepSkip <- 0; ConfigDefault.KeepAuto <- 0; ConfigDefault.Movie <- 1; Config = clone ConfigDefault;").unwrap();
        reset(&mut vm, false).unwrap();
        assert_eq!(&export(&mut vm).unwrap()[27..], &[0, 0, 1]);
        for (name, value) in EXTENDED.into_iter().zip([1, 1, 0]) {
            set_extended(&mut vm, name, value).unwrap();
        }
        let edited = export(&mut vm).unwrap();
        assert_eq!(&edited[..5], b"NKCF\x07");
        reset(&mut vm, false).unwrap();
        import(&mut vm, &edited).unwrap();
        assert_eq!(export(&mut vm).unwrap(), edited);
        vm.eval(
            "input-and-voice-defaults",
            "Config.CenterKey <- 0; Config.SpaceKey <- 0; Config.MuteBGM <- 1; Config.VoiceCheck.RU <- true; ConfigDefault=clone Config; ConfigDefault.VoiceCheck=clone Config.VoiceCheck;",
        ).unwrap();
        let input_defaults = export(&mut vm).unwrap();
        vm.eval("edit-input", "Config.CenterKey=2; Config.SpaceKey=1; Config.MuteBGM=0;").unwrap();
        set_voice(&mut vm, "RU", 0).unwrap();
        set(&mut vm, 2, 32).unwrap();
        let changed = export(&mut vm).unwrap();
        let mut bad = changed.clone();
        *bad.last_mut().unwrap() = 2;
        assert!(import(&mut vm, &bad).is_err());
        assert_eq!(export(&mut vm).unwrap(), changed);
        reset(&mut vm, false).unwrap();
        assert_eq!(export(&mut vm).unwrap(), input_defaults);
        import(&mut vm, &changed).unwrap();
        assert_eq!(export(&mut vm).unwrap(), changed);
        import(&mut vm, &input_defaults).unwrap();
        vm.eval("presentation-defaults", "Config.Screen <- 0; Config.Effect <- 1; Config.SysEffect <- 1; Config.Popup <- 1; Config.Dialog <- 1; Config.KeepIcon <- 1; ConfigDefault=clone Config; ConfigDefault.ShortCuts=clone Config.ShortCuts").unwrap();
        let defaults = export(&mut vm).unwrap();
        let mut previous = defaults.clone();
        previous[4] = 9;
        let start = 6 + usize::from(previous[5]) + 3;
        previous.drain(start..start + PRESENTATION.len());
        set_extended(&mut vm, "Screen", 2).unwrap();
        set_extended(&mut vm, "Popup", 0).unwrap();
        set_shortcut(&mut vm, 3, 1).unwrap();
        import(&mut vm, &previous).unwrap();
        assert_eq!(values(&mut vm).unwrap()["Screen"], 2);
        assert_eq!(values(&mut vm).unwrap()["Popup"], 0);
        set_shortcut(&mut vm, 3, 1).unwrap();
        reset_shortcuts(&mut vm).unwrap();
        assert_eq!(shortcut_ids(&mut vm).unwrap()[0], defaults[23]);
        assert_eq!(values(&mut vm).unwrap()["Popup"], 0);
        reset(&mut vm, true).unwrap();
        assert_eq!(values(&mut vm).unwrap()["Screen"], 2);
        assert_eq!(values(&mut vm).unwrap()["Popup"], 1);
        import(&mut vm, &defaults).unwrap();
        assert_eq!(export(&mut vm).unwrap(), defaults);
    }
    #[test]
    fn version_length_and_ranges_are_strict() {
        let valid = [
            MAGIC,
            &[50, 1, 50, 75, 50, 50, 50, 191, 0, 1, 50, 0],
            &[0; 10],
        ]
        .concat();
        assert!(decode(&valid).is_ok());
        for n in 0..valid.len() {
            assert!(decode(&valid[..n]).is_err())
        }
        for (version, payload) in [(1, &valid[5..12]), (2, &valid[5..22])] {
            let legacy = [b"NKCF".as_slice(), &[version], payload].concat();
            // v2 uses seven values plus ten IDs, not the v3 opacity byte.
            let legacy = if version == 2 {
                [b"NKCF\x02".as_slice(), &valid[5..12], &[0; 10]].concat()
            } else {
                legacy
            };
            assert!(decode(&legacy).is_ok());
            for length in 0..legacy.len() {
                assert!(decode(&legacy[..length]).is_err());
            }
            let mut overlong = legacy.clone();
            overlong.push(0);
            assert!(decode(&overlong).is_err());
            if version == 2 {
                let mut bad = legacy;
                bad[21] = 11;
                assert!(decode(&bad).is_err());
            }
        }
        let mut bad = valid.clone();
        bad.push(0);
        assert!(decode(&bad).is_err());
        for (i, value) in [
            (4, 7),
            (5, 101),
            (6, 2),
            (7, 101),
            (13, 2),
            (14, 2),
            (15, 101),
            (16, 2),
            (17, 11),
        ] {
            let mut bad = valid.clone();
            bad[i] = value;
            assert!(decode(&bad).is_err())
        }
    }
}
