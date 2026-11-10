//! Portable runtime read history, deliberately distinct from original engine saves.
use std::collections::BTreeSet;
const LIMIT: usize = 8 * 1024 * 1024;
#[derive(Default)]
pub(crate) struct History {
    entries: BTreeSet<Vec<u8>>,
    bytes: usize,
}
pub(crate) fn key(source: &[u8], line: usize, text: &[u8]) -> Vec<u8> {
    let mut key = (source.len() as u64).to_le_bytes().to_vec();
    key.extend(source);
    key.extend((line as u64).to_le_bytes());
    key.extend(text);
    key
}
impl History {
    pub fn contains(&self, key: &[u8]) -> bool {
        self.entries.contains(key)
    }
    pub fn insert(&mut self, key: Vec<u8>) -> Result<(), String> {
        if self.entries.contains(&key) {
            return Ok(());
        }
        if key.len() > 65536 || self.bytes + key.len() + 4 > LIMIT || self.entries.len() >= 100000 {
            return Err("read history limit exceeded".into());
        }
        self.bytes += key.len() + 4;
        self.entries.insert(key);
        Ok(())
    }
    pub fn export(&self) -> Vec<u8> {
        let mut out = b"NKRD\x01".to_vec();
        for key in &self.entries {
            out.extend((key.len() as u32).to_le_bytes());
            out.extend(key)
        }
        out
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > LIMIT + 5 {
            return Err("read history limit exceeded".into());
        }
        let mut bytes = bytes
            .strip_prefix(b"NKRD\x01")
            .ok_or("unknown read history format")?;
        let mut result = Self::default();
        while !bytes.is_empty() {
            let n = u32::from_le_bytes(
                bytes
                    .get(..4)
                    .ok_or("truncated read history")?
                    .try_into()
                    .unwrap(),
            ) as usize;
            bytes = &bytes[4..];
            let key = bytes.get(..n).ok_or("truncated read history key")?;
            if n < 16 {
                return Err("invalid read history key".into());
            }
            let source = u64::from_le_bytes(key[..8].try_into().unwrap());
            if source > (n - 16) as u64 {
                return Err("invalid read history source".into());
            }
            if result.contains(key) {
                return Err("duplicate read history key".into());
            }
            result.insert(key.to_vec())?;
            bytes = &bytes[n..];
        }
        Ok(result)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_roundtrip_and_content_identity() {
        let k = key(b"chapter", 12, b"hello");
        let mut h = History::default();
        h.insert(k.clone()).unwrap();
        h.insert(k.clone()).unwrap();
        let data = h.export();
        let h = History::decode(&data).unwrap();
        assert!(h.contains(&k));
        for other in [
            key(b"chapter", 13, b"hello"),
            key(b"other", 12, b"hello"),
            key(b"chapter", 12, b"patched"),
        ] {
            assert!(!h.contains(&other))
        }
        assert!(History::decode(&data[..data.len() - 1]).is_err());
        let mut duplicate = data.clone();
        duplicate.extend(&data[5..]);
        assert!(History::decode(&duplicate).is_err());
        assert!(History::decode(b"NKRD\x02").is_err());
        let mut h = History::default();
        assert!(h.insert(vec![0; 65537]).is_err());
        assert_eq!(h.export(), b"NKRD\x01");
    }
}
