//! Verifies that two GIFs *render* identically: same frame count, same
//! per-frame delays, same loop count, and bit-identical composited RGBA
//! canvases for every frame. Verification helper:
//! `cargo run --example gifcmp -- a.gif b.gif`

use rgb::RGBA8;
use std::fs::File;

struct Rendered {
    repeat: gif::Repeat,
    frames: Vec<(u16, Vec<RGBA8>)>, // (delay, composited canvas)
    width: u16,
    height: u16,
}

fn render(path: &str) -> Rendered {
    let mut opts = gif::DecodeOptions::new();
    opts.set_color_output(gif::ColorOutput::Indexed);
    let mut decoder = opts.read_info(File::open(path).unwrap()).unwrap();
    let mut screen = gif_dispose::Screen::new_decoder(&decoder);
    let (width, height) = (decoder.width(), decoder.height());
    let repeat = decoder.repeat();
    let mut frames = Vec::new();
    while let Some(frame) = decoder.read_next_frame().unwrap() {
        screen.blit_frame(frame).unwrap();
        frames.push((frame.delay, screen.pixels_rgba().pixels().collect()));
    }
    Rendered {
        repeat,
        frames,
        width,
        height,
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let a = render(&args[1]);
    let b = render(&args[2]);

    if (a.width, a.height) != (b.width, b.height) {
        println!("CANVAS SIZE DIFFERS");
        std::process::exit(1);
    }
    if a.repeat != b.repeat {
        println!("LOOP COUNT DIFFERS: {:?} vs {:?}", a.repeat, b.repeat);
        std::process::exit(1);
    }
    if a.frames.len() != b.frames.len() {
        println!(
            "FRAME COUNT DIFFERS: {} vs {}",
            a.frames.len(),
            b.frames.len()
        );
        std::process::exit(1);
    }
    for (i, ((da, pa), (db, pb))) in a.frames.iter().zip(&b.frames).enumerate() {
        if da != db {
            println!("DELAY DIFFERS at frame {i}: {da} vs {db}");
            std::process::exit(1);
        }
        if pa != pb {
            let diff = pa.iter().zip(pb).filter(|(x, y)| x != y).count();
            println!("PIXELS DIFFER at frame {i}: {diff} of {} px", pa.len());
            std::process::exit(1);
        }
    }
    println!(
        "{} frames, {}x{}, repeat {:?}: RENDERS IDENTICAL",
        a.frames.len(),
        a.width,
        a.height,
        a.repeat
    );
}
