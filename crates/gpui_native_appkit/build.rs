use std::{env, path::PathBuf, process::Command};

fn main() {
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os != "macos" {
        return;
    }

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let swift_source = manifest_dir.join("swift/NativeTitlebarTabs.swift");
    println!("cargo:rerun-if-changed={}", swift_source.display());

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let library_path = out_dir.join("libgpui_native_appkit_swift.a");

    let status = Command::new("swiftc")
        .arg("-swift-version")
        .arg("5")
        .arg("-parse-as-library")
        .arg("-emit-library")
        .arg("-static")
        .arg("-module-name")
        .arg("GPUIAppKitBridge")
        .arg("-o")
        .arg(&library_path)
        .arg(&swift_source)
        .status()
        .expect("failed to run swiftc for gpui-native-appkit");

    if !status.success() {
        panic!("swiftc failed while building gpui-native-appkit Swift bridge");
    }

    println!("cargo:rustc-link-search=native={}", out_dir.display());
    println!("cargo:rustc-link-lib=static=gpui_native_appkit_swift");
    println!("cargo:rustc-link-lib=framework=AppKit");
    println!("cargo:rustc-link-lib=framework=Foundation");
}
