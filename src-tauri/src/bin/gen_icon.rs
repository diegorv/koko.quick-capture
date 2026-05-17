//! One-shot CLI that rasterises the app icon source PNG at 1024x1024.
//!
//! Run with `cargo run --bin gen_icon`. The output lands at
//! `src-tauri/icons/source-1024.png`; feed it to `pnpm tauri icon
//! src-tauri/icons/source-1024.png` to regenerate every bundle icon
//! (icon.icns, icon.ico, the squircle PNGs, etc.) in a single shot.
//!
//! The icon design mirrors the in-app Dock disc: dark violet gradient
//! squircle (Apple's icon template uses a 184px corner radius on a
//! 824x824 art box inset 100px from each edge of a 1024 canvas) with a
//! white Lucide brain-circuit glyph centered on top. Stroke widths and
//! padding are bumped relative to the inline Lucide source so the
//! glyph still reads at the tiny 16x16 Finder list sizes Tauri's
//! tooling generates downstream.

use std::path::PathBuf;
use std::process::ExitCode;

const ICON_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024">
  <defs>
    <linearGradient id="bg" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0%" stop-color="#4c1d95"/>
      <stop offset="100%" stop-color="#1a0a3a"/>
    </linearGradient>
  </defs>
  <rect x="100" y="100" width="824" height="824" rx="184" ry="184" fill="url(#bg)"/>
  <g transform="translate(232 232) scale(23.333)" fill="none" stroke="white" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
    <path d="M12 5a3 3 0 1 0-5.997.125 4 4 0 0 0-2.526 5.77 4 4 0 0 0 .556 6.588A4 4 0 1 0 12 18Z"/>
    <path d="M9 13a4.5 4.5 0 0 0 3-4"/>
    <path d="M6.003 5.125A3 3 0 0 0 6.401 6.5"/>
    <path d="M3.477 10.896a4 4 0 0 1 .585-.396"/>
    <path d="M6 18a4 4 0 0 1-1.967-.516"/>
    <path d="M12 13h4"/>
    <path d="M12 18h6a2 2 0 0 1 2 2v1"/>
    <path d="M12 8h8"/>
    <path d="M16 8V5a2 2 0 0 1 2-2"/>
    <circle cx="16" cy="13" r="0.5" fill="white"/>
    <circle cx="18" cy="3" r="0.5" fill="white"/>
    <circle cx="20" cy="21" r="0.5" fill="white"/>
    <circle cx="20" cy="8" r="0.5" fill="white"/>
  </g>
</svg>"##;

fn main() -> ExitCode {
    const SIZE: u32 = 1024;

    let opt = usvg::Options::default();
    let tree = match usvg::Tree::from_str(ICON_SVG, &opt) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("gen_icon: failed to parse SVG: {e}");
            return ExitCode::FAILURE;
        }
    };

    let Some(mut pixmap) = tiny_skia::Pixmap::new(SIZE, SIZE) else {
        eprintln!("gen_icon: failed to allocate {SIZE}x{SIZE} pixmap");
        return ExitCode::FAILURE;
    };

    resvg::render(&tree, tiny_skia::Transform::identity(), &mut pixmap.as_mut());

    // Resolve the icons dir relative to the manifest so the binary
    // can be invoked from any working directory.
    let out_dir: PathBuf = [env!("CARGO_MANIFEST_DIR"), "icons"].iter().collect();
    if let Err(e) = std::fs::create_dir_all(&out_dir) {
        eprintln!("gen_icon: failed to create {}: {e}", out_dir.display());
        return ExitCode::FAILURE;
    }

    let out_path = out_dir.join("source-1024.png");
    if let Err(e) = pixmap.save_png(&out_path) {
        eprintln!("gen_icon: failed to write {}: {e}", out_path.display());
        return ExitCode::FAILURE;
    }

    println!("wrote {}", out_path.display());
    println!("next: pnpm tauri icon src-tauri/icons/source-1024.png");
    ExitCode::SUCCESS
}
