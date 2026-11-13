//! BgvCommand's token compiler and voice suppression data, read from its PE ABI.
use super::{dword, Pe};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BgvChannel {
    pub channel: u8,
    pub tag: String,
    pub voice_suppression_substring: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BgvProfile {
    pub channels: [BgvChannel; 2],
    pub name_prefix: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BgvResolved {
    /// Zero stops both BGV slots; 2 and 3 select their respective slots.
    pub channel: u8,
    pub tag: String,
    pub name: String,
}

impl BgvProfile {
    pub fn parse<T: AsRef<[u8]>>(&self, args: &[T]) -> Result<BgvResolved, String> {
        let mut result = BgvResolved::default();
        for token in args {
            let token = std::str::from_utf8(token.as_ref())
                .map_err(|e| format!("BGV token is not UTF-8: {e}"))?;
            if let Some(channel) = self
                .channels
                .iter()
                .find(|channel| channel.tag.eq_ignore_ascii_case(token))
            {
                result.channel = channel.channel;
                result.tag = token.into();
            } else {
                result.name = format!("{}{}{token}", result.tag, self.name_prefix);
            }
        }
        Ok(result)
    }
}

// These signatures describe one verified Borland instruction layout. Absolute
// addresses and relative CALL operands are read, never used as game identifiers.
// A different compiler layout requires evidence and an additional ABI decoder.
fn expect<'a>(pe: &Pe<'a>, va: u32, signature: &str) -> Result<&'a [u8], String> {
    let pattern: Vec<_> = signature
        .split_ascii_whitespace()
        .map(|token| (token != "??").then(|| u8::from_str_radix(token, 16).unwrap()))
        .collect();
    let code = pe
        .data(va, pattern.len(), true)
        .ok_or("BGV instruction outside executable section")?;
    if !pattern
        .iter()
        .zip(code)
        .all(|(a, b)| a.is_none_or(|a| a == *b))
    {
        return Err(format!(
            "unrecognized BgvCommand instruction layout at {va:#x}"
        ));
    }
    Ok(code)
}

fn ascii_string(pe: &Pe<'_>, va: u32) -> Result<String, String> {
    let mut bytes = Vec::new();
    for i in 0..64 {
        let address = va.checked_add(i).ok_or("BGV string address overflow")?;
        let byte = *pe
            .data(address, 1, false)
            .and_then(|b| b.first())
            .ok_or("BGV string outside PE section")?;
        if byte == 0 {
            if bytes.is_empty() {
                return Err("empty BGV ABI string".into());
            }
            return String::from_utf8(bytes).map_err(|e| e.to_string());
        }
        if !byte.is_ascii_graphic() {
            return Err("unsupported non-ASCII BGV ABI token".into());
        }
        bytes.push(byte);
    }
    Err("unterminated BGV ABI string".into())
}

fn call(pe: &Pe<'_>, va: u32) -> Result<u32, String> {
    let target = pe.call_target(va).ok_or("invalid BGV ABI call")?;
    pe.data(target, 1, true)
        .ok_or("BGV ABI call outside executable section")?;
    Ok(target)
}

fn compare_ascii(pe: &Pe<'_>, va: u32) -> Result<(), String> {
    let code = expect(pe, va,
        "55 8b ec 83 c4 f4 a1 ?? ?? ?? ?? 53 56 57 83 78 08 00 8b 7d 08 8b 75 0c 74 3f eb 02 47 46 0f be 0f 0f be 16 81 e1 ff 00 00 00 81 e2 ff 00 00 00 8a 92 ?? ?? ?? ?? 8a 81 ?? ?? ?? ?? 3a c2 75 04 84 c0 75 d8")?;
    let table = dword(code, 50)?;
    if dword(code, 56)? != table {
        return Err("BGV case comparison uses different mapping tables".into());
    }
    let map = pe.data(table, 128, false).ok_or("missing BGV case map")?;
    if !(0..128).all(|i| map[i] == (i as u8).to_ascii_uppercase())
        && !(0..128).all(|i| map[i] == (i as u8).to_ascii_lowercase())
    {
        return Err("unrecognized BGV ASCII case mapping".into());
    }
    Ok(())
}

