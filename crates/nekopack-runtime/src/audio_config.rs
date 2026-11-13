//! Original RestoreVolume/MuteBGM contracts, selected by configuration evidence.
use nekopack_script::system::SystemVm;
use std::{collections::BTreeMap, error::Error};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const BASE: [(&str, &str); 5] = [
    ("bgm", "BGM"),
    ("voice", "VO"),
    ("se", "SE"),
    ("env", "ENV"),
    ("system", "SYS"),
];
const CATEGORIES: [&str; 8] = ["MAS", "BGM", "VO", "SE", "ENV", "BGV", "SYS", "MV"];

const SNAPSHOT_SOURCE: &str = r#"
            local result="";
            foreach(key,value in Config){
                if(typeof key=="string" && key.len()>=3 &&
                    (key.slice(0,3)=="Vol" || (key.len()>=4 && key.slice(0,4)=="Mute"))){
                    if(typeof value!="integer")throw "invalid audio configuration type";
                    result+=key+"="+value.tostring()+"\n";
                }
            }
            return result;
        "#;

pub(crate) fn install_snapshotter(vm: &mut SystemVm) -> Result<()> {
    vm.eval(
        "audio-snapshot-recorder",
        &format!("function NativeAudioConfigSnapshot(){{{SNAPSHOT_SOURCE}}}"),
    )?;
    Ok(())
}
pub(crate) fn snapshot_category(snapshot: &str, field: &str) -> Result<u32> {
    Config::parse(snapshot)?.category(field)
}

struct Config {
    values: BTreeMap<String, i32>,
    modern: bool,
}
impl Config {
    fn read(vm: &mut SystemVm) -> Result<Self> {
        vm.eval("audio-config-fields", SNAPSHOT_SOURCE)?;
        Self::parse(&vm.result())
    }
    fn parse(snapshot: &str) -> Result<Self> {
        let mut values = BTreeMap::new();
        for line in snapshot.lines() {
            let (key, value) = line
                .split_once('=')
                .ok_or("invalid audio configuration record")?;
            if !CATEGORIES
                .iter()
                .any(|category| key == format!("Vol{category}") || key == format!("Mute{category}"))
            {
                return Err(format!("unknown audio configuration field {key}").into());
            }
            let value: i32 = value.parse()?;
            if key.starts_with("Vol") && !(0..=100).contains(&value) {
                return Err(format!("invalid Config.{key}: {value}").into());
            }
            values.insert(key.to_owned(), value);
        }
        let modern = values.keys().any(|key| {
            key.starts_with("Mute") && key != "MuteBGM"
                || matches!(key.as_str(), "VolBGV" | "VolMV")
        });
        let config = Self { values, modern };
        for (_, category) in BASE {
            config.value(&format!("Vol{category}"))?;
        }
        if modern {
            for category in CATEGORIES {
                config.value(&format!("Vol{category}"))?;
                config.value(&format!("Mute{category}"))?;
            }
        }
        Ok(config)
    }
    fn value(&self, field: &str) -> Result<i32> {
        self.values
            .get(field)
            .copied()
            .ok_or_else(|| format!("missing Config.{field}").into())
    }
    fn category(&self, field: &str) -> Result<u32> {
        let suffix = field.strip_prefix("Vol").ok_or("unknown audio category")?;
        if !CATEGORIES.contains(&suffix) || suffix == "MAS" {
            return Err("unknown audio category".into());
        }
        let volume = self.value(field)?;
        if !self.modern {
            return Ok(volume as u32);
        }
        let master = if self.value("MuteMAS")? != 0 {
            0
        } else {
            self.value("VolMAS")?
        };
        Ok(if self.value(&format!("Mute{suffix}"))? != 0 {
            0
        } else {
            (volume * master / 100) as u32
        })
    }
}

pub(crate) fn category(vm: &mut SystemVm, field: &str) -> Result<u32> {
    Config::read(vm)?.category(field)
}

