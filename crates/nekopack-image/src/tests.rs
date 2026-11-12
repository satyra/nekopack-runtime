use super::*;
include!("pixel_fixtures.rs");

#[test]
fn sprite_positions_persist_and_names_are_not_deduplicated() {
    let mut extra = chunk(b"FRAM", b"\x02same");
    let mut defi = vec![0, 0, 0, 0];
    defi.extend((-3i32).to_be_bytes());
    defi.extend(7i32.to_be_bytes());
    extra.extend(chunk(b"DEFI", &defi));
    extra.extend(&RGBA[8..]);
    extra.extend(chunk(b"FRAM", b"\x02same"));
    extra.extend(&RGBA[8..]);
    let data = image(&extra);
    let m = Mng::parse(&data, 100).unwrap();
    let sprites = m.sprites().unwrap();
    assert_eq!(sprites.len(), 2);
    assert_eq!(sprites[1].position, [-3, 7]);
    assert_eq!(sprites[0].name, sprites[1].name);
    for control in [chunk(b"DEFI", &[0, 1, 0, 0]), chunk(b"FRAM", b"\x01bad")] {
        let data = image(&control);
        assert!(Mng::parse(&data, 100).unwrap().sprites().is_err());
    }
}

#[test]
fn png_reference_pixels_alpha_palette_gray_and_adam7() {
    for (bytes, expected) in [
        (RGBA, RGBA_RGBA),
        (RGB, RGB_RGBA),
        (GRAY, GRAY_RGBA),
        (GRAY_ALPHA, GRAY_ALPHA_RGBA),
        (PALETTE, PALETTE_RGBA),
        (ADAM7, ADAM7_RGBA),
    ] {
        let image = decode_png(bytes, 1024 * 1024).unwrap();
        assert_eq!(image.pixels(), expected);
        let mut exported = Vec::new();
        image.write_png(&mut exported).unwrap();
        assert_eq!(decode_png(&exported, 1024 * 1024).unwrap(), image);
    }
}

#[test]
fn png_truncation_crc_budget_and_sixteen_bit_rejection() {
    for end in 0..RGBA.len() {
        assert!(decode_png(&RGBA[..end], 1024 * 1024).is_err(), "end {end}");
    }
    assert!(decode_png(RGBA, 15).is_err());
    let mut bad = RGBA.to_vec();
    bad[45] ^= 1;
    assert!(decode_png(&bad, 1024 * 1024).is_err());
    let mut encoder_data = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut encoder_data, 1, 1);
        encoder.set_depth(png::BitDepth::Sixteen);
        encoder.set_color(png::ColorType::Grayscale);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&[0x12, 0x34]).unwrap();
        writer.finish().unwrap();
    }
    assert!(matches!(
        decode_png(&encoder_data, 1024 * 1024),
        Err(PixelError::Unsupported(_))
    ));
}

#[test]
fn embedded_pixels_keep_control_chunk_provenance() {
    let mut extra = chunk(b"FRAM", b"\x02named");
    extra.extend(chunk(b"DEFI", &[0, 0, 0, 0]));
    extra.extend(&RGBA[8..]);
    extra.extend(chunk(b"FRAM", b"\x02other"));
    extra.extend(&RGB[8..]);
    let data = image(&extra);
    let m = Mng::parse(&data, 100).unwrap();
    let sources = m.embedded_images().unwrap();
    assert_eq!(sources.len(), 2);
    assert_eq!(sources[0].preceding_fram, Some(1));
    assert_eq!(sources[0].preceding_defi, Some(2));
    assert_eq!(sources[1].preceding_defi, Some(2));
    assert_ne!(sources[0].preceding_fram, sources[1].preceding_fram);
    assert_eq!(m.decode_image(0, 1024 * 1024).unwrap().pixels(), RGBA_RGBA);
    assert_eq!(m.decode_image(1, 1024 * 1024).unwrap().pixels(), RGB_RGBA);
    assert!(matches!(
        m.decode_image(2, 1024 * 1024),
        Err(PixelError::ImageOrdinal(2))
    ));
}

#[test]
fn pixel_extraction_rejects_unimplemented_dependencies() {
    for prefix in [
        chunk(b"PLTE", &[0, 0, 0]),
        chunk(b"tRNS", &[0]),
        chunk(b"ABCD", &[]),
    ] {
        let mut extra = prefix;
        extra.extend(&RGBA[8..]);
        let data = image(&extra);
        assert!(Mng::parse(&data, 100).unwrap().embedded_images().is_err());
    }
    let mut ihdr = RGBA[16..29].to_vec();
    ihdr[11] = 64;
    let mut extra = chunk(b"IHDR", &ihdr);
    extra.extend(&RGBA[33..]);
    let data = image(&extra);
    assert!(Mng::parse(&data, 100).unwrap().embedded_images().is_err());
}
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


#[test]
fn localized_static_frames_keep_names_and_validate_the_original_crc() {
    let mut extra = chunk(b"FRAM", b"\x02BASE\0ASE\0editor trailer");
    extra.extend(chunk(b"DEFI", &[0; 12]));
    extra.extend(&RGBA[8..]);
    let raw = image(&extra);
    assert!(Mng::parse(&raw, 30).is_err());
    let normalized = crate::normalize_localized_ui_frames(&raw, Some([1280, 720])).unwrap();
    let mng = Mng::parse(&normalized, 30).unwrap();
    assert_eq!(&mng.header[..2], &[1280, 720]);
    assert_eq!(mng.sprites().unwrap()[0].name, b"BASE");
    let mut broken = raw;
    let name = broken.windows(4).position(|part| part == b"BASE").unwrap();
    broken[name] ^= 1;
    assert!(crate::normalize_localized_ui_frames(&broken, None).is_err());
}
