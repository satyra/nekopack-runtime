use super::*;
use std::io::Cursor;

#[test]
fn name_hash_reference_vectors_and_byte_semantics() {
    assert_eq!(name_hash(0x9B75ADA7, b"script"), 0x0DDB021E);
    assert_eq!(name_hash(0x9B75ADA7, b"start.bin"), 0xCB8FB53B);
    assert_eq!(name_hash(7, b""), 7);
    assert_eq!(name_hash(7, b"SCRIPT"), name_hash(7, b"script"));
    assert_eq!(name_hash(7, b"image\\actor"), name_hash(7, b"image/actor"));
    assert_ne!(name_hash(7, &[0x82, 0xA0]), name_hash(7, "あ".as_bytes()));
}

fn hex(s: &str) -> Vec<u8> {
    s.as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|v| u8::from_str_radix(std::str::from_utf8(v).unwrap(), 16).unwrap())
        .collect()
}
fn put(data: &mut Vec<u8>, n: u32) {
    data.extend(n.to_le_bytes());
}
fn fixture(payloads: &[&[u8]]) -> Vec<u8> {
    let mut data = b"NEKOPACK".to_vec();
    for n in [
        0,
        301,
        0,
        8 + payloads.len() as u32 * 8,
        7,
        payloads.len() as u32,
    ] {
        put(&mut data, n);
    }
    for payload in payloads {
        put(&mut data, 9);
        put(&mut data, payload.len() as u32);
    }
    for payload in payloads {
        put(&mut data, 0);
        put(&mut data, payload.len() as u32);
        data.extend(*payload);
    }
    data
}
fn open(data: Vec<u8>) -> Result<Archive<Cursor<Vec<u8>>>> {
    Archive::open(Cursor::new(data), 4096)
}

#[test]
fn golden_cipher_vectors_and_every_split() {
    // Fixed outputs from the existing GARbro-derived Python u64 decoder.
    // These are synthetic bytes, not game resources; no round-trip oracle.
    for (key, expected) in [
        (
            1,
            "f3034c78e2d3097fee0f90f8c4a716f8c40738ff984738f8800778f0388778f838",
        ),
        (
            u32::MAX,
            "3985045a8ebfcd8a7a0000b814779617fc18187838e43827f03b38f878c87848f8",
        ),
        (
            0x12345678,
            "6adaf9ae3ee7caa7dcbcfe5774c49847a063e0a7f898388748cfc847f83b7808b8",
        ),
    ] {
        let expected = hex(expected);
        for size in 0..=33 {
            for split in 0..=size {
                let mut data: Vec<u8> = (0..size as u8).collect();
                let mut decoder = Decoder::new(key);
                decoder.apply(&mut data[..split]);
                decoder.apply(&mut data[split..]);
                assert_eq!(
                    data,
                    expected[..size],
                    "key={key} size={size} split={split}"
                );
            }
        }
    }
}

#[test]
fn zero_key_is_plaintext() {
    let mut data = vec![0, 255, 13];
    Decoder::new(0).apply(&mut data);
    assert_eq!(data, [0, 255, 13]);
}

#[test]
fn encrypted_index_and_odd_payload_golden_archive() {
    // Synthetic archive prepared with the external u64 reference implementation.
    let data = hex("4e454b4f5041434b000000002d010000c237e92d10000000bbbc276b5f2acda1cabc276b7e2acda1e237e92d21000000000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20");
    let mut archive = open(data).unwrap();
    assert_eq!(archive.entries()[0].directory_hash, 7);
    assert_eq!(archive.entries()[0].name_hash, 9);
    let expected = hex("9cbd25681a2fcba6307346d83454964778fc80b878b83887f8f0087ff8787808f8");
    let mut actual = Vec::new();
    archive.copy_entry(0, &mut actual).unwrap();
    assert_eq!(actual, expected);
    assert_eq!(archive.read_prefix(0, 7).unwrap(), expected[..7]);
}

#[test]
fn parity_golden_overflow_and_rotation() {
    assert_eq!(parity(0, 0), 0x2DE937C2);
    assert_eq!(parity(0x24315C66, 17), 0x1FE0088E);
    assert_eq!(parity(u32::MAX, u32::MAX), 0xC05F293E);
    assert_eq!(parity(0x4B0EC5B3, 16), 0x564BDB7B);
    let seed = 0x359D3E2Au32.wrapping_sub(0x5D588B65);
    assert_eq!(
        parity(seed, 0),
        0u32.wrapping_sub(0x70E44324).wrapping_add(0x6C078965)
    );
}

