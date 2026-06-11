//! Lossless GIF optimization.
//!
//! Re-encodes the animation as interframe deltas (what `gifsicle -O2` does):
//! one global palette built from the exact composited colors, a full first
//! frame, then per-frame bounding boxes where unchanged pixels are
//! transparent. The *rendered* frames, delays and loop count are preserved
//! exactly; whenever that can't be guaranteed (more than 256 distinct colors
//! on screen, or pixels that turn from opaque back to transparent), the file
//! is left untouched instead.

use anyhow::{bail, Context, Result};
use rgb::{RGB8, RGBA8};
use std::collections::HashMap;
use std::io::Cursor;

// Leading `::` disambiguates the extern crate from this module.
use ::gif::{AnyExtension, DecodeOptions, DisposalMethod, Encoder, Frame};

/// Returns `None` when the file can't be losslessly re-encoded — the caller
/// treats that as "already optimal".
pub fn optimize(data: &[u8], strip: bool) -> Result<Option<Vec<u8>>> {
    let Some(plan) = analyze(data)? else {
        return Ok(None);
    };
    encode(data, &plan, strip).map(Some)
}

struct Plan {
    width: u16,
    height: u16,
    /// Exact RGB colors appearing on the composited canvas, in first-seen order.
    palette: Vec<RGB8>,
    color_index: HashMap<RGB8, u8>,
    /// Index used for "unchanged/transparent" pixels, if one is needed.
    transparent: Option<u8>,
}

fn decoder(data: &[u8]) -> Result<::gif::Decoder<Cursor<&[u8]>>> {
    let mut opts = DecodeOptions::new();
    opts.set_color_output(::gif::ColorOutput::Indexed);
    Ok(opts.read_info(Cursor::new(data))?)
}

/// First pass: composite every frame and collect the exact set of on-screen
/// colors, bailing out (`None`) on anything a delta re-encode can't express.
fn analyze(data: &[u8]) -> Result<Option<Plan>> {
    let mut decoder = decoder(data)?;
    let mut screen = gif_dispose::Screen::new_decoder(&decoder);

    let mut palette: Vec<RGB8> = Vec::new();
    let mut color_index: HashMap<RGB8, u8> = HashMap::new();
    let mut has_transparency = false;
    let mut prev: Option<Vec<RGBA8>> = None;
    let mut frames = 0usize;

    while let Some(frame) = decoder.read_next_frame().context("invalid GIF data")? {
        screen.blit_frame(frame).context("invalid GIF frame")?;
        let canvas: Vec<RGBA8> = screen.pixels_rgba().pixels().collect();
        for (i, px) in canvas.iter().enumerate() {
            if px.a == 0 {
                has_transparency = true;
                // A pixel that goes opaque -> transparent can't be expressed
                // with "keep" disposal deltas; don't risk it.
                if prev.as_ref().is_some_and(|p| p[i].a != 0) {
                    return Ok(None);
                }
            } else {
                let rgb = px.rgb();
                if let std::collections::hash_map::Entry::Vacant(entry) = color_index.entry(rgb) {
                    if palette.len() == 256 {
                        return Ok(None);
                    }
                    entry.insert(palette.len() as u8);
                    palette.push(rgb);
                }
            }
        }
        prev = Some(canvas);
        frames += 1;
    }
    if frames == 0 {
        bail!("GIF contains no frames");
    }

    // Delta frames mark unchanged pixels as transparent, so any animation
    // needs a spare palette slot; so does real transparency.
    let transparent = if has_transparency || frames > 1 {
        if palette.len() == 256 {
            return Ok(None);
        }
        let idx = palette.len() as u8;
        palette.push(RGB8::new(0, 0, 0));
        Some(idx)
    } else {
        None
    };

    Ok(Some(Plan {
        width: decoder.width(),
        height: decoder.height(),
        palette,
        color_index,
        transparent,
    }))
}

