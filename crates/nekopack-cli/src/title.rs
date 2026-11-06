//! Headless execution adapter for explicitly decoded UTF-8 sample trees.
use nekopack_script::runtime::{Scripts, Source};
use std::{
    error::Error,
    io::Write,
    path::{Path, PathBuf},
};
struct Store {
    root: PathBuf,
}
impl Store {
    fn read(&self, dir: &str, name: &[u8], extension: &str) -> Result<Vec<u8>, String> {
        let name = std::str::from_utf8(name).map_err(|_| "invalid resource name")?;
        if name.contains(['/', '\\', '\0']) || name == ".." {
            return Err("resource name is not a basename".into());
        }
        // Decoded fixtures retain on-disk spelling; scenario resource names use
        // ASCII-insensitive lookup on every host, including case-sensitive Android.
        let wanted = format!("{name}{extension}");
        let mut matches = std::fs::read_dir(self.root.join(dir))
            .map_err(|e| e.to_string())?
            .filter_map(|entry| match entry {
                Ok(entry)
                    if entry
                        .file_name()
                        .to_str()
                        .is_some_and(|n| n.eq_ignore_ascii_case(&wanted)) =>
                {
                    Some(Ok(entry.path()))
                }
                Ok(_) => None,
                Err(error) => Some(Err(error.to_string())),
            });
        let path = matches
            .next()
            .ok_or_else(|| format!("missing fixture resource {wanted}"))??;
        if matches.next().transpose()?.is_some() {
            return Err(format!("case-ambiguous fixture resource {wanted}"));
        }
        let data = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        if data.len() > 64 * 1024 * 1024 {
            return Err("script exceeds 64 MiB budget".into());
        }
        Ok(data)
    }
}

impl Scripts for Store {
    fn load(&mut self, name: &[u8]) -> Result<Source, String> {
        Ok(Source {
            name: name.to_vec(),
            bytes: self.read("script", name, ".txt")?,
        })
    }
}
pub fn run(root: &Path, choice: i32) -> Result<(), Box<dyn Error>> {
    run_to(root, choice, &mut std::io::stdout().lock())
}

pub fn run_to(root: &Path, choice: i32, output: &mut impl Write) -> Result<(), Box<dyn Error>> {
    nekopack_runtime::run_title(
        &mut Store {
            root: root.to_owned(),
        },
        choice,
        output,
    )
}
impl nekopack_script::system::ResourceProvider for Store {
    fn resource(&mut self, _kind: &str, _name: &str) -> Result<String, String> {
        Err("fixture does not use resource callbacks".into())
    }
}
impl nekopack_runtime::GameResources for Store {
    fn setup(&self) -> Result<String, String> {
        std::fs::read_to_string(self.root.join("assets.nut")).map_err(|e| e.to_string())
    }
    fn system(&mut self, name: &[u8]) -> Result<Vec<u8>, String> {
        self.read("system", name, "")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fixture_lookup_is_ascii_case_insensitive() {
        let root = std::env::temp_dir().join(format!(
            "neko-fixture-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("script")).unwrap();
        std::fs::write(root.join("script/p01.txt"), b".done").unwrap();
        let mut store = Store { root: root.clone() };
        assert_eq!(store.load(b"P01").unwrap().bytes, b".done");
        assert!(store.load(b"missing").is_err());
        #[cfg(unix)]
        {
            std::fs::write(root.join("script/P01.txt"), b"other").unwrap();
            assert!(store.load(b"P01").unwrap_err().contains("case-ambiguous"));
        }
        std::fs::remove_dir_all(root).unwrap();
    }
}