fn compiler(pe: &Pe<'_>, parse: u32, execute: u32) -> Result<([String; 2], String), String> {
    if execute.checked_sub(parse) != Some(0x6d40) {
        return Err("unrecognized BgvCommand compiler layout".into());
    }
    expect(
        pe,
        parse + 0x16,
        "33 d2 8d 8d 80 fc ff ff 89 95 88 fc ff ff",
    )?;
    let first = expect(pe, parse + 0x1161,
        "66 c7 85 9c fc ff ff 18 00 89 9d e0 fa ff ff 68 ?? ?? ?? ?? 8b 85 e0 fa ff ff 50 e8 ?? ?? ?? ?? 83 c4 08 85 c0 0f 94 c2 83 e2 01 84 d2 0f 84 95 06 00 00")?;
    let tag1 = ascii_string(pe, dword(first, 16)?)?;
    let compare = call(pe, parse + 0x117c)?;
    compare_ascii(pe, compare)?;
    let second = expect(pe, parse + 0x181a,
        "c7 85 88 fc ff ff 02 00 00 00 e9 74 0d 00 00 c7 85 24 fa ff ff ?? ?? ?? ?? 89 9d 20 fa ff ff 8b 8d 24 fa ff ff 51 8b 8d 20 fa ff ff 51 e8 ?? ?? ?? ?? 83 c4 08 85 c0 0f 94 c0 83 e0 01 84 c0 0f 84 3b 07 00 00")?;
    let tag2 = ascii_string(pe, dword(second, 21)?)?;
    if call(pe, parse + 0x1847)? != compare || tag1.eq_ignore_ascii_case(&tag2) {
        return Err("ambiguous BGV role branches".into());
    }
    let concat = expect(pe, parse + 0x1f8b,
        "c7 85 88 fc ff ff 03 00 00 00 e9 03 06 00 00 66 c7 85 9c fc ff ff a4 01 53 68 ?? ?? ?? ?? 8d 45 d8 50 8d 95 88 fe ff ff 52 e8 ?? ?? ?? ?? 83 c4 0c 8d 8d 88 fe ff ff 51 ff 85 a8 fc ff ff 8d 85 60 fe ff ff 50 e8 ?? ?? ?? ?? 83 c4 0c")?;
    let prefix = ascii_string(pe, dword(concat, 26)?)?;
    let join = call(pe, parse + 0x1fb4)?;
    join.checked_add(0x980)
        .ok_or("BGV concatenation address overflow")?;
    if call(pe, parse + 0x1fd0)? != join {
        return Err("unrecognized BGV resource name composition".into());
    }
    // operator+(string, char*): copies left input, then appends right at its old length.
    expect(
        pe,
        join + 0x1c,
        "8b 55 10 89 55 b0 8b 4d 0c 89 4d ac 8b 45 ac 83 c0 08 50",
    )?;
    expect(pe, join + 0x95b,
        "8b 43 20 03 c8 89 8d 94 fe ff ff 8b 95 14 ff ff ff 52 8b 4d b0 51 8b 85 94 fe ff ff 50 e8 ?? ?? ?? ?? 83 c4 0c")?;
    expect(
        pe,
        parse + 0x259d,
        "ff 85 64 fc ff ff 8b 85 64 fc ff ff 8b 95 60 fc ff ff 3b c2 0f 82 9a db ff ff",
    )?;
    Ok(([tag1, tag2], prefix))
}

fn executor(pe: &Pe<'_>, execute: u32) -> Result<[u32; 2], String> {
    execute
        .checked_add(0x4ed2)
        .ok_or("BGV executor address overflow")?;
    let stop = expect(
        pe,
        execute + 0x46b3,
        "85 db 0f 85 0b 07 00 00 68 ?? ?? ?? ?? e8 ?? ?? ?? ?? 59 68 ?? ?? ?? ?? e8 ?? ?? ?? ?? 59",
    )?;
    let slots = [dword(stop, 9)?, dword(stop, 20)?];
    if call(pe, execute + 0x46c0)? != call(pe, execute + 0x46cb)? {
        return Err("unrecognized BGV dual stop".into());
    }
    let select = expect(pe, execute + 0x4dc6,
        "8d 14 9b 8d 14 93 c1 e2 03 81 c2 ?? ?? ?? ?? 89 95 cc f5 ff ff 8b 4d d0 85 c9 0f 84 f7 06 00 00")?;
    let records = dword(select, 11)?;
    if records.checked_add(2 * 0xa8 + 0x60) != Some(slots[0])
        || records.checked_add(3 * 0xa8 + 0x60) != Some(slots[1])
    {
        return Err("BGV channel slots disagree with its stop handler".into());
    }
    expect(pe, execute + 0x4e89,
        "6a 01 8b 85 cc f5 ff ff 50 8b 95 cc f5 ff ff 83 c2 60 52 e8 ?? ?? ?? ?? 83 c4 0c 6a 00 8b 85 c8 f5 ff ff 50 8b 95 cc f5 ff ff 83 c2 60 52 e8 ?? ?? ?? ?? 83 c4 0c 6a 00 6a 00 8b 8d cc f5 ff ff 83 c1 60 51 e8 ?? ?? ?? ??")?;
    Ok(slots)
}

