use crate::{GameResources, quote};
use nekopack_archive::Archive;
use nekopack_image::{Metadata, Mng};
use nekopack_resource::{Catalog, Resolution, resolve};
use nekopack_script::{
    runtime::{Scripts, Source},
    system::ResourceProvider,
};
use std::{
    collections::BTreeMap,
    io::{Read, Seek},
};
/// Original CP932 resources with explicit profiles for the verified Chinese scenario packs.
pub struct DatGame<R: Read + Seek> {
    archives: Vec<Archive<R>>,
    catalog: Catalog,
}
pub(crate) fn decode(bytes: &[u8]) -> Result<String, String> {
    let (text, errors) = encoding_rs::SHIFT_JIS.decode_without_bom_handling(bytes);
    if errors {
        Err("invalid CP932-compatible input".into())
    } else {
        Ok(text.into_owned())
    }
}
fn chinese_header(h: nekopack_archive::Header) -> bool {
    matches!(
        (h.name_seed, h.priority, h.index_key, h.index_length),
        (1259259315, 65535, 1449909115, 2160)
            | (607214694, 260, 534709390, 1576)
            | (607214694, 512, 534168718, 1464)
            | (2608180646, 258, 3267253311, 1896)
    )
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum ChinesePack {
    TinyDungeon,
    EndlessScripts,
    EndlessSystem,
}

fn chinese_pack<R: Read + Seek>(archive: &Archive<R>) -> Option<ChinesePack> {
    let h = archive.header();
    if chinese_header(h) {
        return Some(ChinesePack::TinyDungeon);
    }
    let end = archive
        .entries()
        .last()
        .map(|entry| entry.block_offset + 8 + u64::from(entry.length))?;
    match (h.name_seed, h.priority, h.index_key, h.index_length, end) {
        (2608180649, 0, 633230395, 1304, 1331882) => Some(ChinesePack::EndlessScripts),
        (2608180649, 0, 2780714047, 360, 14682537) => Some(ChinesePack::EndlessSystem),
        _ => None,
    }
}

fn chinese_ui_resource(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "title.mng"
            | "config.mng"
            | "config1.mng"
            | "config2.mng"
            | "config3.mng"
            | "config4.mng"
            | "config5.mng"
            | "menu.mng"
            | "saveload.mng"
            | "shortcut.mng"
            | "msgbox.mng"
            | "log.mng"
            | "x_cg.mng"
            | "x_scene.mng"
            | "x_actor.mng"
            | "config.nut"
            | "message.nut"
            | "saveload.nut"
            | "select.nut"
    )
}

fn decode_chinese_ui_script(raw: &[u8]) -> Result<String, String> {
    let mut out = String::new();
    for line in raw.split_inclusive(|&byte| byte == b'\n') {
        let trimmed = line.trim_ascii_start();
        let localized = [b"Font".as_slice(), b"g.Face", b"local text"]
            .iter()
            .any(|prefix| {
                trimmed
                    .strip_prefix(*prefix)
                    .is_some_and(|tail| tail.trim_ascii_start().starts_with(b"="))
            });
        let encoding = if localized {
            encoding_rs::GB18030
        } else {
            encoding_rs::SHIFT_JIS
        };
        let (text, errors) = encoding.decode_without_bom_handling(line);
        if errors {
            return Err("invalid Chinese UI script encoding".into());
        }
        out.push_str(&text);
    }
    Ok(out)
}

