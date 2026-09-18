fn main() {
    println!("cargo:rerun-if-changed=src/audio-macos.swift");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        let output =
            std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("remvora-audio");
        let architecture = if std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("aarch64") {
            "arm64"
        } else {
            "x86_64"
        };
        let target = format!("{architecture}-apple-macosx13.0");
        let status = std::process::Command::new("/usr/bin/swiftc")
            .args([
                "-target",
                &target,
                "-parse-as-library",
                "-O",
                "src/audio-macos.swift",
                "-o",
            ])
            .arg(output)
            .status()
            .expect("macOS builds need Xcode command line tools");
        assert!(
            status.success(),
            "ScreenCaptureKit audio helper compilation failed"
        );
    }
}
