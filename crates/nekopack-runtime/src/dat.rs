use crate::{quote, GameResources};
use nekopack_archive::Archive;
use nekopack_image::{Metadata, Mng};
use nekopack_resource::{resolve, Catalog, Resolution};
use nekopack_script::{
    runtime::{Scripts, Source},
    system::ResourceProvider,
};
use std::{
    collections::BTreeMap,
    io::{Read, Seek},
};
/// Explicit four-title CP932-compatible profile; never guesses a game's encoding.
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
        let mut archives = Vec::new();
        let mut catalog = Catalog::default();
        for source in sources {
            let archive = Archive::open(source, 16 * 1024 * 1024).map_err(|e| e.to_string())?;
            catalog.add(archive.header(), archive.entries());
            archives.push(archive);
        }
        if archives.is_empty() {
            return Err("no DAT archives selected".into());
        }
        Ok(Self { archives, catalog })
    }
    pub(crate) fn read(&mut self, dir: &str, name: &str) -> Result<Vec<u8>, String> {
        if name.contains(['/', '\\', '\0']) || name == ".." {
            return Err("resource name is not a basename".into());
        }
        let (encoded, _, errors) = encoding_rs::SHIFT_JIS.encode(name);
        if errors {
            return Err("resource name cannot be encoded in CP932 profile".into());
        }
        let candidate = match resolve(&self.catalog.lookup(dir.as_bytes(), &encoded)) {
            Resolution::Selected(c) => c,
            Resolution::Missing => return Err(format!("missing DAT resource {dir}/{name}")),
            Resolution::Ambiguous(_) => return Err(format!("ambiguous DAT resource {dir}/{name}")),
        };
        let archive = &mut self.archives[candidate.source];
        let length = archive.entries()[candidate.ordinal].length as usize;
        if length > 64 * 1024 * 1024 {
            return Err("resource exceeds 64 MiB limit".into());
        }
        archive
            .read_prefix(candidate.ordinal, length)
            .map_err(|e| e.to_string())
    }
}
impl<R: Read + Seek> Scripts for DatGame<R> {
    fn load(&mut self, name: &[u8]) -> Result<Source, String> {
        let text = std::str::from_utf8(name).map_err(|e| e.to_string())?;
        Ok(Source {
            name: name.to_vec(),
            bytes: decode(&self.read("script", &format!("{text}.txt"))?)?.into_bytes(),
        })
    }
}
impl<R: Read + Seek> GameResources for DatGame<R> {
    fn setup(&self) -> Result<String, String> {
        Ok(include_str!("../host/dat.nut").into())
    }
    fn system(&mut self, name: &[u8]) -> Result<Vec<u8>, String> {
        let name = std::str::from_utf8(name).map_err(|e| e.to_string())?;
        Ok(decode(&self.read("system", name)?)?.into_bytes())
    }
}
impl<R: Read + Seek> ResourceProvider for DatGame<R> {
    fn resource(&mut self, kind: &str, name: &str) -> Result<String, String> {
        match kind {
            "Data" | "Ini" => {
                let text = decode(&self.read("system", name)?)?;
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
            "Assets" => {
                let file = format!("{name}.mng");
                let bytes = self.read("system", &file)?;
                let mng = Mng::parse(&bytes, 100000).map_err(|e| e.to_string())?;
                let mut label = String::new();
                let mut position = [0, 0];
                let mut ordinal = 0;
                let mut entries = BTreeMap::new();
                for chunk in mng.chunks {
                    match chunk.metadata {
                        Metadata::Frame(frame) => label = decode(frame.name)?,
                        Metadata::Definition(def) => {
                            if let Some(pos) = def.position {
                                position = pos
                            }
                        }
                        Metadata::ImageHeader { width, height, .. } => {
                            entries.insert(
                                label.clone(),
                                format!(
                                    "[{},{},{},{},{}]",
                                    quote(format!("system/{file}:{ordinal}").as_bytes())?,
                                    position[0],
                                    position[1],
                                    width,
                                    height
                                ),
                            );
                            ordinal += 1;
                        }
                        _ => {}
                    }
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
        assert!(game
            .resource("Ini", "missing.txt")
            .unwrap_err()
            .contains("missing DAT"));
        let mut tied = DatGame::mount([pack(2, b"a"), pack(2, b"b")]).unwrap();
        assert!(tied
            .resource("Data", "config.txt")
            .unwrap_err()
            .contains("ambiguous DAT"));
        let mut bad = DatGame::mount([pack(0, &[0x81])]).unwrap();
        assert!(bad
            .resource("Data", "config.txt")
            .unwrap_err()
            .contains("invalid CP932"));
    }
    #[test]
    fn encoding_and_empty_mount_fail_explicitly() {
        assert!(decode(&[0x81]).is_err());
        assert_eq!(decode(&[0x82, 0xa0]).unwrap(), "あ");
        assert!(DatGame::<std::io::Cursor<Vec<u8>>>::mount([]).is_err());
    }
}
