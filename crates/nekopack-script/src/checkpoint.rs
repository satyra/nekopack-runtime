//! Bounded little-endian checkpoint primitives. No platform pointers or native VM heap.
pub const LIMIT: usize = 16 * 1024 * 1024;
pub fn digest(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325u64, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x100000001b3)
    })
}
#[derive(Default)]
pub struct Writer(pub Vec<u8>);
impl Writer {
    pub fn number(&mut self, value: u64) {
        self.0.extend(value.to_le_bytes());
    }
    pub fn bytes(&mut self, value: &[u8]) {
        self.number(value.len() as u64);
        self.0.extend(value);
    }
    pub fn text(&mut self, value: &str) {
        self.bytes(value.as_bytes());
    }
}
pub struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}
impl<'a> Reader<'a> {
    pub fn new(bytes: &'a [u8]) -> Result<Self, String> {
        if bytes.len() > LIMIT {
            return Err("checkpoint size limit exceeded".into());
        }
        Ok(Self { bytes, at: 0 })
    }
    fn take(&mut self, count: usize) -> Result<&'a [u8], String> {
        let end = self
            .at
            .checked_add(count)
            .ok_or("checkpoint length overflow")?;
        let value = self.bytes.get(self.at..end).ok_or("truncated checkpoint")?;
        self.at = end;
        Ok(value)
    }
    pub fn number(&mut self) -> Result<u64, String> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    pub fn count(&mut self, limit: usize) -> Result<usize, String> {
        let n = usize::try_from(self.number()?).map_err(|_| "checkpoint count overflow")?;
        if n > limit {
            return Err("checkpoint count exceeds limit".into());
        }
        Ok(n)
    }
    pub fn bytes(&mut self) -> Result<&'a [u8], String> {
        let n = self.count(LIMIT)?;
        self.take(n)
    }
    pub fn text(&mut self) -> Result<String, String> {
        String::from_utf8(self.bytes()?.to_vec()).map_err(|_| "invalid checkpoint UTF-8".into())
    }
    pub fn finish(self) -> Result<(), String> {
        if self.at == self.bytes.len() {
            Ok(())
        } else {
            Err("trailing checkpoint data".into())
        }
    }
}
