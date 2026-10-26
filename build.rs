use std::env;
use std::process::Command;

fn detect_sdk_major_version() -> Option<u32> {
    let output = Command::new("xcrun")
        .args(["--sdk", "macosx", "--show-sdk-version"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let version = String::from_utf8_lossy(&output.stdout);
    version.trim().split('.').next()?.parse().ok()
}

fn swift_define_flags(sdk_version: Option<u32>) -> Vec<String> {
    let mut flags = Vec::new();
    if sdk_version.is_some_and(|version| version >= 26) {
        flags.push("-DSPEECH_HAS_MACOS26_SDK".to_string());
    }
    flags
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=swift-bridge");
    println!("cargo:rerun-if-env-changed=DOCS_RS");
    println!("cargo:rerun-if-env-changed=DEVELOPER_DIR");
    println!("cargo:rerun-if-env-changed=SDKROOT");

    if env::var("DOCS_RS").is_ok() {
        return;
    }

    println!("cargo:rustc-link-lib=framework=Speech");
    println!("cargo:rustc-link-lib=framework=AVFoundation");
    println!("cargo:rustc-link-lib=framework=CoreMedia");
    println!("cargo:rustc-link-lib=framework=Foundation");

    let swift_dir = "swift-bridge";
    let out_dir = env::var("OUT_DIR").unwrap();
    let swift_build_dir = format!("{out_dir}/swift-build");

    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    let swift_triple = match target_arch.as_str() {
        "x86_64" => "x86_64-apple-macosx",
        "aarch64" => "arm64-apple-macosx",
        other => panic!("speech: unsupported target arch '{other}'"),
    };

    let sdk_version = detect_sdk_major_version();
    let define_flags = swift_define_flags(sdk_version);

    let mut swift_args = vec![
        "build",
        "-c",
        "release",
        "--triple",
        swift_triple,
        "--package-path",
        swift_dir,
        "--scratch-path",
        &swift_build_dir,
    ];
    for flag in &define_flags {
        swift_args.push("-Xswiftc");
        swift_args.push(flag);
    }

    let mut swift_build = Command::new("swift");
    swift_build.args(&swift_args).args(["--jobs", "4"]);
    let output = swift_build.output().expect("Failed to build Swift bridge");

    assert!(
        output.status.success(),
        "Swift build failed ({}):\n{}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let output = swift_build
        .arg("--show-bin-path")
        .output()
        .expect("failed to query the Swift product path");
    assert!(
        output.status.success(),
        "Swift product path query failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let swift_bin_dir = String::from_utf8(output.stdout).expect("Swift product path must be UTF-8");
    let swift_bin_dir = swift_bin_dir.trim();
    assert!(!swift_bin_dir.is_empty(), "Swift product path is empty");

    println!("cargo:rustc-link-search=native={swift_bin_dir}");
    println!("cargo:rustc-link-lib=static=SpeechBridge");
    println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");

    if let Ok(output) = Command::new("xcode-select").arg("-p").output() {
        if output.status.success() {
            let xcode_path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let swift_lib_path =
                format!("{xcode_path}/Toolchains/XcodeDefault.xctoolchain/usr/lib/swift/macosx");
            println!("cargo:rustc-link-arg=-Wl,-rpath,{swift_lib_path}");
        }
    }
}
