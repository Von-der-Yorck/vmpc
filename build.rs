//! Embeds the VMPC icon and version info into the Windows executable.
//! The version comes from Cargo.toml, so the .exe properties always match the app.

fn main() {
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rerun-if-changed=assets/vmpc.rc.in");
        println!("cargo:rerun-if-changed=assets/vmpc.ico");
        let manifest = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
        let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("vmpc.rc");
        let version = std::env::var("CARGO_PKG_VERSION").unwrap();
        let commas = ["CARGO_PKG_VERSION_MAJOR", "CARGO_PKG_VERSION_MINOR", "CARGO_PKG_VERSION_PATCH"]
            .map(|k| std::env::var(k).unwrap())
            .join(",")
            + ",0";
        let icon = manifest.join("assets").join("vmpc.ico").to_string_lossy().replace('\\', "\\\\");
        let rc = std::fs::read_to_string(manifest.join("assets").join("vmpc.rc.in"))
            .unwrap()
            .replace("@ICON@", &icon)
            .replace("@VERSION_COMMAS@", &commas)
            .replace("@VERSION@", &version);
        std::fs::write(&out, rc).unwrap();
        // Optional: without a resource compiler the app still builds, just without an .exe icon.
        if let Err(e) = embed_resource::compile(&out, embed_resource::NONE).manifest_optional() {
            println!("cargo:warning=app icon not embedded: {e:?}");
        }
    }
}
