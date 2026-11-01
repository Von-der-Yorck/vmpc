//! VMPC — Video Media Player Converter.
//!
//! A lightweight, neumorphic media player and converter written in Rust.
//! Usage: `vmpc [FILES or URLs...]`, or `vmpc --export-icon <dir>` to write the
//! app icon as PNG/ICO (used by the packaging scripts).

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod converter;
mod ffmpeg;
mod formats;
mod frames;
mod icons;
mod jobs;
mod player;
mod settings;
mod theme;
mod util;

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--export-icon") {
        let dir = std::path::PathBuf::from(args.get(1).map(String::as_str).unwrap_or("."));
        let _ = std::fs::create_dir_all(&dir);
        for size in [256u32, 512] {
            let png = icons::encode_png(size, size, &icons::logo_rgba(size));
            std::fs::write(dir.join(format!("vmpc-{size}.png")), png).expect("write png");
        }
        std::fs::write(dir.join("vmpc.ico"), icons::encode_ico(&[16, 24, 32, 48, 64, 128, 256])).expect("write ico");
        for size in [55u32, 110] {
            std::fs::write(dir.join(format!("wizard-{size}.bmp")), icons::encode_bmp(size)).expect("write bmp");
        }
        println!("Icons written to {}", dir.display());
        return Ok(());
    }

    let icon = egui::IconData { rgba: icons::logo_rgba(256), width: 256, height: 256 };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("VMPC — Video Media Player Converter")
            .with_app_id("vmpc")
            .with_inner_size([1200.0, 760.0])
            .with_min_inner_size([820.0, 560.0])
            .with_icon(icon)
            .with_drag_and_drop(true),
        vsync: true,
        ..Default::default()
    };
    eframe::run_native("VMPC", options, Box::new(move |cc| Ok(Box::new(app::App::new(cc, args)))))
}
