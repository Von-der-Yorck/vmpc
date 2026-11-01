//! Embeds the VMPC icon and version info into the Windows executable.

fn main() {
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rerun-if-changed=assets/vmpc.rc");
        println!("cargo:rerun-if-changed=assets/vmpc.ico");
        // Optional: without a resource compiler the app still builds, just without an .exe icon.
        if let Err(e) = embed_resource::compile("assets/vmpc.rc", embed_resource::NONE).manifest_optional() {
            println!("cargo:warning=app icon not embedded: {e:?}");
        }
    }
}