fn suppression(pe: &Pe<'_>, slots: [u32; 2]) -> Result<[String; 2], String> {
    let mut functions = Vec::new();
    for (offset, bytes) in pe.bytes.windows(8).enumerate() {
        if bytes != b"MuteBGM\0" {
            continue;
        }
        let string = pe.va(offset).ok_or("unmapped MuteBGM string")?;
        for section in pe.sections.iter().filter(|s| s.executable) {
            let code = &pe.bytes[section.raw..section.raw + section.size];
            for (i, part) in code.windows(10).enumerate() {
                if part[0] != 0x68 || part[5] != 0x68 || part[6..10] != string.to_le_bytes() {
                    continue;
                }
                let wrapper = dword(part, 1)?;
                if wrapper.checked_add(12).is_none() {
                    continue;
                }
                let Some(body) = pe.data(wrapper, 12, true) else {
                    continue;
                };
                if body[..4] == [0x55, 0x8b, 0xec, 0xe8] && body[8..] == [0x33, 0xc0, 0x5d, 0xc3] {
                    // Registration signature after push function/name, not arbitrary pointers.
                    let tail = &code[i + 10..];
                    if tail.starts_with(&[0x56, 0x53, 0xe8]) {
                        functions.push(call(pe, wrapper + 3)?);
                    }
                }
            }
        }
    }
    if functions.len() != 1 {
        return Err("unrecognized or ambiguous BGV MuteBGM binding".into());
    }
    let function = functions[0];
    function
        .checked_add(0x4d1)
        .ok_or("BGV suppression address overflow")?;
    let code = expect(pe, function + 0x422,
        "8a 45 b3 84 c0 74 24 68 ?? ?? ?? ?? e8 ?? ?? ?? ?? 59 50 6a 00 68 ?? ?? ?? ?? 8d 55 d8 52 e8 ?? ?? ?? ?? 83 c4 10 83 f8 ff 75 04 33 db eb 05 bb 01 00 00 00 8a 45 b3 84 c0 74 24 68 ?? ?? ?? ?? e8 ?? ?? ?? ?? 59 50 6a 00 68 ?? ?? ?? ?? 8d 55 d8 52 e8 ?? ?? ?? ?? 83 c4 10 83 f8 ff 75 04 33 c9 eb 05 b9 01 00 00 00 88 8d 2f ff ff ff 6a 00 84 db 74 04 33 c0 eb 05 a1 ?? ?? ?? ?? 50 68 ?? ?? ?? ?? e8 ?? ?? ?? ?? 83 c4 0c 6a 00 8a 95 2f ff ff ff 84 d2 74 04 33 c9 eb 06 8b 0d ?? ?? ?? ?? 51 68 ?? ?? ?? ?? e8 ?? ?? ?? ?? 83 c4 0c")?;
    let tags = [dword(code, 8)?, dword(code, 60)?];
    if tags != [dword(code, 22)?, dword(code, 74)?]
        || slots != [dword(code, 127)?, dword(code, 163)?]
        || call(pe, function + 0x440)? != call(pe, function + 0x474)?
        || call(pe, function + 0x4a5)? != call(pe, function + 0x4c9)?
    {
        return Err("BGV voice suppression does not match its channel slots".into());
    }
    Ok([ascii_string(pe, tags[0])?, ascii_string(pe, tags[1])?])
}

