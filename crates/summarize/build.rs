//! Compiles `swift/Summarize.swift` into a static library with the Xcode toolchain and links
//! it with the Swift runtime and the frameworks it uses (ADR 0007). Other platforms build none.

use std::env;
use std::error::Error;
use std::path::PathBuf;
use std::process::Command;

const SOURCE: &str = "swift/Summarize.swift";

fn main() -> Result<(), Box<dyn Error>> {
    println!("cargo:rerun-if-changed={SOURCE}");
    println!("cargo:rerun-if-env-changed=MACOSX_DEPLOYMENT_TARGET");
    if env::var("CARGO_CFG_TARGET_OS")? != "macos" {
        return Ok(());
    }
    let out = PathBuf::from(env::var("OUT_DIR")?);
    let arch = match env::var("CARGO_CFG_TARGET_ARCH")?.as_str() {
        "aarch64" => "arm64",
        "x86_64" => "x86_64",
        other => return Err(format!("no Swift target for the architecture {other}").into()),
    };
    // Foundation Models needs macOS 26, as does the app.
    let macos = env::var("MACOSX_DEPLOYMENT_TARGET").unwrap_or_else(|_| "26.0".to_owned());
    // Swift 5 mode, as in `crumble-transcribe`: the bridge hands results between threads
    // through a box Swift 6 would reject (ADR 0006).
    let status = Command::new("xcrun")
        .args(["swiftc", "-parse-as-library", "-O", "-swift-version", "5"])
        .args(["-target", &format!("{arch}-apple-macosx{macos}")])
        .args([
            "-emit-library",
            "-static",
            "-module-name",
            "CrumbleSummarize",
        ])
        .arg("-o")
        .arg(out.join("libcrumble_summarize_swift.a"))
        .arg(SOURCE)
        .status()
        .map_err(|error| format!("could not run xcrun, which comes with Xcode: {error}"))?;
    if !status.success() {
        return Err(format!("swiftc could not compile {SOURCE}").into());
    }
    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-lib=static=crumble_summarize_swift");
    // The Swift runtime ships with macOS; the SDK holds the stubs to link against.
    let sdk = Command::new("xcrun")
        .args(["--sdk", "macosx", "--show-sdk-path"])
        .output()?;
    let sdk = String::from_utf8(sdk.stdout)?;
    println!(
        "cargo:rustc-link-search=native={}/usr/lib/swift",
        sdk.trim()
    );
    for framework in ["Foundation", "FoundationModels"] {
        println!("cargo:rustc-link-lib=framework={framework}");
    }
    Ok(())
}
