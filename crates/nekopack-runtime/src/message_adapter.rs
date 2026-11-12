//! Reuse the mounted message handler's own local parser and voice predicate.
//! Presentation remains native; parameter semantics remain owned by system code.

pub(crate) fn voice_adapter(source: &str) -> Result<String, String> {
    const HANDLER: &str = "com[\"\"] = function(args){";
    const PARSED: &str = "local is_readed =";
    const VOICE: &str = "local voice = IsAvailVoice(";
    fn unique<'a>(source: &'a str, marker: &str) -> Result<usize, String> {
        let mut offsets = source.match_indices(marker).map(|(offset, _)| offset);
        let first = offsets
            .next()
            .ok_or_else(|| format!("message adapter missing {marker}"))?;
        if offsets.next().is_some() {
            return Err(format!("message adapter ambiguous {marker}"));
        }
        Ok(first)
    }
    let start = unique(source, HANDLER)? + HANDLER.len();
    let parsed = unique(source, PARSED)?;
    let voice = unique(source, VOICE)?;
    if !(start < parsed && parsed < voice) {
        return Err("message adapter boundaries are out of order".into());
    }
    // In the verified source contract this prefix only declares m and parses
    // args into local fields; MessageLog/global/UI writes follow PARSED.
    let parser = &source[start..parsed];
    let predicate = source[voice..].lines().next().unwrap();
    if !predicate
        .trim()
        .trim_end_matches(';')
        .trim_end()
        .ends_with(')')
    {
        return Err("message adapter voice declaration is not a complete statement".into());
    }
    Ok(format!(
        "function NativeMessageVoice(args){{\n{parser}\n{predicate}\nreturn voice\n}}"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use nekopack_script::system::SystemVm;

    fn fixture(field: &str) -> String {
        format!(
            r#"com[""] = function(args){{
local m = {{name="",voice="",mask="",text=""}}
foreach(line in args){{
    if(line[0]==':'){{
        local p=split(line,":")
        p.append("");p.append("");p.append("")
        m.name=p[0];m.voice=p[1];m.mask=p[2]
    }}else{{m.text+=line}}
}}
local is_readed = IsReaded()
local voice = IsAvailVoice(m.{field})
Presentation()
}}"#
        )
    }

    #[test]
    fn original_parser_and_predicate_choose_their_own_input() {
        for (field, expected) in [("voice", false), ("mask", true)] {
            let mut vm = SystemVm::new();
            vm.eval(
                "config",
                r#"
Config <- {VoiceCheck={AA=false,BB=true,CC=false,XX=false}}
function IsAvailVoice(input){
    foreach(tag in split(input," \t")){
        if(!(tag in Config.VoiceCheck))tag="XX"
        if(Config.VoiceCheck[tag])return true
    }
    return false
}"#,
            )
            .unwrap();
            vm.eval("adapter", &voice_adapter(&fixture(field)).unwrap())
                .unwrap();
            vm.eval(
                "speaker",
                r#"return NativeMessageVoice([":Speaker:AA:CC BB"]).tostring()"#,
            )
            .unwrap();
            assert_eq!(vm.result(), expected.to_string());
        }
    }

    #[test]
    fn malformed_or_duplicate_contract_is_rejected() {
        assert!(voice_adapter("").is_err());
        let source = fixture("voice");
        assert!(voice_adapter(&format!("{source}\n{source}")).is_err());
        assert!(
            voice_adapter(&source.replace("IsAvailVoice(m.voice)", "IsAvailVoice(m.voice"))
                .is_err()
        );
    }

    #[test]
    #[ignore = "requires five original game directories in NEKO_VOICE_CODE_TEST_DIRS; system and boot code only"]
    fn five_original_message_parsers_execute_their_own_voice_rules() {
        use crate::{GameResources, dat::DatGame};
        let paths = std::env::var_os("NEKO_VOICE_CODE_TEST_DIRS").unwrap();
        let paths: Vec<_> = std::env::split_paths(&paths).collect();
        assert_eq!(paths.len(), 5);
        let mut name_profiles = 0;
        let mut mask_profiles = 0;
        for root in paths {
            let mut store = DatGame::mount_named(
                std::fs::read_dir(&root)
                    .unwrap()
                    .map(|entry| entry.unwrap().path())
                    .filter(|path| {
                        matches!(
                            path.file_name().and_then(|name| name.to_str()),
                            Some("system.dat" | "script.dat")
                        )
                    })
                    .map(|path| {
                        (
                            path.file_name().unwrap().to_str().unwrap().to_owned(),
                            std::fs::File::open(path).unwrap(),
                        )
                    }),
                true,
            )
            .unwrap();
            let source = String::from_utf8(store.system(b"message.nut").unwrap()).unwrap();
            let (_, mut vm) =
                crate::bootstrap_begin(&mut store, 1, &mut std::io::sink(), true).unwrap();
            // These are source-contract assertions for the two verified original
            // variants, not production profile selection or duplicated filters.
            let mask_input = source.contains("local voice = IsAvailVoice(m.mask)");
            if mask_input {
                mask_profiles += 1;
            } else {
                assert!(source.contains("local voice = IsAvailVoice(m.voice)"));
                name_profiles += 1;
            }
            for (xx, ve) in [(false, false), (false, true), (true, false), (true, true)] {
                vm.eval("voice-controls", &format!("foreach(tag,value in Config.VoiceCheck)Config.VoiceCheck[tag]=false; Config.VoiceCheck.XX={xx}; Config.VoiceCheck.VE={ve};")).unwrap();
                vm.eval(
                    "synthetic-speaker",
                    r#"return NativeMessageVoice([":Synthetic:XX_unit:VE"]).tostring()"#,
                )
                .unwrap();
                assert_eq!(
                    vm.result(),
                    (if mask_input { ve } else { xx }).to_string(),
                    "{}",
                    root.display()
                );
            }
            // ED's mask handler must retain its any-enabled-tag behavior, and
            // the name handler must continue ignoring mask contents.
            vm.eval("multi-mask", r#"Config.VoiceCheck.VE=true;Config.VoiceCheck.XX=false;return NativeMessageVoice([":Synthetic:XX_unit:UNKNOWN VE"]).tostring()"#).unwrap();
            assert_eq!(vm.result(), mask_input.to_string());
        }
        assert_eq!((name_profiles, mask_profiles), (4, 1));
    }
}
