use nekopack_script::{
    checkpoint::{Reader, Writer},
    runtime::Variables,
};
use std::collections::BTreeSet;

const HEADER: &[u8] = b"NKUL\x01";
const MAX_FLAGS: usize = 4096;

#[derive(Default)]
pub(crate) struct Unlocks(BTreeSet<(u8, u32)>);

impl Unlocks {
    pub(crate) fn capture(&mut self, vars: &Variables) {
        self.0.extend(
            vars.unlock_slots()
                .filter(|slot| vars.slots().get(slot).is_some_and(|value| *value != 0)),
        );
    }

    pub(crate) fn apply(&self, vars: &mut Variables) {
        let allowed: Vec<_> = vars
            .unlock_slots()
            .filter(|slot| self.0.contains(slot))
            .collect();
        for (bank, index) in allowed {
            vars.restore_unlock(bank, index);
        }
    }

    pub(crate) fn encode(&self) -> Vec<u8> {
        let mut out = Writer(HEADER.to_vec());
        for (bank, index) in &self.0 {
            out.number(u64::from(*bank) << 32 | u64::from(*index));
        }
        out.0
    }

    pub(crate) fn decode(bytes: &[u8]) -> Result<Self, String> {
        let payload = bytes
            .strip_prefix(HEADER)
            .ok_or("unknown unlock progress format")?;
        if payload.len() % 8 != 0 || payload.len() / 8 > MAX_FLAGS {
            return Err("invalid unlock progress length".into());
        }
        let mut reader = Reader::new(payload)?;
        let mut flags = BTreeSet::new();
        for _ in 0..payload.len() / 8 {
            let value = reader.number()?;
            let bank = value >> 32;
            if !matches!(bank, 70 | 71) || !flags.insert((bank as u8, value as u32)) {
                return Err("invalid or duplicate unlock flag".into());
            }
        }
        Ok(Self(flags))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restore_merges_unlocks_without_restoring_local_or_unrelated_variables() {
        fn variables() -> Variables {
            let mut vars = Variables::default();
            for (slot, alias) in [
                (b"GF300".as_slice(), b"CG".as_slice()),
                (b"G200", b"_scene"),
                (b"G904", b"clear"),
                (b"G201", b"unrelated"),
                (b"L1", b"choice"),
            ] {
                vars.define_system(slot, alias).unwrap();
            }
            vars
        }
        let mut live = variables();
        live.unlock_visual(b"cg");
        for alias in [b"_scene".as_slice(), b"clear", b"unrelated", b"choice"] {
            live.set(alias, 1).unwrap();
        }
        let mut ledger = Unlocks::default();
        ledger.capture(&live);
        let bytes = ledger.encode();
        let mut older = variables();
        Unlocks::decode(&bytes).unwrap().apply(&mut older);
        assert_eq!(older.get(b"@CG").unwrap(), 1);
        assert_eq!(older.get(b"_scene").unwrap(), 1);
        assert_eq!(older.get(b"clear").unwrap(), 1);
        assert_eq!(older.get(b"unrelated").unwrap(), 0);
        assert_eq!(older.get(b"choice").unwrap(), 0);
        assert!(Unlocks::decode(&bytes[..bytes.len() - 1]).is_err());
    }
}