#[test]
fn random_access_duplicate_hashes_and_odd_boundaries() {
    let mut archive = open(fixture(&[b"abc", b"de", b""])).unwrap();
    assert_eq!(archive.header().priority, 301);
    assert_eq!(archive.entries().len(), 3);
    assert_eq!(
        archive.entries()[1].block_offset,
        archive.entries()[0].block_offset + 11
    );
    assert_eq!(
        archive.entries()[0].name_hash,
        archive.entries()[1].name_hash
    );
    assert_eq!(archive.read_prefix(1, 100).unwrap(), b"de");
    let mut output = Vec::new();
    assert_eq!(archive.copy_entry(0, &mut output).unwrap(), 3);
    assert_eq!(output, b"abc");
    assert_eq!(archive.copy_entry(2, &mut output).unwrap(), 0);
    assert!(matches!(
        archive.copy_entry(3, &mut output),
        Err(Error::EntryOutOfRange(3))
    ));
}

#[test]
fn rejects_every_truncation() {
    let data = fixture(&[b"abc", b"de"]);
    for size in 0..data.len() {
        assert!(open(data[..size].to_vec()).is_err(), "size {size}");
    }
}

#[test]
fn invalid_signature_index_length_and_budget() {
    let original = fixture(&[b"abc"]);
    let mut bad = original.clone();
    bad[0] = 0;
    assert!(open(bad).is_err());
    for length in [0u32, 7, 9, u32::MAX] {
        let mut bad = original.clone();
        bad[20..24].copy_from_slice(&length.to_le_bytes());
        assert!(open(bad).is_err());
    }
    assert!(Archive::open(Cursor::new(original), 15).is_err());
}

#[test]
fn directory_count_and_block_length_mismatch() {
    let mut bad = fixture(&[b"abc"]);
    bad[28..32].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(open(bad).is_err());
    let mut bad = fixture(&[b"abc"]);
    bad[44..48].copy_from_slice(&4u32.to_le_bytes());
    assert!(open(bad).is_err());
}

#[test]
fn invalid_nonzero_keys_are_errors() {
    for offset in [16, 40] {
        let mut bad = fixture(&[b"abc"]);
        bad[offset..offset + 4].copy_from_slice(&1u32.to_le_bytes());
        assert!(open(bad).is_err());
    }
}

#[test]
fn reports_trailing_data_without_discarding_entries() {
    let mut data = fixture(&[b"a"]);
    data.extend([1, 2, 3]);
    let archive = open(data).unwrap();
    assert_eq!(archive.trailing_bytes(), 3);
    assert_eq!(archive.entries().len(), 1);
}

#[test]
fn streams_across_buffer_boundary_and_propagates_write_error() {
    let payload: Vec<u8> = (0..131_075).map(|i| i as u8).collect();
    let mut archive = open(fixture(&[&payload])).unwrap();
    let mut result = Vec::new();
    archive.copy_entry(0, &mut result).unwrap();
    assert_eq!(result, payload);
    struct Fail;
    impl Write for Fail {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::other("test sink"))
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    assert!(matches!(
        archive.copy_entry(0, &mut Fail),
        Err(Error::Io(_))
    ));
}

// Sparse source avoids allocating a >4 GiB synthetic payload.
struct Sparse {
    segments: Vec<(u64, Vec<u8>)>,
    position: u64,
    length: u64,
}
impl Read for Sparse {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        let count = (self.length - self.position).min(out.len() as u64) as usize;
        out[..count].fill(0);
        for (start, bytes) in &self.segments {
            let lo = self.position.max(*start);
            let hi = (self.position + count as u64).min(start + bytes.len() as u64);
            if lo < hi {
                out[(lo - self.position) as usize..(hi - self.position) as usize]
                    .copy_from_slice(&bytes[(lo - start) as usize..(hi - start) as usize]);
            }
        }
        self.position += count as u64;
        Ok(count)
    }
}
impl Seek for Sparse {
    fn seek(&mut self, seek: SeekFrom) -> io::Result<u64> {
        self.position = match seek {
            SeekFrom::Start(n) => n,
            SeekFrom::End(n) => self.length.checked_add_signed(n).unwrap(),
            SeekFrom::Current(n) => self.position.checked_add_signed(n).unwrap(),
        };
        Ok(self.position)
    }
}

#[test]
fn offsets_above_four_gib() {
    let mut first = fixture(&[b"", b""]);
    first.truncate(56); // header, index, first block header
    first[36..40].copy_from_slice(&u32::MAX.to_le_bytes());
    first[52..56].copy_from_slice(&u32::MAX.to_le_bytes());
    let next = 56 + u64::from(u32::MAX);
    let source = Sparse {
        segments: vec![(0, first), (next, vec![0; 8])],
        position: 0,
        length: next + 8,
    };
    let mut archive = Archive::open(source, 4096).unwrap();
    assert_eq!(archive.entries()[1].block_offset, next);
    assert!(archive.entries()[1].block_offset > u64::from(u32::MAX));
    assert_eq!(archive.read_prefix(1, 16).unwrap(), []);
    assert_eq!(archive.trailing_bytes(), 0);
}