/// Absence is None; a present but unrecognized command must not inherit this ABI.
pub fn probe_bgv(bytes: &[u8]) -> Result<Option<BgvProfile>, String> {
    let markers: Vec<_> = bytes
        .windows(11)
        .enumerate()
        .filter_map(|(i, b)| (b == b"BgvCommand\0").then_some(i))
        .collect();
    if markers.is_empty() {
        return Ok(None);
    }
    if markers.len() != 1 {
        return Err("ambiguous BgvCommand RTTI".into());
    }
    let pe = Pe::parse(bytes)?;
    let header = markers[0]
        .checked_sub(48)
        .and_then(|i| pe.va(i))
        .ok_or("unmapped BgvCommand RTTI")?;
    let mut handlers = Vec::new();
    for (i, b) in bytes.windows(20).enumerate() {
        if b[..4] == header.to_le_bytes() && b[4..12] == [0; 8] {
            let parse = dword(bytes, i + 12)?;
            let execute = dword(bytes, i + 16)?;
            if pe.data(parse, 3, true) == Some(&[0x55, 0x8b, 0xec])
                && pe.data(execute, 3, true) == Some(&[0x55, 0x8b, 0xec])
            {
                handlers.push((parse, execute));
            }
        }
    }
    if handlers.len() != 1 {
        return Err("unrecognized or ambiguous BgvCommand vtable".into());
    }
    let (tags, name_prefix) = compiler(&pe, handlers[0].0, handlers[0].1)?;
    let suppressed = suppression(&pe, executor(&pe, handlers[0].1)?)?;
    Ok(Some(BgvProfile {
        channels: [
            BgvChannel {
                channel: 2,
                tag: tags[0].clone(),
                voice_suppression_substring: suppressed[0].clone(),
            },
            BgvChannel {
                channel: 3,
                tag: tags[1].clone(),
                voice_suppression_substring: suppressed[1].clone(),
            },
        ],
        name_prefix,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn token_order_and_last_assignment_follow_native_compiler() {
        let p = BgvProfile {
            channels: [
                BgvChannel {
                    channel: 2,
                    tag: "QX".into(),
                    voice_suppression_substring: "voice_one".into(),
                },
                BgvChannel {
                    channel: 3,
                    tag: "ZY".into(),
                    voice_suppression_substring: "voice_two".into(),
                },
            ],
            name_prefix: "LOOP".into(),
        };
        assert_eq!(p.parse::<&str>(&[]).unwrap(), BgvResolved::default());
        assert_eq!(
            p.parse(&["QX"]).unwrap(),
            BgvResolved {
                channel: 2,
                tag: "QX".into(),
                name: String::new()
            }
        );
        assert_eq!(
            p.parse(&["QX", "001", "002", "zy"]).unwrap(),
            BgvResolved {
                channel: 3,
                tag: "zy".into(),
                name: "QXLOOP002".into()
            }
        );
        assert_eq!(p.parse(&["001", "QX"]).unwrap().name, "LOOP001");
        assert_eq!(p.parse(&["001"]).unwrap().channel, 0);
        assert_eq!(p.parse(&["qx", "001"]).unwrap().name, "qxLOOP001");
        assert!(p.parse(&[vec![0xff]]).is_err());
    }
    #[test]
    fn absent_and_malformed_pe_are_distinct() {
        assert_eq!(probe_bgv(b"unrelated"), Ok(None));
        assert!(probe_bgv(b"BgvCommand\0").is_err());
    }
    #[test]
    #[ignore = "set NEKO_ENGINE_ABI_SORAKOI for read-only original PE and relocated constant checks"]
    fn original_and_changed_constants() {
        let bytes = std::fs::read(std::env::var_os("NEKO_ENGINE_ABI_SORAKOI").unwrap()).unwrap();
        let profile = probe_bgv(&bytes).unwrap().unwrap();
        assert_eq!(profile.channels[0].tag, "AJ");
        assert_eq!(profile.channels[1].tag, "HM");
        assert_eq!(profile.name_prefix, "BGV");
        assert_eq!(profile.channels[0].voice_suppression_substring, "AJ");
        assert_eq!(profile.channels[1].voice_suppression_substring, "HM");
        // Append new constants to the last section and redirect instruction pointers.
        // Compiler tags and suppression tags deliberately differ.
        let pe = Pe::parse(&bytes).unwrap();
        let (index, last) = pe
            .sections
            .iter()
            .enumerate()
            .max_by_key(|(_, s)| s.raw)
            .unwrap();
        let start = bytes.len();
        let constant_va = pe.base + last.rva + (start - last.raw) as u32;
        let raw = last.raw;
        let nt = dword(&bytes, 0x3c).unwrap() as usize;
        let table = nt + 24 + super::super::word(&bytes, nt + 20).unwrap() as usize;
        let mut changed = bytes.clone();
        changed.extend_from_slice(b"QX\0ZY\0ALT\0SX\0SY\0");
        let length = (changed.len() - raw) as u32;
        changed[table + index * 40 + 16..table + index * 40 + 20]
            .copy_from_slice(&length.to_le_bytes());
        for (instruction, displacement) in [
            (0x44ac59, 0),
            (0x44b317, 3),
            (0x44ba8d, 6),
            (0x448376, 10),
            (0x448384, 10),
            (0x4483aa, 13),
            (0x4483b8, 13),
        ] {
            let offset = pe.offset(instruction, 4, true).unwrap();
            changed[offset..offset + 4]
                .copy_from_slice(&(constant_va + displacement).to_le_bytes());
        }
        let result = probe_bgv(&changed).unwrap().unwrap();
        assert_eq!(result.channels[0].tag, "QX");
        assert_eq!(result.channels[1].tag, "ZY");
        assert_eq!(result.name_prefix, "ALT");
        assert_eq!(result.channels[0].voice_suppression_substring, "SX");
        assert_eq!(result.channels[1].voice_suppression_substring, "SY");
        let at = pe.offset(0x44b308, 4, true).unwrap();
        changed[at] = 4;
        assert!(probe_bgv(&changed).is_err());
    }
}
