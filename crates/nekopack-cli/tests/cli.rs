use std::process::Command;

#[test]
fn binary_output_metadata_and_error_exit() {
    let path = std::env::temp_dir().join(format!("nekopack-cli-test-{}.dat", std::process::id()));
    let mut bytes = b"NEKOPACK".to_vec();
    for n in [0u32, 301, 0, 16, 7, 1, 9, 3, 0, 3] {
        bytes.extend(n.to_le_bytes());
    }
    bytes.extend([0, 255, 10]);
    std::fs::write(&path, &bytes).unwrap();
    let binary = env!("CARGO_BIN_EXE_nekopack-cli");
    let result = Command::new(binary)
        .arg("read-entry")
        .arg(&path)
        .arg("0")
        .output()
        .unwrap();
    assert!(result.status.success());
    assert_eq!(result.stdout, [0, 255, 10]);
    assert!(result.stderr.is_empty());
    let result = Command::new(binary)
        .arg("inspect-archive")
        .arg(&path)
        .output()
        .unwrap();
    assert!(result.status.success());
    let text = String::from_utf8(result.stdout).unwrap();
    assert!(text.starts_with("archive\t00000000\t301\t16\t1\t0\n"));
    assert!(text.ends_with("0\t00000007\t00000009\t40\t3\t00000000\t00FF0A\n"));
    let result = Command::new(binary)
        .arg("read-index")
        .arg(&path)
        .output()
        .unwrap();
    assert!(result.status.success());
    assert_eq!(result.stdout, bytes[24..40]);
    let result = Command::new(binary)
        .arg("read-entry")
        .arg(&path)
        .arg("1")
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(result.stdout.is_empty());
    assert!(String::from_utf8(result.stderr)
        .unwrap()
        .contains("out of range"));
    std::fs::write(&path, b"bad").unwrap();
    let result = Command::new(binary)
        .arg("inspect-archive")
        .arg(&path)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(result.stdout.is_empty());
    std::fs::remove_file(path).unwrap();
}

#[test]
fn rejects_missing_or_unknown_command() {
    for args in [
        vec![],
        vec!["unknown", "file.dat"],
        vec!["read-entry", "file.dat", "bad"],
    ] {
        let result = Command::new(env!("CARGO_BIN_EXE_nekopack-cli"))
            .args(args)
            .output()
            .unwrap();
        assert!(!result.status.success());
        assert!(result.stdout.is_empty());
        assert!(!result.stderr.is_empty());
    }
}

#[test]
fn names_and_priority_cli_preserve_sources_and_ties() {
    let root = std::env::temp_dir();
    let paths = [
        root.join(format!("neko-base-{}.dat", std::process::id())),
        root.join(format!("neko-patch-{}.dat", std::process::id())),
    ];
    let binary = env!("CARGO_BIN_EXE_nekopack-cli");
    for (path, priority) in paths.iter().zip([0, 301]) {
        let mut bytes = b"NEKOPACK".to_vec();
        for n in [
            7u32,
            priority,
            0,
            16,
            nekopack_archive::name_hash(7, b"script"),
            1,
            nekopack_archive::name_hash(7, b"start.txt"),
            0,
            0,
            0,
        ] {
            bytes.extend(n.to_le_bytes());
        }
        std::fs::write(path, bytes).unwrap();
    }
    let result = Command::new(binary)
        .args(["lookup-name", "736372697074", "73746172742e747874"])
        .args(&paths)
        .output()
        .unwrap();
    assert!(result.status.success());
    assert_eq!(
        result.stdout,
        b"selected\t1\t0\ncandidate\t0\t0\t0\ncandidate\t1\t0\t301\n"
    );
    let result = Command::new(binary)
        .arg("inspect-overlays")
        .args([&paths[1], &paths[1]])
        .output()
        .unwrap();
    assert!(result.status.success());
    assert!(String::from_utf8(result.stdout)
        .unwrap()
        .contains("\tambiguous\t2\n"));
    let result = Command::new(binary)
        .args(["hash-name", "9B75ADA7", "736372697074"])
        .output()
        .unwrap();
    assert!(result.status.success());
    assert_eq!(result.stdout, b"0DDB021E\n");
    for invalid in ["1", "ZZ", "あ"] {
        assert!(!Command::new(binary)
            .args(["hash-name", "7", invalid])
            .output()
            .unwrap()
            .status
            .success());
    }
    for path in paths {
        std::fs::remove_file(path).unwrap();
    }
}

