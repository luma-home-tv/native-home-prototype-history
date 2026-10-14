#[path = "../../rust-modules/build_support/host_link.rs"]
mod host_link;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../../rust-modules/build_support/host_link.rs");
    if std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default()=="arm" {
        println!("cargo:rustc-link-lib=luna-service2");
        println!("cargo:rustc-link-lib=glib-2.0");
    }
    compile_events();
    host_link::emit(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."));
}

fn compile_events() {
    use std::{path::PathBuf, process::Command};
    println!("cargo:rerun-if-changed=sdl-events.c");
    if std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default() == "arm" {
        return;
    }
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("sdl-events.o");
    let flags = Command::new("pkg-config")
        .args(["--cflags", "sdl2"])
        .output()
        .expect("SDL pkg-config");
    assert!(flags.status.success(), "SDL include flags unavailable");
    let flags = String::from_utf8(flags.stdout).unwrap();
    let status = Command::new("cc")
        .args([
            "-c",
            "sdl-events.c",
            "-O2",
            "-fPIC",
            "-Wall",
            "-Wextra",
            "-Werror",
        ])
        .args(flags.split_whitespace())
        .arg("-o")
        .arg(&out)
        .status()
        .unwrap();
    assert!(status.success(), "SDL adapter compile");
    println!("cargo:rustc-link-arg={}", out.display());
}
