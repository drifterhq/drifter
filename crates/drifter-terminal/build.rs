fn main() {
    println!("cargo:rerun-if-env-changed=GHOSTTY_VT_DIR");
    if !cfg!(target_os = "linux") {
        return;
    }
    let dir = match std::env::var("GHOSTTY_VT_DIR") {
        Ok(dir) => dir,
        Err(_) => {
            println!("cargo:warning=GHOSTTY_VT_DIR is not set; set it to a ghostty zig-out folder");
            return;
        }
    };
    println!("cargo:rustc-link-search=native={dir}/lib");
    println!("cargo:rustc-link-lib=static=ghostty-vt");
    println!("cargo:rustc-link-lib=dylib=m");
    println!("cargo:rustc-link-lib=dylib=pthread");
    println!("cargo:rustc-link-lib=dylib=dl");
}
