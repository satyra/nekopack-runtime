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
