use std::{env, path::PathBuf, process::Command};
fn run(mut command: Command) {
    let status = command.status().expect("launch C++ toolchain");
    assert!(status.success(), "C++ toolchain failed: {command:?}");
}
fn main() {
    let root = PathBuf::from("../../vendor/squirrel2");
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let compiler = env::var("CXX").unwrap_or_else(|_| "c++".into());
    let archiver = env::var("AR").unwrap_or_else(|_| "ar".into());
    let mut objects = Vec::new();
    for name in [
        "sqapi",
        "sqbaselib",
        "sqfuncstate",
        "sqdebug",
        "sqlexer",
        "sqobject",
        "sqcompiler",
        "sqstate",
        "sqtable",
        "sqmem",
        "sqvm",
        "sqclass",
        "sqstdstring",
        "sqstdrex",
        "bridge",
    ] {
        let source = if name == "bridge" {
            PathBuf::from("src/squirrel_bridge.cpp")
        } else {
            root.join(if name.starts_with("sqstd") {
                "sqstdlib"
            } else {
                "squirrel"
            })
            .join(format!("{name}.cpp"))
        };
        let object = out.join(format!("{name}.o"));
        let mut command = Command::new(&compiler);
        command.args(
            env::var("NEKO_CXX_ARGS")
                .unwrap_or_default()
                .split_whitespace(),
        );
        command
            .args([
                "-c",
                "-O1",
                "-std=c++11",
                "-fno-exceptions",
                "-fno-rtti",
                "-w",
            ])
            .arg("-I")
            .arg(root.join("include"))
            .arg(source)
            .arg("-o")
            .arg(&object);
        run(command);
        objects.push(object);
    }
    let mut archive = Command::new(archiver);
    archive.args(
        env::var("NEKO_AR_ARGS")
            .unwrap_or_default()
            .split_whitespace(),
    );
    archive
        .arg("crs")
        .arg(out.join("libneko_squirrel.a"))
        .args(objects);
    run(archive);
    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-lib=static=neko_squirrel");
    println!("cargo:rerun-if-changed={}", root.display());
    println!("cargo:rerun-if-changed=src/squirrel_bridge.cpp");
    for key in ["CXX", "AR", "NEKO_CXX_ARGS", "NEKO_AR_ARGS"] {
        println!("cargo:rerun-if-env-changed={key}");
    }
}
