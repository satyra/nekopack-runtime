//! Static provenance analysis. Highest-priority selection is an explicit policy,
//! not a claim that original-engine directory enumeration order is known.
use nekopack_archive::{name_hash, Entry, Header};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Candidate {
    pub source: usize,
    pub ordinal: usize,
    pub priority: u32,
}
#[derive(Debug, PartialEq, Eq)]
pub enum Resolution {
    Missing,
    Selected(Candidate),
    Ambiguous(Vec<Candidate>),
}
/// Equal priority remains ambiguous, including repeated hashes within one pack.
pub fn resolve(candidates: &[Candidate]) -> Resolution {
    let Some(priority) = candidates.iter().map(|c| c.priority).max() else {
        return Resolution::Missing;
    };
    let highest: Vec<_> = candidates
        .iter()
        .copied()
        .filter(|c| c.priority == priority)
        .collect();
    if highest.len() == 1 {
        Resolution::Selected(highest[0])
    } else {
        Resolution::Ambiguous(highest)
    }
}
#[derive(Default)]
pub struct Catalog {
    packs: Vec<(Header, Vec<Entry>)>,
}
pub type HashKey = (u32, u32, u32); // seed, directory hash, filename hash
impl Catalog {
    /// Source IDs are insertion ordinals; entry ordinals always refer to the original pack.
    pub fn add(&mut self, header: Header, entries: &[Entry]) -> usize {
        let source = self.packs.len();
        self.packs.push((header, entries.to_vec()));
        source
    }
    /// Hash the same encoded logical names with each pack's own seed.
    /// Returned candidates are hash matches, not proof of collision-free name recovery.
    pub fn lookup(&self, directory: &[u8], filename: &[u8]) -> Vec<Candidate> {
        let mut result = Vec::new();
        for (source, (header, entries)) in self.packs.iter().enumerate() {
            let dir = name_hash(header.name_seed, directory);
            let file = name_hash(header.name_seed, filename);
            for (ordinal, entry) in entries.iter().enumerate() {
                if entry.directory_hash == dir && entry.name_hash == file {
                    result.push(Candidate {
                        source,
                        ordinal,
                        priority: header.priority,
                    });
                }
            }
        }
        result
    }
    /// Unknown hashes are grouped only within the same seed namespace.
    /// Cross-seed relationships require lookup with actual candidate name bytes.
    pub fn overlaps(&self) -> BTreeMap<HashKey, Vec<Candidate>> {
        let mut groups: BTreeMap<_, Vec<_>> = BTreeMap::new();
        for (source, (header, entries)) in self.packs.iter().enumerate() {
            for (ordinal, entry) in entries.iter().enumerate() {
                groups
                    .entry((header.name_seed, entry.directory_hash, entry.name_hash))
                    .or_default()
                    .push(Candidate {
                        source,
                        ordinal,
                        priority: header.priority,
                    });
            }
        }
        groups.retain(|_, candidates| candidates.len() > 1);
        groups
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn add(c: &mut Catalog, seed: u32, priority: u32) {
        c.add(
            Header {
                name_seed: seed,
                priority,
                index_key: 0,
                index_length: 16,
            },
            &[Entry {
                directory_hash: name_hash(seed, b"script"),
                name_hash: name_hash(seed, b"start.txt"),
                block_offset: 40,
                length: 0,
                key: 0,
            }],
        );
    }
    #[test]
    fn priority_ties_and_cross_seed_lookup() {
        let mut c = Catalog::default();
        assert_eq!(
            resolve(&c.lookup(b"script", b"start.txt")),
            Resolution::Missing
        );
        add(&mut c, 1, 0);
        add(&mut c, 1, 301);
        add(&mut c, 2, 0);
        assert_eq!(c.overlaps().len(), 1);
        let candidates = c.lookup(b"SCRIPT", b"START.TXT");
        assert_eq!(candidates.len(), 3);
        assert_eq!(
            resolve(&candidates),
            Resolution::Selected(Candidate {
                source: 1,
                ordinal: 0,
                priority: 301
            })
        );
        add(&mut c, 1, 301);
        assert!(
            matches!(resolve(&c.lookup(b"script", b"start.txt")), Resolution::Ambiguous(v) if v.len() == 2)
        );
    }
    #[test]
    fn equal_priority_never_depends_on_insertion_order() {
        let mut c = Catalog::default();
        add(&mut c, 1, 0);
        add(&mut c, 1, 0);
        let mut candidates = c.lookup(b"script", b"start.txt");
        assert!(matches!(resolve(&candidates), Resolution::Ambiguous(_)));
        candidates.reverse();
        assert!(matches!(resolve(&candidates), Resolution::Ambiguous(_)));
    }
}
