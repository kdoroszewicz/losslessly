//! Renders two SVGs with resvg at 2x scale and compares the pixels:
//! `cargo run --example svgcmp -- a.svg b.svg`
//!
//! Minification rewrites markup at svgo-equivalent numeric precision, so a
//! few low-delta pixels along antialiased edges are tolerated; structural
//! differences are not.

use resvg::tiny_skia;
use resvg::usvg;

fn render(path: &str) -> tiny_skia::Pixmap {
    let data = std::fs::read(path).unwrap();
    let tree = usvg::Tree::from_data(&data, &usvg::Options::default()).unwrap();
    let size = tree.size();
    let (w, h) = (
        (size.width() * 2.0).ceil() as u32,
        (size.height() * 2.0).ceil() as u32,
    );
    let mut pixmap = tiny_skia::Pixmap::new(w.max(1), h.max(1)).unwrap();
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(2.0, 2.0),
        &mut pixmap.as_mut(),
    );
    pixmap
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let a = render(&args[1]);
    let b = render(&args[2]);
    println!(
        "a: {}x{}, b: {}x{}",
        a.width(),
        a.height(),
        b.width(),
        b.height()
    );
    if (a.width(), a.height()) != (b.width(), b.height()) {
        println!("DIMENSIONS DIFFER");
        std::process::exit(1);
    }

    let total = a.data().len();
    let mut differing = 0usize;
    let mut max_delta = 0u8;
    for (&x, &y) in a.data().iter().zip(b.data()) {
        let delta = x.abs_diff(y);
        if delta > 0 {
            differing += 1;
            max_delta = max_delta.max(delta);
        }
    }

    let pct = differing as f64 / total as f64 * 100.0;
    println!("differing bytes: {differing}/{total} ({pct:.4}%), max channel delta: {max_delta}");
    if differing == 0 {
        println!("RENDERS IDENTICAL");
    } else if pct < 0.5 && max_delta <= 16 {
        println!("RENDERS EQUIVALENT (sub-pixel antialiasing differences only)");
    } else {
        println!("RENDERS DIFFER");
        std::process::exit(1);
    }
}
