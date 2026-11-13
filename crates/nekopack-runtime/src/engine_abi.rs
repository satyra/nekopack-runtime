//! Read-only detection of original x86 command ABIs.
//! Unknown handlers are errors; filenames, product versions and canvas sizes are not inputs.
#[path = "engine_abi/bgv.rs"]
mod bgv;
pub use bgv::{probe_bgv, BgvChannel, BgvProfile, BgvResolved};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WipeTiming {
    MillisecondsInteger,
    SecondsFloat,
}

struct Section {
    rva: u32,
    raw: usize,
    size: usize,
    executable: bool,
}
struct Pe<'a> {
    bytes: &'a [u8],
    base: u32,
    sections: Vec<Section>,
}
fn word(bytes: &[u8], at: usize) -> Result<u16, String> {
    Ok(u16::from_le_bytes(
        bytes
            .get(at..at + 2)
            .ok_or("truncated PE")?
            .try_into()
            .unwrap(),
    ))
}
fn dword(bytes: &[u8], at: usize) -> Result<u32, String> {
    Ok(u32::from_le_bytes(
        bytes
            .get(at..at + 4)
            .ok_or("truncated PE")?
            .try_into()
            .unwrap(),
    ))
}
impl<'a> Pe<'a> {
    fn parse(bytes: &'a [u8]) -> Result<Self, String> {
        if bytes.get(..2) != Some(b"MZ") {
            return Err("not a PE image".into());
        }
        let nt = dword(bytes, 0x3c)? as usize;
        if bytes.get(nt..nt + 4) != Some(b"PE\0\0") || word(bytes, nt + 4)? != 0x14c {
            return Err("engine ABI probe requires an x86 PE image".into());
        }
        let optional = nt + 24;
        if word(bytes, optional)? != 0x10b {
            return Err("engine ABI probe requires PE32".into());
        }
        let base = dword(bytes, optional + 28)?;
        let table = optional + usize::from(word(bytes, nt + 20)?);
        let mut sections = Vec::new();
        for i in 0..usize::from(word(bytes, nt + 6)?) {
            let at = table + i * 40;
            let section = Section {
                rva: dword(bytes, at + 12)?,
                size: dword(bytes, at + 16)? as usize,
                raw: dword(bytes, at + 20)? as usize,
                executable: dword(bytes, at + 36)? & 0x2000_0000 != 0,
            };
            if section
                .raw
                .checked_add(section.size)
                .is_none_or(|end| end > bytes.len())
            {
                return Err("PE section outside file".into());
            }
            sections.push(section);
        }
        Ok(Self {
            bytes,
            base,
            sections,
        })
    }
    fn va(&self, offset: usize) -> Option<u32> {
        let s = self
            .sections
            .iter()
            .find(|s| offset >= s.raw && offset - s.raw < s.size)?;
        self.base
            .checked_add(s.rva)?
            .checked_add(u32::try_from(offset - s.raw).ok()?)
    }
    fn offset(&self, va: u32, size: usize, executable: bool) -> Option<usize> {
        let rva = va.checked_sub(self.base)?;
        let s = self.sections.iter().find(|s| {
            rva >= s.rva
                && (rva - s.rva) as usize <= s.size
                && size <= s.size - (rva - s.rva) as usize
                && (!executable || s.executable)
        })?;
        Some(s.raw + (rva - s.rva) as usize)
    }
    fn data(&self, va: u32, size: usize, executable: bool) -> Option<&'a [u8]> {
        let start = self.offset(va, size, executable)?;
        self.bytes.get(start..start + size)
    }
    fn call_target(&self, call: u32) -> Option<u32> {
        let code = self.data(call, 5, true)?;
        if code[0] != 0xe8 {
            return None;
        }
        Some(
            call.wrapping_add(5)
                .wrapping_add(u32::from_le_bytes(code[1..5].try_into().unwrap())),
        )
    }
    fn digit_classifier(&self, va: u32) -> bool {
        let Some(b) = self.data(va, 19, true) else {
            return false;
        };
        b.starts_with(&[0x55, 0x8b, 0xec, 0x6a, 4, 0x8b, 0x45, 8, 0x50, 0xe8])
            && b[14..] == [0x83, 0xc4, 8, 0x5d, 0xc3]
            && self
                .call_target(va + 9)
                .and_then(|a| self.data(a, 1, true))
                .is_some()
    }
    fn atoi(&self, va: u32) -> bool {
        let Some(b) = self.data(va, 110, true) else {
            return false;
        };
        // Skip leading whitespace, optional sign, decimal accumulation, digit range,
        // signed result and return. The whitespace classifier's relative call varies.
        b.starts_with(&[
            0x55, 0x8b, 0xec, 0x53, 0x56, 0x57, 0x33, 0xff, 0x8b, 0x75, 8, 0x8a, 0x1e, 0x46, 0x0f,
            0xbe, 0xc3, 0x50, 0xe8,
        ]) && contains(b, &[0x80, 0xfb, 0x2b, 0x74, 5, 0x80, 0xfb, 0x2d])
            && contains(b, &[0x03, 0xd2, 0x8d, 0x14, 0x92, 0x03, 0xd1])
            && contains(b, &[0x80, 0xfb, 0x30, 0x7c, 5, 0x80, 0xfb, 0x39])
            && contains(b, &[0x8b, 0xc7, 0xf7, 0xd8])
            && contains(b, &[0x5f, 0x5e, 0x5b, 0x5d, 0xc3])
    }
    fn atof(&self, va: u32) -> bool {
        let Some(b) = self.data(va, 19, true) else {
            return false;
        };
        b.starts_with(&[0x55, 0x8b, 0xec, 0x6a, 0, 0x8b, 0x45, 8, 0x50, 0xe8])
            && b[14..] == [0x83, 0xc4, 8, 0x5d, 0xc3]
            && self
                .call_target(va + 9)
                .and_then(|a| self.data(a, 1, true))
                .is_some()
    }
    fn ftol(&self, va: u32) -> bool {
        let Some(b) = self.data(va, 48, true) else {
            return false;
        };
        b.starts_with(&[0x55, 0x8b, 0xec, 0x8d, 0x65, 0xf4, 0x9b, 0xd9, 0x7d, 0xfc])
            && contains(b, &[0x81, 0x4d, 0xfc, 1, 0x0c, 0, 0])
            && contains(b, &[0xd9, 0x6d, 0xfc, 0xdf, 0x7d, 0xf4])
            && contains(
                b,
                &[0x8b, 0x45, 0xf4, 0x8b, 0x55, 0xf8, 0x8b, 0xe5, 0x5d, 0xc3],
            )
    }
    fn numeric_branch(&self, ctor: u32, execute: u32) -> Result<WipeTiming, String> {
        let size = usize::try_from(execute - ctor).map_err(|_| "invalid handler size")?;
        let code = self
            .data(ctor, size, true)
            .ok_or("WipeCommand constructor outside executable section")?;
        let mut results = Vec::new();
        for i in 0..code.len().saturating_sub(11) {
            if code[i] != 0xe8 || code[i + 5..i + 9] != [0x59, 0x85, 0xc0, 0x74] {
                continue;
            }
            let Some(classifier) = self.call_target(ctor + i as u32) else {
                continue;
            };
            if !self.digit_classifier(classifier) {
                continue;
            }
            let start = i + 10;
            let end = (start as isize + code[i + 9] as i8 as isize) as usize;
            if end <= start || end > code.len() {
                return Err("invalid WipeCommand numeric branch".into());
            }
            let branch = &code[start..end];
            let mut found = Vec::new();
            for j in 0..branch.len().saturating_sub(5) {
                if branch[j] != 0xe8 {
                    continue;
                }
                let call = ctor + (start + j) as u32;
                let Some(target) = self.call_target(call) else {
                    continue;
                };
                let tail = &branch[j + 5..];
                let (timing, stored) = if self.atoi(target) && tail.starts_with(&[0x59, 0x89, 0x85])
                {
                    (WipeTiming::MillisecondsInteger, tail.get(3..7))
                } else if self.atof(target)
                    && tail.starts_with(&[0x59, 0xd8, 0x0d])
                    && tail.len() >= 18
                {
                    let constant = u32::from_le_bytes(tail[3..7].try_into().unwrap());
                    if self.data(constant, 4, false) != Some(1000f32.to_le_bytes().as_slice())
                        || tail[7] != 0xe8
                        || tail[12..14] != [0x89, 0x85]
                        || !self.call_target(call + 12).is_some_and(|a| self.ftol(a))
                    {
                        continue;
                    }
                    (WipeTiming::SecondsFloat, tail.get(14..18))
                } else {
                    continue;
                };
                if let Some(slot) = stored {
                    let mut default = vec![0xc7, 0x85];
                    default.extend_from_slice(slot);
                    default.extend_from_slice(&500u32.to_le_bytes());
                    if contains(code, &default) {
                        found.push(timing);
                    }
                }
            }
            if found.len() != 1 {
                return Err("unrecognized WipeCommand numeric conversion".into());
            }
            results.extend(found);
        }
        if results.len() != 1 {
            return Err("unrecognized or ambiguous WipeCommand numeric branch".into());
        }
        Ok(results[0])
    }
}
fn contains(bytes: &[u8], pattern: &[u8]) -> bool {
    bytes.windows(pattern.len()).any(|w| w == pattern)
}

