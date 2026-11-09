//! Portable, bounded runtime preferences. Not the original engine's config.sav.
use nekopack_script::system::SystemVm;
use std::{collections::BTreeMap, error::Error};
pub(crate) type Result<T> = std::result::Result<T, Box<dyn Error>>;
pub(crate) const FIELDS: [&str; 7] = [
    "TextSpeed",
    "KeepVoice",
    "VolBGM",
    "VolVO",
    "VolSE",
    "VolENV",
    "VolSYS",
];
const MAGIC: &[u8] = b"NKCF\x01";
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
fn validate(v: &[u8]) -> Result<()> {
    if v.len() != FIELDS.len()
        || v.iter()
            .enumerate()
            .any(|(i, &v)| v > if i == 1 { 1 } else { 100 })
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
    Ok([MAGIC, v.as_slice()].concat())
}
pub(crate) fn import(vm: &mut SystemVm, bytes: &[u8]) -> Result<()> {
    let v = decode(bytes)?;
    let assignments = FIELDS
        .iter()
        .zip(v)
        .map(|(k, v)| format!("Config.{k}={v};"))
        .collect::<String>();
    vm.eval("restore-preferences", &(assignments + "RestoreVolume()"))?;
    Ok(())
}
fn decode(bytes: &[u8]) -> Result<&[u8]> {
    let v = bytes
        .strip_prefix(MAGIC)
        .ok_or("unknown runtime preferences format/version")?;
    validate(v)?;
    Ok(v)
}
pub(crate) fn set(vm: &mut SystemVm, index: usize, value: i32) -> Result<()> {
    let mut snapshot = export(vm)?;
    *snapshot
        .get_mut(MAGIC.len() + index)
        .ok_or("unknown preference")? = u8::try_from(value)?;
    import(vm, &snapshot)
}
pub(crate) fn reset(vm: &mut SystemVm) -> Result<()> {
    let code = FIELDS
        .iter()
        .map(|k| format!("Config.{k}=ConfigDefault.{k};"))
        .collect::<String>();
    vm.eval("reset-supported-preferences", &(code + "RestoreVolume()"))?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn import_is_validated_before_mutation_and_reset_uses_game_defaults() {
        let mut vm = SystemVm::try_new().unwrap();
        vm.eval("setup", "Config <- {TextSpeed=50,KeepVoice=1,VolBGM=50,VolVO=75,VolSE=50,VolENV=50,VolSYS=50,VoiceCheck={}}\nConfigDefault <- clone Config\nApplies <- 0\nfunction RestoreVolume(){Applies++}").unwrap();
        let original = export(&mut vm).unwrap();
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
        let valid = [MAGIC, &[50, 1, 50, 75, 50, 50, 50]].concat();
        assert!(decode(&valid).is_ok());
        for n in 0..valid.len() {
            assert!(decode(&valid[..n]).is_err())
        }
        let mut bad = valid.clone();
        bad.push(0);
        assert!(decode(&bad).is_err());
        for (i, value) in [(4, 2), (5, 101), (6, 2), (7, 101)] {
            let mut bad = valid.clone();
            bad[i] = value;
            assert!(decode(&bad).is_err())
        }
    }
}