/// Second pass: re-encode as a full first frame plus bounding-box deltas.
fn encode(data: &[u8], plan: &Plan, strip: bool) -> Result<Vec<u8>> {
    let mut decoder = decoder(data)?;
    let mut screen = gif_dispose::Screen::new_decoder(&decoder);
    let repeat = decoder.repeat();

    let palette_bytes: Vec<u8> = plan
        .palette
        .iter()
        .flat_map(|c| [c.r, c.g, c.b])
        .collect();
    let mut out = Vec::with_capacity(data.len());
    let mut encoder = Encoder::new(&mut out, plan.width, plan.height, &palette_bytes)?;
    encoder.set_repeat(repeat)?;

    if !strip {
        for (label, blocks) in collect_metadata_extensions(data)? {
            let refs: Vec<&[u8]> = blocks.iter().map(Vec::as_slice).collect();
            encoder.write_raw_extension(AnyExtension(label), &refs)?;
        }
    }

    let width = plan.width as usize;
    let index_of = |px: RGBA8| -> u8 {
        if px.a == 0 {
            plan.transparent.expect("transparency implies reserved slot")
        } else {
            plan.color_index[&px.rgb()]
        }
    };

    let mut prev: Option<Vec<RGBA8>> = None;
    while let Some(src) = decoder.read_next_frame()? {
        screen.blit_frame(src)?;
        let canvas: Vec<RGBA8> = screen.pixels_rgba().pixels().collect();

        let mut frame = Frame {
            delay: src.delay,
            dispose: DisposalMethod::Keep,
            transparent: plan.transparent,
            needs_user_input: src.needs_user_input,
            ..Frame::default()
        };

        match &prev {
            None => {
                frame.width = plan.width;
                frame.height = plan.height;
                frame.buffer = canvas.iter().map(|&px| index_of(px)).collect();
            }
            Some(prev) => match diff_bbox(prev, &canvas, width) {
                None => {
                    // Nothing changed; emit a 1x1 transparent frame to keep
                    // the frame count and timing intact.
                    frame.width = 1;
                    frame.height = 1;
                    frame.buffer = vec![plan.transparent.unwrap()].into();
                }
                Some((x0, y0, x1, y1)) => {
                    frame.left = x0 as u16;
                    frame.top = y0 as u16;
                    frame.width = (x1 - x0 + 1) as u16;
                    frame.height = (y1 - y0 + 1) as u16;
                    let mut buffer = Vec::with_capacity(
                        frame.width as usize * frame.height as usize,
                    );
                    for y in y0..=y1 {
                        for x in x0..=x1 {
                            let i = y * width + x;
                            buffer.push(if canvas[i] == prev[i] {
                                plan.transparent.unwrap()
                            } else {
                                index_of(canvas[i])
                            });
                        }
                    }
                    frame.buffer = buffer.into();
                }
            },
        }

        encoder.write_frame(&frame)?;
        prev = Some(canvas);
    }

    drop(encoder);
    Ok(out)
}

/// Bounding box of pixels that differ between two canvases, or `None` if
/// they are identical.
fn diff_bbox(
    a: &[RGBA8],
    b: &[RGBA8],
    width: usize,
) -> Option<(usize, usize, usize, usize)> {
    let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0, 0);
    for (i, (pa, pb)) in a.iter().zip(b).enumerate() {
        if pa != pb {
            let (x, y) = (i % width, i / width);
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
    }
    (x0 != usize::MAX).then_some((x0, y0, x1, y1))
}

/// Block-level scan for metadata extensions worth preserving: comments and
/// application extensions (XMP, ICC, ...). The graphic control and NETSCAPE
/// loop extensions are excluded — the encoder re-emits those itself.
/// Each extension is returned as (label, sub-blocks).
fn collect_metadata_extensions(data: &[u8]) -> Result<Vec<(u8, Vec<Vec<u8>>)>> {
    let err = || anyhow::anyhow!("malformed GIF block structure");
    let mut found = Vec::new();
    let mut pos = 6; // header

    // Logical screen descriptor + optional global color table.
    let flags = *data.get(pos + 4).ok_or_else(err)?;
    pos += 7;
    if flags & 0x80 != 0 {
        pos += 3 * (1 << ((flags & 0x07) + 1));
    }

    loop {
        match *data.get(pos).ok_or_else(err)? {
            0x3B => break, // trailer
            0x2C => {
                // Image descriptor (+ optional local color table) + LZW data.
                let flags = *data.get(pos + 9).ok_or_else(err)?;
                pos += 10;
                if flags & 0x80 != 0 {
                    pos += 3 * (1 << ((flags & 0x07) + 1));
                }
                pos += 1; // LZW minimum code size
                pos = skip_sub_blocks(data, pos).ok_or_else(err)?;
            }
            0x21 => {
                let label = *data.get(pos + 1).ok_or_else(err)?;
                pos += 2;
                let mut blocks = Vec::new();
                while let Some(&len) = data.get(pos) {
                    pos += 1;
                    if len == 0 {
                        break;
                    }
                    let block = data.get(pos..pos + len as usize).ok_or_else(err)?;
                    blocks.push(block.to_vec());
                    pos += len as usize;
                }
                let is_loop_ext = label == 0xFF
                    && blocks
                        .first()
                        .is_some_and(|b| b.starts_with(b"NETSCAPE2.0") || b.starts_with(b"ANIMEXTS1.0"));
                if label != 0xF9 && !is_loop_ext {
                    found.push((label, blocks));
                }
            }
            _ => return Err(err()),
        }
    }
    Ok(found)
}

fn skip_sub_blocks(data: &[u8], mut pos: usize) -> Option<usize> {
    loop {
        let len = *data.get(pos)? as usize;
        pos += 1;
        if len == 0 {
            return Some(pos);
        }
        pos += len;
    }
}