#[test]
fn standalone_mng_metadata_cli_and_crc_failure() {
    // Fixed synthetic MNG generated with Python struct + zlib.crc32.
    let hex = "8a4d4e470d0a1a0a0000001c4d48445200000280000001e00000003c000000000000000000000000000000018d07d8b2000000064652414d016672616d6554e7b8de000000004d454e442120f7d5";
    let mut data: Vec<u8> = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect();
    let path = std::env::temp_dir().join(format!("neko-metadata-{}.mng", std::process::id()));
    std::fs::write(&path, &data).unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_nekopack-cli"))
        .arg("inspect-mng")
        .arg(&path)
        .output()
        .unwrap();
    assert!(result.status.success());
    let text = String::from_utf8(result.stdout).unwrap();
    assert!(text.starts_with("mng\t0\t640\t480\t60\t0\t0\t0\t1\n"));
    assert!(text.contains("\tFRAM\t6\t54E7B8DE\t0\t016672616D65\n"));
    data[20] ^= 1;
    std::fs::write(&path, data).unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_nekopack-cli"))
        .arg("inspect-mng")
        .arg(&path)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(result.stdout.is_empty());
    assert!(String::from_utf8(result.stderr)
        .unwrap()
        .contains("CRC mismatch"));
    std::fs::remove_file(path).unwrap();
}

#[test]
fn standalone_png_raster_protocol_and_failure() {
    let png: &[u8] = &[
        137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 2, 0, 0, 0, 2, 8, 6,
        0, 0, 0, 114, 182, 13, 36, 0, 0, 0, 26, 73, 68, 65, 84, 120, 156, 99, 248, 207, 45, 198,
        192, 148, 242, 159, 145, 129, 61, 34, 185, 225, 255, 191, 191, 255, 1, 52, 37, 7, 194, 222,
        179, 178, 202, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
    ];
    let path = std::env::temp_dir().join(format!("neko-png-{}.png", std::process::id()));
    std::fs::write(&path, png).unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_nekopack-cli"))
        .arg("decode-png")
        .arg(&path)
        .output()
        .unwrap();
    assert!(result.status.success());
    assert_eq!(&result.stdout[..8], b"NRGBA001");
    let fields: Vec<u32> = result.stdout[8..36]
        .as_chunks::<4>()
        .0
        .iter()
        .map(|v| u32::from_le_bytes(*v))
        .collect();
    assert_eq!(fields, [0, 0, 2, 2, u32::MAX, u32::MAX, 16]);
    assert_eq!(
        &result.stdout[36..],
        &[255, 11, 22, 0, 2, 100, 255, 1, 7, 88, 99, 128, 255, 254, 253, 255]
    );
    std::fs::write(&path, b"not PNG").unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_nekopack-cli"))
        .arg("decode-png")
        .arg(&path)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(result.stdout.is_empty());
    std::fs::remove_file(path).unwrap();
}

#[test]
fn opaque_canvas_cli_and_invalid_background() {
    // Synthetic 1x1 MNG/DAT: partially transparent (183,64,54,187).
    let hex = "4e454b4f5041434b000000000000000000000000100000000700000001000000090000008b000000000000008b0000008a4d4e470d0a1a0a0000001c4d4844520000000100000001000000000000000000000000000000000000000b05bb0a7c000000054652414d0242415345a09d624e0000000d49484452000000010000000108060000001f15c4890000000d49444154789c63d8ee60b61b0004c801e9969d58380000000049454e44ae426082000000004d454e442120f7d5";
    let bytes: Vec<u8> = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect();
    let path = std::env::temp_dir().join(format!("neko-canvas-{}.dat", std::process::id()));
    std::fs::write(&path, bytes).unwrap();
    let binary = env!("CARGO_BIN_EXE_nekopack-cli");
    for (command, background, expected) in [
        ("compose-mng", None, [183, 64, 54, 187]),
        ("compose-mng-on", Some("000000"), [134, 47, 40, 255]),
        ("compose-mng-on", Some("FFFFFF"), [202, 115, 108, 255]),
    ] {
        let mut cmd = Command::new(binary);
        cmd.arg(command).arg(&path);
        if let Some(color) = background {
            cmd.arg(color);
        }
        let result = cmd.arg("0:0").output().unwrap();
        assert!(result.status.success(), "{:?}", result.stderr);
        let decoded = nekopack_image::decode_png(&result.stdout, 1024 * 1024).unwrap();
        assert_eq!(decoded.pixels(), expected);
    }
    for bad in ["0", "00000000", "GG0000"] {
        let result = Command::new(binary)
            .arg("compose-mng-on")
            .arg(&path)
            .args([bad, "0:0"])
            .output()
            .unwrap();
        assert!(!result.status.success());
        assert!(result.stdout.is_empty());
    }
    std::fs::remove_file(path).unwrap();
}
