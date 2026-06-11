//! Generates GIF test fixtures for verifying the lossless GIF path:
//! `cargo run --example gifgen -- <outdir>`
//!
//! - anim.gif: 20 deliberately unoptimized full frames (moving square),
//!   with a comment extension
//! - static.gif: single frame, no animation
//! - trans.gif: pixels go opaque -> transparent (Background disposal);
//!   iopt must refuse to rewrite this one

use gif::{DisposalMethod, Encoder, ExtensionData, Frame, Repeat};
use std::borrow::Cow;
use std::fs::File;

fn palette() -> Vec<u8> {
    let mut pal = Vec::new();
    for i in 0..16u8 {
        pal.extend([i * 16, 255 - i * 16, (i * 37) % 255]);
    }
    pal
}

fn main() {
    let dir = std::env::args().nth(1).expect("usage: gifgen <outdir>");
    std::fs::create_dir_all(&dir).unwrap();

    // anim.gif: static patterned background, 20x20 square moving diagonally,
    // every frame written as a full canvas (the worst case iopt should fix).
    let (w, h) = (200u16, 200u16);
    let mut file = File::create(format!("{dir}/anim.gif")).unwrap();
    let mut enc = Encoder::new(&mut file, w, h, &palette()).unwrap();
    enc.set_repeat(Repeat::Infinite).unwrap();
    enc.write_extension(ExtensionData::new_control_ext(10, DisposalMethod::Keep, false, None))
        .unwrap();
    enc.write_raw_extension(gif::AnyExtension(0xFE), &[b"iopt test comment"])
        .unwrap();
    let background: Vec<u8> = (0..w as usize * h as usize)
        .map(|i| (((i % w as usize) / 25 + (i / w as usize) / 25) % 12) as u8)
        .collect();
    for step in 0..20u16 {
        let mut buf = background.clone();
        let (sx, sy) = (10 + step * 8, 10 + step * 8);
        for y in sy..sy + 20 {
            for x in sx..sx + 20 {
                buf[y as usize * w as usize + x as usize] = 14;
            }
        }
        let frame = Frame {
            width: w,
            height: h,
            delay: 10,
            buffer: Cow::Owned(buf),
            ..Frame::default()
        };
        enc.write_frame(&frame).unwrap();
    }
    drop(enc);

    // static.gif: one 100x100 frame using many palette entries.
    let mut pal = Vec::new();
    for i in 0..128u8 {
        pal.extend([i * 2, i, 255 - i]);
    }
    let mut file = File::create(format!("{dir}/static.gif")).unwrap();
    let mut enc = Encoder::new(&mut file, 100, 100, &pal).unwrap();
    let buf: Vec<u8> = (0..100usize * 100).map(|i| (i % 128) as u8).collect();
    let frame = Frame {
        width: 100,
        height: 100,
        buffer: Cow::Owned(buf),
        ..Frame::default()
    };
    enc.write_frame(&frame).unwrap();
    drop(enc);

    // trans.gif: frame 0 paints an opaque block, then Background disposal
    // clears it, so its pixels render opaque -> transparent over time.
    let mut file = File::create(format!("{dir}/trans.gif")).unwrap();
    let mut enc = Encoder::new(&mut file, 50, 50, &[255, 0, 0, 0, 255, 0]).unwrap();
    enc.set_repeat(Repeat::Infinite).unwrap();
    let f0 = Frame {
        width: 50,
        height: 50,
        delay: 50,
        dispose: DisposalMethod::Background,
        buffer: Cow::Owned(vec![0u8; 2500]),
        ..Frame::default()
    };
    enc.write_frame(&f0).unwrap();
    let f1 = Frame {
        left: 10,
        top: 10,
        width: 5,
        height: 5,
        delay: 50,
        dispose: DisposalMethod::Keep,
        buffer: Cow::Owned(vec![1u8; 25]),
        ..Frame::default()
    };
    enc.write_frame(&f1).unwrap();
    drop(enc);

    println!("fixtures written to {dir}");
}
