use super::*;
fn chunk(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut v = (body.len() as u32).to_be_bytes().to_vec();
    v.extend(kind);
    v.extend(body);
    v.extend(crc32(&v[4..]).to_be_bytes());
    v
}
fn image(extra: &[u8]) -> Vec<u8> {
    let mut v = b"\x8aMNG\r\n\x1a\n".to_vec();
    let h: Vec<u8> = [640u32, 480, 60, 0, 0, 0, 1]
        .iter()
        .flat_map(|n| n.to_be_bytes())
        .collect();
    v.extend(chunk(b"MHDR", &h));
    v.extend(extra);
    v.extend(chunk(b"MEND", &[]));
    v
}
#[test]
fn crc_standard_check_vector() {
    assert_eq!(crc32(b"123456789"), 0xCBF43926);
}
#[test]
fn metadata_raw_bytes_and_unknown_critical() {
    let mut extra = chunk(b"tEXt", b"name\0\x82\xa0\xff");
    extra.extend(chunk(b"ABCD", &[7, 8]));
    let data = image(&extra);
    let m = Mng::parse(&data, 100).unwrap();
    assert_eq!(m.header[..3], [640, 480, 60]);
    assert_eq!(
        m.chunks[1].metadata,
        Metadata::Text {
            keyword: b"name",
            text: b"\x82\xa0\xff"
        }
    );
    assert_eq!(m.chunks[2].metadata, Metadata::Unknown { critical: true });
    assert_eq!(m.chunks[2].data, [7, 8]);
}
#[test]
fn fram_optional_fields_and_signed_coordinates() {
    let mut body = vec![2, b'a', 0, 2, 1, 1, 2];
    body.extend(60u32.to_be_bytes());
    body.extend(100u32.to_be_bytes());
    body.push(1);
    for n in [-10i32, 640, -20, 480] {
        body.extend(n.to_be_bytes());
    }
    body.extend(7u32.to_be_bytes());
    let data = image(&chunk(b"FRAM", &body));
    let m = Mng::parse(&data, 10).unwrap();
    let Metadata::Frame(f) = &m.chunks[1].metadata else {
        panic!()
    };
    assert_eq!(f.delay, Some(60));
    assert_eq!(f.timeout, Some(100));
    assert_eq!(f.clipping, Some((1, [-10, 640, -20, 480])));
    assert_eq!(f.sync_ids, Some(vec![7]));
    assert_eq!(f.name, b"a");
    for body in [&b""[..], &b"\x01"[..], &b"\x01name"[..]] {
        let data = image(&chunk(b"FRAM", body));
        assert!(Mng::parse(&data, 10).is_ok());
    }
    let data = image(&chunk(b"FRAM", &[1, 0, 0, 0, 0, 2]));
    let m = Mng::parse(&data, 10).unwrap();
    assert!(matches!(&m.chunks[1].metadata, Metadata::Frame(f) if f.sync_ids == Some(vec![])));
}
#[test]
fn defi_signed_and_term() {
    let mut body = vec![0, 7, 1, 0];
    body.extend((-5i32).to_be_bytes());
    body.extend(9i32.to_be_bytes());
    let mut extra = chunk(b"DEFI", &body);
    extra.extend(chunk(b"TERM", &[3, 1, 0, 0, 0, 60, 0, 0, 0, 4]));
    let data = image(&extra);
    let m = Mng::parse(&data, 10).unwrap();
    assert!(matches!(&m.chunks[1].metadata, Metadata::Definition(d) if d.position == Some([-5,9])));
    assert_eq!(
        m.chunks[2].metadata,
        Metadata::Termination {
            action: 3,
            repeat: Some((1, 60, 4))
        }
    );
}
#[test]
fn rejects_truncation_crc_and_chunk_budget() {
    let data = image(&[]);
    for end in 0..data.len() {
        assert!(Mng::parse(&data[..end], 100).is_err());
    }
    let mut bad = data.clone();
    bad[20] ^= 1;
    assert!(Mng::parse(&bad, 10).is_err());
    assert!(Mng::parse(&data, 1).is_err());
    let mut bad = data;
    bad.extend([0]);
    assert!(Mng::parse(&bad, 10).is_err());
}
#[test]
fn rejects_malformed_known_metadata() {
    for (kind, body) in [
        (b"FRAM", &b"\x01\0\x02\0\0\0\x01"[..]),
        (b"DEFI", &b"12345"[..]),
        (b"TERM", &b"\x03"[..]),
        (b"tEXt", &b"no-separator"[..]),
        (b"IHDR", &b"bad"[..]),
    ] {
        let data = image(&chunk(kind, body));
        assert!(Mng::parse(&data, 10).is_err());
    }
}
#[test]
fn requires_header_first_and_balanced_images() {
    let mut bad = b"\x8aMNG\r\n\x1a\n".to_vec();
    bad.extend(chunk(b"MEND", &[]));
    assert!(Mng::parse(&bad, 10).is_err());
    for kind in [b"IDAT", b"IEND"] {
        assert!(Mng::parse(&image(&chunk(kind, &[])), 10).is_err());
    }
    let mut ihdr = vec![];
    ihdr.extend(1u32.to_be_bytes());
    ihdr.extend(1u32.to_be_bytes());
    ihdr.extend([8, 6, 0, 0, 0]);
    assert!(Mng::parse(&image(&chunk(b"IHDR", &ihdr)), 10).is_err());
    let mut extra = chunk(b"IHDR", &ihdr);
    extra.extend(chunk(b"IDAT", &[1]));
    extra.extend(chunk(b"IEND", &[]));
    assert!(Mng::parse(&image(&extra), 10).is_ok()); // metadata only, not a zlib validator
}
