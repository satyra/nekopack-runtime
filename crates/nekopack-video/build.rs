use std::{env, path::PathBuf, process::Command};
fn main() {
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let mut c = Command::new(env::var("CXX").unwrap_or_else(|_| "c++".into()));
    c.args(
        env::var("NEKO_CXX_ARGS")
            .unwrap_or_default()
            .split_whitespace(),
    );
    c.args([
        "-c",
        "-O2",
        "-std=c++11",
        "-fexceptions",
        "-fno-rtti",
        "-fPIC",
        "-fwrapv",
        "src/decoder.cpp",
        "-o",
    ])
    .arg(out.join("decoder.o"));
    assert!(c.status().expect("C++ decoder compiler").success());
    let mut ar = Command::new(env::var("AR").unwrap_or_else(|_| "ar".into()));
    ar.args(
        env::var("NEKO_AR_ARGS")
            .unwrap_or_default()
            .split_whitespace(),
    );
    ar.arg("crs")
        .arg(out.join("libneko_video.a"))
        .arg(out.join("decoder.o"));
    assert!(ar.status().expect("decoder archiver").success());
    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-lib=static=neko_video");
    let target = env::var("TARGET").unwrap();
    if target.contains("windows-gnu") {
        let output = Command::new(env::var_os("RUSTC").unwrap())
            .args(["--print", "sysroot"])
            .output()
            .expect("rustc sysroot");
        assert!(output.status.success());
        let sysroot = String::from_utf8(output.stdout).unwrap();
        println!(
            "cargo:rustc-link-search=native={}/lib/rustlib/{target}/lib/self-contained",
            sysroot.trim()
        );
    }
    if target.contains("android") {
        println!("cargo:rustc-link-lib=c++_shared");
    } else if target.contains("apple") {
        println!("cargo:rustc-link-lib=c++");
    } else {
        println!("cargo:rustc-link-lib=static=stdc++");
    }
    println!("cargo:rerun-if-changed=src/decoder.cpp");
    println!("cargo:rerun-if-changed=../../vendor/pl_mpeg/pl_mpeg.h");
    for key in ["CXX", "AR", "NEKO_CXX_ARGS", "NEKO_AR_ARGS"] {
        println!("cargo:rerun-if-env-changed={key}");
    }
}