fn decode_chinese_script(raw: &[u8]) -> Result<String, String> {
    decode_mixed_scenario(raw, false)
}
fn decode_mixed_scenario(raw: &[u8], endless: bool) -> Result<String, String> {
    let mut out = String::new();
    for (line_no, line) in raw.split(|&b| b == b'\n').enumerate() {
        if line_no != 0 {
            out.push('\n');
        }
        let trimmed = line.trim_ascii_start();
        if trimmed.starts_with(b";") {
            out.push(';');
            continue;
        }
        let syntax = trimmed.starts_with(b".")
            || trimmed.starts_with(b"?")
            || trimmed.starts_with(b"#")
            || (endless && trimmed.starts_with(b"@"));
        let label =
            !endless && (trimmed.starts_with(b".title ") || trimmed.starts_with(b".route "));
        let enc = if syntax && !label {
            encoding_rs::SHIFT_JIS
        } else {
            encoding_rs::GB18030
        };
        let (text, errors) = enc.decode_without_bom_handling(line);
        if errors {
            return Err(format!(
                "invalid mixed script encoding at line {}",
                line_no + 1
            ));
        }
        out.push_str(&text);
    }
    Ok(out)
}
impl<R: Read + Seek> DatGame<R> {
    /// Mount-layout identity plus script validation is used by runtime saves.
    /// This is not a cryptographic hash of every resource payload.
    pub(crate) fn save_identity(&self) -> u64 {
        let mut w = nekopack_script::checkpoint::Writer::default();
        for a in &self.archives {
            let h = a.header();
            for n in [h.name_seed, h.priority, h.index_key, h.index_length] {
                w.number(u64::from(n));
            }
            for e in a.entries() {
                for n in [e.directory_hash, e.name_hash, e.length, e.key] {
                    w.number(u64::from(n));
                }
                w.number(e.block_offset);
            }
        }
        nekopack_script::checkpoint::digest(&w.0)
    }
    pub fn mount(sources: impl IntoIterator<Item = R>) -> Result<Self, String> {
        Self::mount_version(sources, false)
    }
    pub fn mount_version(
        sources: impl IntoIterator<Item = R>,
        original_only: bool,
    ) -> Result<Self, String> {
        let mut archives = Vec::new();
        let mut catalog = Catalog::default();
        for source in sources {
            let archive = Archive::open(source, 16 * 1024 * 1024).map_err(|e| e.to_string())?;
            if original_only && chinese_pack(&archive).is_some() {
                continue;
            }
            catalog.add(archive.header(), archive.entries());
            archives.push(archive);
        }
        if archives.is_empty() {
            return Err("no DAT archives selected".into());
        }
        Ok(Self { archives, catalog })
    }
    pub(crate) fn read(&mut self, dir: &str, name: &str) -> Result<Vec<u8>, String> {
        self.read_optional(dir, name)?
            .ok_or_else(|| format!("missing DAT resource {dir}/{name}"))
    }
    pub(crate) fn read_optional(
        &mut self,
        dir: &str,
        name: &str,
    ) -> Result<Option<Vec<u8>>, String> {
        let Some(candidate) = self.resource_entry(dir, name)? else {
            return Ok(None);
        };
        let localized_ui = dir == "system"
            && name.to_ascii_lowercase().ends_with(".mng")
            && chinese_header(self.archives[candidate.source].header());
        let bytes = self.read_entry(candidate)?;
        if localized_ui {
            let canvas = self.original_ui_canvas(name)?;
            return nekopack_image::normalize_localized_ui_frames(&bytes, canvas)
                .map(Some)
                .map_err(|error| format!("{name}: {error}"));
        }
        Ok(Some(bytes))
    }
    fn original_ui_canvas(&mut self, name: &str) -> Result<Option<[u32; 2]>, String> {
        let (encoded, _, _) = encoding_rs::SHIFT_JIS.encode(name);
        let base: Vec<_> = self
            .catalog
            .lookup(b"system", &encoded)
            .into_iter()
            .filter(|candidate| !chinese_header(self.archives[candidate.source].header()))
            .collect();
        match resolve(&base) {
            Resolution::Missing => Ok(None),
            Resolution::Ambiguous(_) => Err(format!("ambiguous original canvas for {name}")),
            Resolution::Selected(candidate) => {
                let prefix = self.archives[candidate.source]
                    .read_prefix(candidate.ordinal, 48)
                    .map_err(|error| error.to_string())?;
                nekopack_image::mng_canvas(&prefix)
                    .map(Some)
                    .map_err(|error| error.to_string())
            }
        }
    }
    fn read_entry(&mut self, candidate: nekopack_resource::Candidate) -> Result<Vec<u8>, String> {
        let archive = &mut self.archives[candidate.source];
        let length = archive.entries()[candidate.ordinal].length as usize;
        if length > 64 * 1024 * 1024 {
            return Err("resource exceeds 64 MiB limit".into());
        }
        archive
            .read_prefix(candidate.ordinal, length)
            .map_err(|e| e.to_string())
    }
    pub fn has_chinese_scenario_patch(&self) -> bool {
        self.archives.iter().any(|archive| {
            matches!(
                chinese_pack(archive),
                Some(ChinesePack::TinyDungeon | ChinesePack::EndlessScripts)
            )
        })
    }
    pub fn recognized_chinese_archives(&self) -> Vec<usize> {
        self.archives
            .iter()
            .enumerate()
            .filter_map(|(index, archive)| chinese_pack(archive).map(|_| index))
            .collect()
    }
    pub fn has_resource(&self, dir: &str, name: &str) -> Result<bool, String> {
        Ok(self.resource_entry(dir, name)?.is_some())
    }
    fn resource_entry(
        &self,
        dir: &str,
        name: &str,
    ) -> Result<Option<nekopack_resource::Candidate>, String> {
        if name.contains(['/', '\\', '\0']) || name == ".." {
            return Err("resource name is not a basename".into());
        }
        let (encoded, _, errors) = encoding_rs::SHIFT_JIS.encode(name);
        if errors {
            return Err("resource name cannot be encoded in CP932 profile".into());
        }
        let boot_script = ["start.txt", "title.txt", "titlelogo.txt"]
            .iter()
            .any(|boot| name.eq_ignore_ascii_case(boot));
        let candidates = self.catalog.lookup(dir.as_bytes(), &encoded);
        let candidates: Vec<_> = candidates
            .into_iter()
            .filter_map(|mut candidate| {
                match chinese_pack(&self.archives[candidate.source]) {
                    None => {}
                    Some(ChinesePack::TinyDungeon) => {
                        let supported = (dir == "system" && chinese_ui_resource(name))
                            || (dir == "script" && !boot_script);
                        if !supported {
                            return None;
                        }
                    }
                    Some(ChinesePack::EndlessScripts) if dir == "script" && !boot_script => {
                        candidate.priority = 1
                    }
                    Some(ChinesePack::EndlessSystem)
                        if dir == "system" && name.to_ascii_lowercase().ends_with(".mng") =>
                    {
                        candidate.priority = 1
                    }
                    Some(_) => return None,
                }
                Some(candidate)
            })
            .collect();
        match resolve(&candidates) {
            Resolution::Selected(c) => Ok(Some(c)),
            Resolution::Missing => Ok(None),
            Resolution::Ambiguous(_) => Err(format!("ambiguous DAT resource {dir}/{name}")),
        }
    }
}
impl<R: Read + Seek> Scripts for DatGame<R> {
    fn load(&mut self, name: &[u8]) -> Result<Source, String> {
        let text = std::str::from_utf8(name).map_err(|e| e.to_string())?;
        let filename = format!("{text}.txt");
        let selected = self
            .resource_entry("script", &filename)?
            .ok_or_else(|| format!("missing DAT resource script/{filename}"))?;
        let chinese = chinese_pack(&self.archives[selected.source]);
        let raw = self.read_entry(selected)?;
        Ok(Source {
            name: name.to_vec(),
            bytes: match chinese {
                Some(ChinesePack::EndlessScripts) => decode_mixed_scenario(&raw, true)?,
                Some(ChinesePack::TinyDungeon) => decode_chinese_script(&raw)?,
                _ => decode(&raw)?,
            }
            .into_bytes(),
        })
    }
}
impl<R: Read + Seek> GameResources for DatGame<R> {
    fn setup(&self) -> Result<String, String> {
        Ok(include_str!("../host/dat.nut").into())
    }
    fn system(&mut self, name: &[u8]) -> Result<Vec<u8>, String> {
        let name = std::str::from_utf8(name).map_err(|e| e.to_string())?;
        let candidate = self
            .resource_entry("system", name)?
            .ok_or_else(|| format!("missing DAT resource system/{name}"))?;
        let chinese = chinese_header(self.archives[candidate.source].header());
        let bytes = self.read_entry(candidate)?;
        Ok(if chinese {
            decode_chinese_ui_script(&bytes)?
        } else {
            decode(&bytes)?
        }
        .into_bytes())
    }
}
impl<R: Read + Seek> ResourceProvider for DatGame<R> {
    fn resource(&mut self, kind: &str, name: &str) -> Result<String, String> {
        match kind {
            "Data" | "Ini" => {
                let bytes = self.read_optional("system", name)?;
                let Some(bytes) = bytes else {
                    if kind == "Data"
                        && name
                            .strip_prefix("x_cg_page")
                            .and_then(|name| name.strip_suffix(".txt"))
                            .is_some_and(|number| number.parse::<u32>().is_ok())
                    {
                        // The room script enumerates more numbered pages than the archive ships.
                        return Ok("[]".into());
                    }
                    return Err(format!("missing DAT resource system/{name}"));
                };
                let text = decode(&bytes)?;
                let lines = text
                    .lines()
                    .map(str::trim)
                    .filter(|l| !l.is_empty() && !l.starts_with(';'));
                if kind == "Data" {
                    let rows = lines
                        .map(|line| {
                            let words = line
                                .split([' ', '\t'])
                                .filter(|s| !s.is_empty())
                                .map(|s| quote(s.as_bytes()))
                                .collect::<Result<Vec<_>, _>>()?;
                            Ok(format!("[{}]", words.join(",")))
                        })
                        .collect::<Result<Vec<_>, String>>()?;
                    Ok(format!("[{}]", rows.join(",")))
                } else {
                    let mut entries = BTreeMap::new();
                    for line in lines {
                        if let Some((k, v)) = line.split_once('=') {
                            entries.insert(k.trim(), v.trim());
                        }
                    }
                    let pairs = entries
                        .into_iter()
                        .map(|(k, v)| {
                            Ok(format!(
                                "[{}]={}",
                                quote(k.as_bytes())?,
                                quote(v.as_bytes())?
                            ))
                        })
                        .collect::<Result<Vec<_>, String>>()?;
                    Ok(format!("{{{}}}", pairs.join(",")))
                }
            }
            "Picture" => {
                let (dir, file) = name.rsplit_once('/').ok_or("invalid picture path")?;
                match self.read_optional(dir, file)? {
                    Some(bytes) => {
                        let [x, y] =
                            crate::story::png_position(&bytes).map_err(|e| e.to_string())?;
                        Ok(format!("[{},{x},{y}]", quote(name.as_bytes())?))
                    }
                    None => Ok("null".into()),
                }
            }
            "Assets" => {
                let (dir, stem) = name.rsplit_once('/').unwrap_or(("system", name));
                let file = format!("{stem}.mng");
                let bytes = self.read(dir, &file)?;
                let mng = Mng::parse(&bytes, 100000).map_err(|e| e.to_string())?;
                let images = mng.embedded_images().map_err(|e| e.to_string())?;
                let mut entries = BTreeMap::new();
                for sprite in mng.sprites().map_err(|e| e.to_string())? {
                    let image = &images[sprite.ordinal];
                    let Metadata::ImageHeader { width, height, .. } =
                        mng.chunks[image.first_chunk].metadata
                    else {
                        return Err("missing sprite image header".into());
                    };
                    entries.insert(
                        decode(sprite.name)?,
                        format!(
                            "[{},{},{},{},{}]",
                            quote(format!("{dir}/{file}:{}", sprite.ordinal).as_bytes())?,
                            sprite.position[0],
                            sprite.position[1],
                            width,
                            height
                        ),
                    );
                }
                let pairs = entries
                    .into_iter()
                    .map(|(k, v)| Ok(format!("[{}]={v}", quote(k.as_bytes())?)))
                    .collect::<Result<Vec<_>, String>>()?;
                Ok(format!("{{{}}}", pairs.join(",")))
            }
            _ => Err(format!("unknown resource kind {kind}")),
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn mixed_chinese_ui_script_decodes_font_and_preview_without_changing_labels() {
        let label = encoding_rs::SHIFT_JIS.encode("local button = \"設定\"\n").0;
        let font = encoding_rs::GB18030.encode("Font = \"中文字体\"\n").0;
        let preview = encoding_rs::GB18030.encode("local text = \"设置预览\"\n").0;
        let raw = [label.as_ref(), font.as_ref(), preview.as_ref()].concat();
        assert_eq!(
            super::decode_chinese_ui_script(&raw).unwrap(),
            "local button = \"設定\"\nFont = \"中文字体\"\nlocal text = \"设置预览\"\n"
        );
    }

    #[test]
    fn mixed_chinese_script_keeps_commands_and_decodes_choices() {
        let command = encoding_rs::SHIFT_JIS.encode("? 回想 = 1").0;
        let text = encoding_rs::GB18030.encode("请选择。").0;
        let mut raw = b".select\n".to_vec();
        raw.extend_from_slice(&text);
        raw.extend_from_slice(b"\n.case 1\n");
        raw.extend_from_slice(&command);
        raw.extend_from_slice(b"\n.end\n");
        assert_eq!(
            super::decode_chinese_script(&raw).unwrap(),
            ".select\n请选择。\n.case 1\n? 回想 = 1\n.end\n"
        );
    }

    use super::*;

    fn pack(priority: u32, payload: &[u8]) -> std::io::Cursor<Vec<u8>> {
        let mut bytes = b"NEKOPACK".to_vec();
        for word in [
            1,
            priority,
            0,
            16,
            nekopack_archive::name_hash(1, b"system"),
            1,
            nekopack_archive::name_hash(1, b"config.txt"),
            payload.len() as u32,
            0,
            payload.len() as u32,
        ] {
            bytes.extend(word.to_le_bytes());
        }
        bytes.extend(payload);
        std::io::Cursor::new(bytes)
    }
    #[test]
    fn dat_overlay_lazy_data_missing_and_ambiguity() {
        let mut game =
            DatGame::mount([pack(0, b"value=old"), pack(2, b"; comment\nvalue = new\n")]).unwrap();
        assert_eq!(
            game.resource("Ini", "CONFIG.TXT").unwrap(),
            r#"{["value"]="new"}"#
        );
        assert_eq!(
            game.resource("Data", "config.txt").unwrap(),
            r#"[["value","=","new"]]"#
        );
        assert!(
            game.resource("Ini", "missing.txt")
                .unwrap_err()
                .contains("missing DAT")
        );
        let mut tied = DatGame::mount([pack(2, b"a"), pack(2, b"b")]).unwrap();
        assert!(
            tied.resource("Data", "config.txt")
                .unwrap_err()
                .contains("ambiguous DAT")
        );
        let mut bad = DatGame::mount([pack(0, &[0x81])]).unwrap();
        assert!(
            bad.resource("Data", "config.txt")
                .unwrap_err()
                .contains("invalid CP932")
        );
    }
    #[test]
    fn encoding_and_empty_mount_fail_explicitly() {
        assert!(decode(&[0x81]).is_err());
        assert_eq!(decode(&[0x82, 0xa0]).unwrap(), "あ");
        assert!(DatGame::<std::io::Cursor<Vec<u8>>>::mount([]).is_err());
    }
}