/// Return None only when no WipeCommand type is present. A present but unknown
/// handler must not silently inherit another engine's timing convention.
pub fn probe(bytes: &[u8]) -> Result<Option<WipeTiming>, String> {
    let markers: Vec<_> = bytes
        .windows(12)
        .enumerate()
        .filter_map(|(i, w)| (w == b"WipeCommand\0").then_some(i))
        .collect();
    if markers.is_empty() {
        return Ok(None);
    }
    if markers.len() != 1 {
        return Err("ambiguous WipeCommand type marker".into());
    }
    let pe = Pe::parse(bytes)?;
    // Borland RTTI's type descriptor begins 48 bytes before this class name.
    // The VMT layout is [type_info, 0, 0, parse, execute]: type_info is a
    // PREFIX, 12 bytes before the method table. In adjacent tables the pointer
    // after a pair of methods belongs to the NEXT class, not that method pair.
    // Verified against actual script-key -> object -> VMT registration, rather
    // than assigning a class name to the nearest preceding pair of functions.
    let header = markers[0]
        .checked_sub(48)
        .and_then(|o| pe.va(o))
        .ok_or("unmapped WipeCommand RTTI")?;
    let mut handlers = Vec::new();
    for (i, w) in bytes.windows(20).enumerate() {
        if w[..4] != header.to_le_bytes() || w[4..12] != [0; 8] {
            continue;
        }
        let ctor = dword(bytes, i + 12)?;
        let execute = dword(bytes, i + 16)?;
        if ctor >= execute {
            continue;
        }
        if pe.data(ctor, 3, true) == Some([0x55, 0x8b, 0xec].as_slice())
            && pe.data(execute, 3, true) == Some([0x55, 0x8b, 0xec].as_slice())
        {
            handlers.push((ctor, execute));
        }
    }
    if handlers.len() != 1 {
        return Err("unrecognized or ambiguous WipeCommand vtable".into());
    }
    pe.numeric_branch(handlers[0].0, handlers[0].1).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn put16(b: &mut [u8], at: usize, n: u16) {
        b[at..at + 2].copy_from_slice(&n.to_le_bytes());
    }
    fn put32(b: &mut [u8], at: usize, n: u32) {
        b[at..at + 4].copy_from_slice(&n.to_le_bytes());
    }
    fn call(b: &mut Vec<u8>, start: u32, target: u32) {
        let at = start + b.len() as u32;
        b.push(0xe8);
        b.extend_from_slice(&target.wrapping_sub(at + 5).to_le_bytes());
    }
    fn image(timing: WipeTiming) -> Vec<u8> {
        let mut b = vec![0; 0x2200];
        b[..2].copy_from_slice(b"MZ");
        put32(&mut b, 0x3c, 0x80);
        b[0x80..0x84].copy_from_slice(b"PE\0\0");
        put16(&mut b, 0x84, 0x14c);
        put16(&mut b, 0x86, 2);
        put16(&mut b, 0x94, 0xe0);
        put16(&mut b, 0x98, 0x10b);
        put32(&mut b, 0x98 + 28, 0x400000);
        for (at, rva, raw, flags) in [
            (0x178, 0x1000, 0x200, 0x60000020),
            (0x1a0, 0x3000, 0x1200, 0x40000040),
        ] {
            put32(&mut b, at + 12, rva);
            put32(&mut b, at + 16, 0x1000);
            put32(&mut b, at + 20, raw);
            put32(&mut b, at + 36, flags);
        }
        b[0x12b0..0x12bc].copy_from_slice(b"WipeCommand\0");
        put32(&mut b, 0x1300, 0x403080);
        put32(&mut b, 0x130c, 0x401000);
        put32(&mut b, 0x1310, 0x401100);
        b[0x300..0x304].copy_from_slice(&[0x55, 0x8b, 0xec, 0xc3]);
        let mut classifier = vec![0x55, 0x8b, 0xec, 0x6a, 4, 0x8b, 0x45, 8, 0x50];
        call(&mut classifier, 0x401200, 0x401600);
        classifier.extend_from_slice(&[0x83, 0xc4, 8, 0x5d, 0xc3]);
        b[0x400..0x400 + classifier.len()].copy_from_slice(&classifier);
        let mut atof = classifier.clone();
        atof[4] = 0;
        b[0x600..0x600 + atof.len()].copy_from_slice(&atof);
        let atoi = &mut b[0x500..0x56e];
        atoi[..19].copy_from_slice(&[
            0x55, 0x8b, 0xec, 0x53, 0x56, 0x57, 0x33, 0xff, 0x8b, 0x75, 8, 0x8a, 0x1e, 0x46, 0x0f,
            0xbe, 0xc3, 0x50, 0xe8,
        ]);
        atoi[24..32].copy_from_slice(&[0x80, 0xfb, 0x2b, 0x74, 5, 0x80, 0xfb, 0x2d]);
        atoi[40..47].copy_from_slice(&[0x03, 0xd2, 0x8d, 0x14, 0x92, 0x03, 0xd1]);
        atoi[60..68].copy_from_slice(&[0x80, 0xfb, 0x30, 0x7c, 5, 0x80, 0xfb, 0x39]);
        atoi[80..84].copy_from_slice(&[0x8b, 0xc7, 0xf7, 0xd8]);
        atoi[90..95].copy_from_slice(&[0x5f, 0x5e, 0x5b, 0x5d, 0xc3]);
        let ftol = &mut b[0x700..0x730];
        ftol[..10].copy_from_slice(&[0x55, 0x8b, 0xec, 0x8d, 0x65, 0xf4, 0x9b, 0xd9, 0x7d, 0xfc]);
        ftol[12..19].copy_from_slice(&[0x81, 0x4d, 0xfc, 1, 0x0c, 0, 0]);
        ftol[19..25].copy_from_slice(&[0xd9, 0x6d, 0xfc, 0xdf, 0x7d, 0xf4]);
        ftol[38..48].copy_from_slice(&[0x8b, 0x45, 0xf4, 0x8b, 0x55, 0xf8, 0x8b, 0xe5, 0x5d, 0xc3]);
        b[0x1200..0x1204].copy_from_slice(&1000f32.to_le_bytes());
        let mut ctor = vec![0x55, 0x8b, 0xec];
        call(&mut ctor, 0x401000, 0x401200);
        ctor.extend_from_slice(&[0x59, 0x85, 0xc0, 0x74, 0]);
        let branch = ctor.len();
        ctor.push(0x50);
        call(
            &mut ctor,
            0x401000,
            if timing == WipeTiming::SecondsFloat {
                0x401400
            } else {
                0x401300
            },
        );
        ctor.push(0x59);
        if timing == WipeTiming::SecondsFloat {
            ctor.extend_from_slice(&[0xd8, 0x0d]);
            ctor.extend_from_slice(&0x403000u32.to_le_bytes());
            call(&mut ctor, 0x401000, 0x401500);
        }
        ctor.extend_from_slice(&[0x89, 0x85]);
        ctor.extend_from_slice(&0xfffffcf0u32.to_le_bytes());
        ctor.extend_from_slice(&[0xe9, 0, 0, 0, 0]);
        ctor[branch - 1] = (ctor.len() - branch) as u8;
        ctor.extend_from_slice(&[0xc7, 0x85]);
        ctor.extend_from_slice(&0xfffffcf0u32.to_le_bytes());
        ctor.extend_from_slice(&500u32.to_le_bytes());
        b[0x200..0x200 + ctor.len()].copy_from_slice(&ctor);
        b
    }
    #[test]
    fn recognizes_numeric_abis() {
        for timing in [WipeTiming::MillisecondsInteger, WipeTiming::SecondsFloat] {
            assert_eq!(probe(&image(timing)).unwrap(), Some(timing));
        }
    }
    #[test]
    fn rtti_prefix_is_not_the_previous_classes_suffix() {
        let mut b = image(WipeTiming::SecondsFloat);
        // These adjacent methods precede the Wipe RTTI pointer at 0x1300.
        // Treating [parse, execute, type_info] as one record would select them.
        put32(&mut b, 0x12f8, 0x401700);
        put32(&mut b, 0x12fc, 0x401710);
        b[0x900..0x904].copy_from_slice(&[0x55, 0x8b, 0xec, 0xc3]);
        b[0x910..0x914].copy_from_slice(&[0x55, 0x8b, 0xec, 0xc3]);
        // The pointer immediately after Wipe's methods describes the NEXT class.
        put32(&mut b, 0x1314, 0x403380);
        put32(&mut b, 0x1320, 0x401700);
        put32(&mut b, 0x1324, 0x401710);
        assert_eq!(probe(&b).unwrap(), Some(WipeTiming::SecondsFloat));
    }
    #[test]
    fn unknown_and_ambiguous_handlers_are_errors() {
        let mut b = image(WipeTiming::SecondsFloat);
        b[0x1200..0x1204].copy_from_slice(&1f32.to_le_bytes());
        assert!(probe(&b).is_err());
        let mut b = image(WipeTiming::MillisecondsInteger);
        b[0x53c] = 0;
        assert!(probe(&b).is_err());
        let mut b = image(WipeTiming::MillisecondsInteger);
        let duplicate = b[0x1300..0x1314].to_vec();
        b[0x1400..0x1414].copy_from_slice(&duplicate);
        assert!(probe(&b).is_err());
        let mut b = image(WipeTiming::SecondsFloat);
        b[0x716] = 0;
        assert!(probe(&b).is_err());
    }
    #[test]
    fn absent_type_is_none_and_present_malformed_pe_errors() {
        assert_eq!(probe(b"unrelated file").unwrap(), None);
        assert!(probe(b"WipeCommand\0").is_err());
    }
    #[test]
    #[ignore = "set NEKO_ENGINE_ABI_GAMES and NEKO_ENGINE_ABI_SORAKOI for local read-only PE checks"]
    fn local_original_handlers() {
        fn find(root: &std::path::Path, name: &str) -> std::path::PathBuf {
            for entry in std::fs::read_dir(root).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    let hit = find(&path, name);
                    if !hit.as_os_str().is_empty() {
                        return hit;
                    }
                } else if path.file_name().unwrap() == name {
                    return path;
                }
            }
            std::path::PathBuf::new()
        }
        let root = std::path::PathBuf::from(std::env::var_os("NEKO_ENGINE_ABI_GAMES").unwrap());
        for name in [
            "Tiny_BoD.exe",
            "Tiny_BfY.exe",
            "Tiny_BoS.exe",
            "Tiny_BaW.exe",
            "ed.exe",
        ] {
            let path = find(&root, name);
            assert!(!path.as_os_str().is_empty());
            assert_eq!(
                probe(&std::fs::read(path).unwrap()).unwrap(),
                Some(WipeTiming::MillisecondsInteger),
                "{name}"
            );
        }
        let path = std::env::var_os("NEKO_ENGINE_ABI_SORAKOI").unwrap();
        assert_eq!(
            probe(&std::fs::read(path).unwrap()).unwrap(),
            Some(WipeTiming::SecondsFloat)
        );
    }
}