pub(crate) fn packet(vm: &mut SystemVm) -> Result<Vec<u8>> {
    let config = Config::read(vm)?;
    let mut fields = Vec::new();
    for (key, suffix) in BASE {
        fields.push(format!(
            "\"{key}\":{}",
            config.category(&format!("Vol{suffix}"))?
        ));
    }
    if config.modern {
        fields.push(format!("\"bgv\":{}", config.category("VolBGV")?));
        fields.push(format!(
            "\"voice_bgm_percent\":{}",
            config.category("VolMV")?
        ));
        fields.push("\"voice_duck\":false".into());
    } else if let Some(value) = config.values.get("MuteBGM") {
        fields.push(format!("\"voice_duck\":{}", *value != 0));
    }
    Ok(format!("{{{}}}", fields.join(",")).into_bytes())
}

// Retain newlines for statement matching, while comments and literals cannot
// manufacture a MuteBGM call. Both Squirrel comment forms are supported.
fn executable_source(source: &str) -> String {
    let bytes = source.as_bytes();
    let mut out = String::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i..].starts_with(b"//") {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
        } else if bytes[i..].starts_with(b"/*") {
            i += 2;
            while i < bytes.len() && !bytes[i..].starts_with(b"*/") {
                if bytes[i] == b'\n' {
                    out.push('\n');
                }
                i += 1;
            }
            i = (i + 2).min(bytes.len());
            out.push(' ');
        } else if matches!(bytes[i], b'"' | b'\'') {
            let quote = bytes[i];
            i += 1;
            while i < bytes.len() && bytes[i] != quote {
                if bytes[i] == b'\\' {
                    i += 1;
                }
                i += 1;
            }
            i = (i + 1).min(bytes.len());
            out.push(' ');
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    out
}

