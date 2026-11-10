//! Portable, bounded runtime preferences. Not the original engine's config.sav.
use nekopack_script::system::SystemVm;
use std::{collections::BTreeMap, error::Error};
pub(crate) type Result<T> = std::result::Result<T, Box<dyn Error>>;
pub(crate) const FIELDS: [&str; 8] = [
    "TextSpeed",
    "KeepVoice",
    "VolBGM",
    "VolVO",
    "VolSE",
    "VolENV",
    "VolSYS",
    "BoxOpacity",
];
const MAGIC: &[u8] = b"NKCF\x03";
pub(crate) const ACTIONS: [&str; 11] = [
    "", "AUTO", "SKIP", "LOG", "HIDE", "SAVE", "LOAD", "QSAVE", "QLOAD", "RESET", "QUIT",
];
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
        1 => 1,
        7 => 255,
        _ => 100,
    }
}
fn validate(v: &[u8]) -> Result<()> {
    if !(7..=FIELDS.len()).contains(&v.len())
        || v.iter()
            .enumerate()
            .any(|(i, &v)| i32::from(v) > maximum(i))
    {
        return Err("invalid preference values".into());
    }
    Ok(())
}
pub(crate) fn export(vm: &mut SystemVm) -> Result<Vec<u8>> {
    let map = values(vm)?;
    let v = FIELDS
        .iter()
        .map(|k| Ok(u8::try_from(*map.get(*k).ok_or("missing preference")?)?))
        .collect::<Result<Vec<_>>>()?;
    validate(&v)?;
    Ok([MAGIC, v.as_slice(), shortcut_ids(vm)?.as_slice()].concat())
}
pub(crate) fn import(vm: &mut SystemVm, bytes: &[u8]) -> Result<()> {
    let (v, shortcuts) = decode(bytes)?;
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
    vm.eval("restore-preferences", &(assignments + "RestoreVolume()"))?;
    Ok(())
}
fn decode(bytes: &[u8]) -> Result<(&[u8], Option<&[u8]>)> {
    let (version, payload) = bytes
        .strip_prefix(b"NKCF")
        .and_then(|v| v.split_first())
        .ok_or("unknown runtime preferences format/version")?;
    let (fields, length) = match version {
        1 => (7, 7),
        2 => (7, 17),
        3 => (8, 18),
        _ => return Err("unknown runtime preferences format/version".into()),
    };
    if payload.len() != length {
        return Err("invalid preferences length".into());
    }
    let (v, shortcuts) = payload.split_at(fields);
    validate(v)?;
    if shortcuts.iter().any(|id| usize::from(*id) >= ACTIONS.len()) {
        return Err("unknown stored shortcut action".into());
    }
    Ok((v, if *version == 1 { None } else { Some(shortcuts) }))
}
pub(crate) fn set(vm: &mut SystemVm, index: usize, value: i32) -> Result<()> {
    if index >= FIELDS.len() {
        return Err("unknown preference".into());
    }
    let mut snapshot = export(vm)?;
    *snapshot
        .get_mut(MAGIC.len() + index)
        .ok_or("unknown preference")? = u8::try_from(value)?;
    import(vm, &snapshot)
}
pub(crate) fn reset(vm: &mut SystemVm) -> Result<()> {
    let mut code = FIELDS
        .iter()
        .map(|k| format!("Config.{k}=ConfigDefault.{k};"))
        .collect::<String>();
    code += "for(local i=2;i<12;++i)Config.ShortCuts[i]=ConfigDefault.ShortCuts[i];";
    vm.eval("reset-supported-preferences", &(code + "RestoreVolume()"))?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn import_is_validated_before_mutation_and_reset_uses_game_defaults() {
        let mut vm = SystemVm::try_new().unwrap();
        vm.eval("setup", "Config <- {TextSpeed=50,KeepVoice=1,VolBGM=50,VolVO=75,VolSE=50,VolENV=50,VolSYS=50,BoxOpacity=191,VoiceCheck={},ShortCuts=[\"CONFIG\",\"MENU\",\"RESET\",\"QUIT\",\"SAVE\",\"LOAD\",\"QSAVE\",\"QLOAD\",\"LOG\",\"HIDE\",\"AUTO\",\"SKIP\"]}\nConfigDefault <- clone Config\nConfigDefault.ShortCuts = clone Config.ShortCuts\nApplies <- 0\nfunction RestoreVolume(){Applies++}").unwrap();
        let original = export(&mut vm).unwrap();
        assert_eq!(original.len(), 23);
        set_shortcut(&mut vm, 3, 1).unwrap();
        set_shortcut(&mut vm, 12, 0).unwrap();
        let edited = export(&mut vm).unwrap();
        assert_eq!(edited[13], 1);
        assert_eq!(edited[22], 0);
        set(&mut vm, 2, 20).unwrap();
        assert_eq!(shortcut_ids(&mut vm).unwrap()[0], 1);
        assert_eq!(shortcut_ids(&mut vm).unwrap()[9], 0);
        let legacy = [b"NKCF\x01".as_slice(), &original[5..12]].concat();
        import(&mut vm, &legacy).unwrap();
        assert_eq!(export(&mut vm).unwrap(), edited);
        // v2 imports its own shortcuts but preserves the new opacity field.
        let legacy2 = [b"NKCF\x02".as_slice(), &edited[5..12], &edited[13..]].concat();
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
        let mut corrupt = edited.clone();
        corrupt[22] = 11;
        assert!(import(&mut vm, &corrupt).is_err());
        assert_eq!(export(&mut vm).unwrap(), edited);
        assert!(set_shortcut(&mut vm, 2, 1).is_err());
        assert!(set_shortcut(&mut vm, 13, 1).is_err());
        reset(&mut vm).unwrap();
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
        reset(&mut vm).unwrap();
        assert_eq!(export(&mut vm).unwrap(), original);
        import(&mut vm, &changed).unwrap();
        assert_eq!(export(&mut vm).unwrap(), changed);
    }
    #[test]
    fn version_length_and_ranges_are_strict() {
        let valid = [MAGIC, &[50, 1, 50, 75, 50, 50, 50, 191], &[0; 10]].concat();
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
        for (i, value) in [(4, 4), (5, 101), (6, 2), (7, 101), (13, 11)] {
            let mut bad = valid.clone();
            bad[i] = value;
            assert!(decode(&bad).is_err())
        }
    }
}
