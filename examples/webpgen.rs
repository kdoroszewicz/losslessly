//! Generates WebP test fixtures: `cargo run --example webpgen -- <outdir>`
//!
//! - lossless.webp: VP8L encoded at minimum effort (what losslessly should shrink)
//! - meta.webp: the same image in an extended (VP8X) container with EXIF
//!   and XMP chunks that losslessly must carry over
//! - lossy.webp: VP8 encoded; losslessly must leave it untouched

use webp::{Encoder, WebPConfig};

fn pixels(w: usize, h: usize) -> Vec<u8> {
    let mut px = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        for x in 0..w {
            let on_disc = (x * x + y * y) < w * h / 2;
            px.extend([
                if on_disc { 200 } else { (x % 32) as u8 * 8 },
                (y % 64) as u8 * 4,
                if (x / 16 + y / 16) % 2 == 0 { 220 } else { 40 },
                if x < 8 && y < 8 { 0 } else { 255 },
            ]);
        }
    }
    px
}

fn chunk(out: &mut Vec<u8>, fourcc: &[u8; 4], payload: &[u8]) {
    out.extend_from_slice(fourcc);
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(payload);
    if payload.len() % 2 == 1 {
        out.push(0);
    }
}

fn main() {
    let dir = std::env::args().nth(1).expect("usage: webpgen <outdir>");
    std::fs::create_dir_all(&dir).unwrap();
    let (w, h) = (256u32, 256u32);
    let px = pixels(w as usize, h as usize);

    let mut config = WebPConfig::new().unwrap();
    config.lossless = 1;
    config.quality = 0.0; // minimum lossless effort
    config.method = 0;
    config.exact = 1;
    let low_effort = Encoder::from_rgba(&px, w, h)
        .encode_advanced(&config)
        .unwrap();
    std::fs::write(format!("{dir}/lossless.webp"), &*low_effort).unwrap();

    // Extended container: VP8X + image data + EXIF + XMP.
    let image_chunk = &low_effort[12..];
    let mut canvas = [0u8; 10];
    canvas[..4].copy_from_slice(&(0x10u32 | 0x08 | 0x04).to_le_bytes()); // alpha|exif|xmp
    canvas[4..7].copy_from_slice(&(w - 1).to_le_bytes()[..3]);
    canvas[7..10].copy_from_slice(&(h - 1).to_le_bytes()[..3]);
    let mut body = Vec::new();
    chunk(&mut body, b"VP8X", &canvas);
    body.extend_from_slice(image_chunk);
    if image_chunk.len() % 2 == 1 {
        body.push(0);
    }
    chunk(&mut body, b"EXIF", b"losslessly-test-exif-payload");
    chunk(&mut body, b"XMP ", b"<x:xmpmeta>losslessly</x:xmpmeta>");
    let mut meta = Vec::new();
    meta.extend_from_slice(b"RIFF");
    meta.extend_from_slice(&((body.len() + 4) as u32).to_le_bytes());
    meta.extend_from_slice(b"WEBP");
    meta.extend_from_slice(&body);
    std::fs::write(format!("{dir}/meta.webp"), &meta).unwrap();

    let lossy = Encoder::from_rgba(&px, w, h).encode(75.0);
    std::fs::write(format!("{dir}/lossy.webp"), &*lossy).unwrap();

    println!("fixtures written to {dir}");
}
