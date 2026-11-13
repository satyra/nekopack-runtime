use std::{env, path::PathBuf, process::Command};
fn main() {
    println!("cargo:rustc-check-cfg=cfg(neko_ffmpeg)");
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
    let ffmpeg = env::var_os("NEKO_FFMPEG_DIR").map(PathBuf::from);
    if let Some(root) = &ffmpeg {
        let mut compiler = Command::new(env::var("CXX").unwrap_or_else(|_| "c++".into()));
        compiler.args(
            env::var("NEKO_CXX_ARGS")
                .unwrap_or_default()
                .split_whitespace(),
        );
        compiler
            .args([
                "-c",
                "-O2",
                "-std=c++11",
                "-fexceptions",
                "-fno-rtti",
                "-fPIC",
                "src/asf_decoder.cpp",
                "-o",
            ])
            .arg(out.join("asf_decoder.o"))
            .arg("-I")
            .arg(root.join("include"));
        assert!(compiler.status().expect("FFmpeg bridge compiler").success());
        println!("cargo:rustc-cfg=neko_ffmpeg");
    }
    let mut ar = Command::new(env::var("AR").unwrap_or_else(|_| "ar".into()));
    ar.args(
        env::var("NEKO_AR_ARGS")
            .unwrap_or_default()
            .split_whitespace(),
    );
    ar.arg("crs")
        .arg(out.join("libneko_video.a"))
        .arg(out.join("decoder.o"));
    if ffmpeg.is_some() {
        ar.arg(out.join("asf_decoder.o"));
    }
    assert!(ar.status().expect("decoder archiver").success());
    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-lib=static=neko_video");
    if let Some(root) = ffmpeg {
        println!(
            "cargo:rustc-link-search=native={}",
            root.join("lib").display()
        );
        for library in ["avformat", "avcodec", "swscale", "swresample", "avutil"] {
            println!("cargo:rustc-link-lib=dylib={library}");
        }
    }
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
    println!("cargo:rerun-if-changed=src/asf_decoder.cpp");
    println!("cargo:rerun-if-changed=../../vendor/pl_mpeg/pl_mpeg.h");
    for key in [
        "CXX",
        "AR",
        "NEKO_CXX_ARGS",
        "NEKO_AR_ARGS",
        "NEKO_FFMPEG_DIR",
    ] {
        println!("cargo:rerun-if-env-changed={key}");
    }
}
