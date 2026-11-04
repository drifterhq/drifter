use std::path::{Path, PathBuf};
use std::process::Command;

const ZIG: &str = "0.17.1";

fn main() {
    println!("cargo:rerun-if-changed=../../vendor/ghostty/build.zig");
    println!("cargo:rerun-if-changed=../../vendor/ghostty/build.zig.zon");
    println!("cargo:rerun-if-changed=../../vendor/ghostty/src");
    println!("cargo:rerun-if-changed=../../vendor/ghostty/include");
    println!("cargo:rerun-if-env-changed=PATH");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("linux") {
        return;
    }
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let root = manifest.join("../../vendor/ghostty");
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let arch = match std::env::var("CARGO_CFG_TARGET_ARCH").unwrap().as_str() {
        "x86_64" => "x86_64",
        "aarch64" => "aarch64",
        other => panic!("drifter-terminal does not build for {other}"),
    };
    let zig = find_zig().unwrap_or_else(|| {
        panic!("Zig {ZIG} is missing. Run scripts/zig.sh once, then build again.")
    });
    let prefix = out.join("ghostty");
    let status = Command::new(&zig)
        .current_dir(&root)
        .arg("build")
        .arg("-Demit-lib-vt")
        .arg("-Doptimize=ReleaseFast")
        .arg(format!("-Dtarget={arch}-linux-gnu"))
        .arg("-Dcpu=baseline")
        .arg("--prefix")
        .arg(&prefix)
        .arg("--cache-dir")
        .arg(out.join("zig-cache"))
        .arg("--global-cache-dir")
        .arg(out.join("zig-global"))
        .status()
        .unwrap_or_else(|error| panic!("could not run {}: {error}", zig.display()));
    if !status.success() {
        panic!("zig build of libghostty-vt failed");
    }
    println!("cargo:rustc-link-search=native={}", prefix.join("lib").display());
    println!("cargo:rustc-link-lib=static=ghostty-vt");
    println!("cargo:rustc-link-lib=dylib=m");
    println!("cargo:rustc-link-lib=dylib=pthread");
    println!("cargo:rustc-link-lib=dylib=dl");
}

fn find_zig() -> Option<PathBuf> {
    let on_path = PathBuf::from("zig");
    if version_of(&on_path).as_deref() == Some(ZIG) {
        return Some(on_path);
    }
    let data = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| Path::new(&home).join(".local/share")))?;
    let own = data.join(format!("drifter/zig-{ZIG}/zig"));
    (version_of(&own).as_deref() == Some(ZIG)).then_some(own)
}

fn version_of(zig: &Path) -> Option<String> {
    let output = Command::new(zig).arg("version").output().ok()?;
    output.status.success().then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}
