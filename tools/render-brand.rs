//! Converts the supplied SVG identity to PNG and a Windows multi-resolution ICO.
//! Uses the same resvg already compiled for GPUI; no downloaded assets or libraries.
use resvg::{tiny_skia, usvg};
use std::{fs, path::Path};

fn render(source: &Path, width: u32, fonts: &usvg::Options<'_>) -> Vec<u8> {
    let tree = usvg::Tree::from_data(&fs::read(source).expect("Read supplied SVG"), fonts)
        .expect("Valid SVG");
    let scale = width as f32 / tree.size().width();
    let height = (tree.size().height() * scale).round() as u32;
    let mut pixels = tiny_skia::Pixmap::new(width, height).expect("Bounded brand image");
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixels.as_mut(),
    );
    pixels.encode_png().expect("PNG")
}
fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    assert_eq!(
        args.len(),
        2,
        "render-brand <source-directory> <output-directory>"
    );
    let source = Path::new(&args[0]);
    let output = Path::new(&args[1]);
    fs::create_dir_all(output).unwrap();
    let mut options = usvg::Options::default();
    options.font_family = "Segoe UI".into();
    options.fontdb_mut().load_system_fonts();
    for (name, width) in [
        ("icon-app", 256),
        ("icon-transparent", 256),
        ("logo-dark", 900),
        ("logo-light", 900),
    ] {
        fs::write(
            output.join(format!("{name}.png")),
            render(&source.join(format!("{name}.svg")), width, &options),
        )
        .unwrap();
    }
    let sizes = [16u32, 20, 24, 32, 40, 48, 64, 128, 256];
    let frames: Vec<_> = sizes
        .iter()
        .map(|&size| render(&source.join("icon-app.svg"), size, &options))
        .collect();
    let mut ico = vec![0, 0, 1, 0, sizes.len() as u8, 0];
    let mut offset = 6 + 16 * sizes.len() as u32;
    for (&size, png) in sizes.iter().zip(&frames) {
        ico.extend([if size == 256 { 0 } else { size as u8 }; 2]);
        ico.extend([0, 0]);
        ico.extend(1u16.to_le_bytes());
        ico.extend(32u16.to_le_bytes());
        ico.extend((png.len() as u32).to_le_bytes());
        ico.extend(offset.to_le_bytes());
        offset += png.len() as u32;
    }
    for png in frames {
        ico.extend(png);
    }
    fs::write(output.join("netlane-diagnostics.ico"), ico).unwrap();
}
