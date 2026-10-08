fn main() {
    // ggml's Metal code checks the macOS version with @available, which calls
    // __isPlatformVersionAtLeast from clang's runtime library. rustc doesn't link that
    // library, so when building for an older macOS than the SDK (as release builds do),
    // linking fails unless it's added here.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        let out = std::process::Command::new("clang")
            .arg("--print-resource-dir")
            .output()
            .expect("run clang --print-resource-dir");
        let dir = String::from_utf8(out.stdout).expect("clang resource dir");
        println!("cargo:rustc-link-search=native={}/lib/darwin", dir.trim());
        println!("cargo:rustc-link-lib=static=clang_rt.osx");
    }
    tauri_build::build()
}