pub(crate) fn validate_source(vm: &mut SystemVm, message_source: &str) -> Result<()> {
    let config = Config::read(vm)?;
    let source = executable_source(message_source);
    let compact: String = source.chars().filter(|c| !c.is_whitespace()).collect();
    let conditional = compact.contains("if(Config.MuteBGM)MuteBGM()")
        || compact.contains("if(Config.MuteBGM){MuteBGM()");
    let unconditional = source.lines().any(|line| {
        let line: String = line.chars().filter(|c| !c.is_whitespace()).collect();
        line.starts_with("MuteBGM()")
    });
    if config.modern && !conditional && unconditional {
        return Ok(());
    }
    // Older native MuteBGM implementations also gate inside the function, so
    // their message loop may call it unconditionally. Configuration family,
    // rather than placement of that gate, distinguishes the two contracts.
    if !config.modern && (conditional || unconditional) && config.values.contains_key("MuteBGM") {
        return Ok(());
    }
    Err("unknown or inconsistent original audio configuration contract".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    const OLD: &str = "Config <- {VolBGM=75,VolVO=80,VolSE=60,VolENV=40,VolSYS=20,MuteBGM=1}";
    const NEW: &str = "Config <- {VolMAS=67,MuteMAS=0,VolBGM=75,MuteBGM=0,VolVO=80,MuteVO=0,VolSE=60,MuteSE=0,VolENV=40,MuteENV=0,VolBGV=30,MuteBGV=0,VolSYS=20,MuteSYS=0,VolMV=50,MuteMV=0}";
    fn vm(source: &str) -> SystemVm {
        let mut vm = SystemVm::new();
        vm.eval("config", source).unwrap();
        vm
    }
    #[test]
    fn modern_master_mutes_and_integer_truncation() {
        let mut vm = vm(NEW);
        assert_eq!(category(&mut vm, "VolBGM").unwrap(), 50);
        assert_eq!(category(&mut vm, "VolSYS").unwrap(), 13);
        let packet = String::from_utf8(packet(&mut vm).unwrap()).unwrap();
        assert!(packet.contains("\"voice_bgm_percent\":33"));
        assert!(packet.contains("\"bgv\":20"));
        assert!(packet.contains("\"voice_duck\":false"));
        vm.eval("mute", "Config.MuteVO=1;Config.MuteMV=1").unwrap();
        assert_eq!(category(&mut vm, "VolVO").unwrap(), 0);
        assert_eq!(category(&mut vm, "VolMV").unwrap(), 0);
        assert_eq!(category(&mut vm, "VolBGM").unwrap(), 50);
        vm.eval("master", "Config.MuteMAS=-1").unwrap();
        for suffix in ["BGM", "VO", "SE", "ENV", "BGV", "SYS", "MV"] {
            assert_eq!(category(&mut vm, &format!("Vol{suffix}")).unwrap(), 0);
        }
    }
    #[test]
    fn legacy_master_is_ignored_and_switch_is_preserved() {
        let mut vm = vm(OLD);
        vm.eval("master", "Config.VolMAS <- 1").unwrap();
        assert_eq!(category(&mut vm, "VolBGM").unwrap(), 75);
        assert!(
            String::from_utf8(packet(&mut vm).unwrap())
                .unwrap()
                .contains("\"voice_duck\":true")
        );
        vm.eval("minimal", "delete Config.MuteBGM").unwrap();
        assert!(
            !String::from_utf8(packet(&mut vm).unwrap())
                .unwrap()
                .contains("voice_duck")
        );
    }
    #[test]
    fn partial_unknown_and_invalid_configuration_is_rejected() {
        for change in [
            "Config.VolBGV <- 50",
            "Config.MuteSYS <- 0",
            "Config.VolMystery <- 50",
            "Config.MuteMystery <- 1",
            "Config.VolSE=101",
            "Config.VolSE=-1",
            "Config.VolSE=50.0",
            "Config.MuteBGM=true",
        ] {
            let mut vm = vm(OLD);
            vm.eval("bad", change).unwrap();
            assert!(packet(&mut vm).is_err(), "{change}");
        }
        assert!(category(&mut vm(NEW), "VolUnknown").is_err());
        assert!(category(&mut vm(NEW), "VolMAS").is_err());
    }
    #[test]
    fn original_message_calls_validate_abi_without_comments_or_literals() {
        let old = "if (Config.MuteBGM) MuteBGM()\nMuteBGM()";
        let new = "// if(Config.MuteBGM)MuteBGM()\n/* old call */\nMuteBGM()";
        validate_source(&mut vm(OLD), old).unwrap();
        validate_source(&mut vm(NEW), new).unwrap();
        validate_source(&mut vm(OLD), new).unwrap();
        assert!(validate_source(&mut vm(NEW), old).is_err());
        assert!(validate_source(&mut vm(NEW), "// MuteBGM()\nlocal x=\"MuteBGM()\"").is_err());
        assert!(validate_source(&mut vm(OLD), "/* if(Config.MuteBGM)MuteBGM() */").is_err());
    }
    #[test]
    fn legacy_contract_accepts_native_internal_gate_with_unconditional_message_calls() {
        let source = "function MessageLoop(){\nwhile(wait(0)){\nMuteBGM()\n}\n}";
        let mut old = vm(OLD);
        old.eval("old-master", "Config.VolMAS <- 50").unwrap();
        validate_source(&mut old, source).unwrap();
        assert_eq!(category(&mut old, "VolBGM").unwrap(), 75);
        old.eval("missing-switch", "delete Config.MuteBGM").unwrap();
        assert!(validate_source(&mut old, source).is_err());
        assert!(validate_source(&mut vm(OLD), "function MessageLoop(){\nwait(0)\n}").is_err());
        let mut partial = vm(OLD);
        partial
            .eval("partial-modern", "Config.MuteMAS <- 0")
            .unwrap();
        assert!(validate_source(&mut partial, source).is_err());
    }
}
