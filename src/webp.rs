//! Lossless WebP optimization.
//!
//! Only lossless (VP8L) stills are touched: they are decoded and re-encoded
//! at libwebp's maximum effort (method 6, quality 100) in `exact` mode, so
//! the stored RGBA values — including those of fully transparent pixels —
//! survive bit-for-bit. Lossy (VP8) and animated files are left untouched:
//! re-encoding those without quality loss is not possible.

use anyhow::{Result, anyhow, bail};
use webp::{BitstreamFeatures, BitstreamFormat, Decoder, Encoder, WebPConfig};

/// Returns `None` when the file can't be losslessly re-encoded — the caller
/// treats that as "already optimal".
pub fn optimize(data: &[u8], strip: bool) -> Result<Option<Vec<u8>>> {
    let Some(features) = BitstreamFeatures::new(data) else {
        bail!("invalid WebP data");
    };
    if features.has_animation() || !matches!(features.format(), Some(BitstreamFormat::Lossless)) {
        return Ok(None);
    }

    let image = Decoder::new(data)
        .decode()
        .ok_or_else(|| anyhow!("failed to decode WebP"))?;
    let encoder = if features.has_alpha() {
        Encoder::from_rgba(&image, image.width(), image.height())
    } else {
        Encoder::from_rgb(&image, image.width(), image.height())
    };

    let mut config = WebPConfig::new().map_err(|()| anyhow!("failed to initialize WebP config"))?;
    config.lossless = 1;
    config.quality = 100.0;
    config.method = 6;
    config.exact = 1;
    let encoded = encoder
        .encode_advanced(&config)
        .map_err(|e| anyhow!("WebP encoding failed: {e:?}"))?;

    let metadata = if strip {
        Vec::new()
    } else {
        metadata_chunks(data)?
    };
    if metadata.is_empty() {
        Ok(Some(encoded.to_vec()))
    } else {
        Ok(Some(build_extended_container(
            &encoded,
            &metadata,
            features.has_alpha(),
        )?))
    }
}

type Chunk = ([u8; 4], Vec<u8>);

/// Collects the ICCP/EXIF/XMP chunks from a WebP RIFF container.
fn metadata_chunks(data: &[u8]) -> Result<Vec<Chunk>> {
    let err = || anyhow!("malformed WebP container");
    let mut found = Vec::new();
    let mut pos = 12; // RIFF header + "WEBP"
    while pos < data.len() {
        let fourcc: [u8; 4] = data.get(pos..pos + 4).ok_or_else(err)?.try_into()?;
        let size_bytes: [u8; 4] = data.get(pos + 4..pos + 8).ok_or_else(err)?.try_into()?;
        let size = u32::from_le_bytes(size_bytes) as usize;
        let payload = data.get(pos + 8..pos + 8 + size).ok_or_else(err)?;
        if matches!(&fourcc, b"ICCP" | b"EXIF" | b"XMP ") {
            found.push((fourcc, payload.to_vec()));
        }
        pos += 8 + size + (size & 1); // chunks are padded to even sizes
    }
    Ok(found)
}

/// Wraps a freshly encoded simple WebP (single VP8L chunk) in an extended
/// (VP8X) container carrying the given metadata chunks, in the order the
/// spec mandates: VP8X, ICCP, image data, EXIF, XMP.
fn build_extended_container(
    encoded: &[u8],
    metadata: &[Chunk],
    has_alpha: bool,
) -> Result<Vec<u8>> {
    let image_chunk = encoded
        .get(12..)
        .filter(|rest| rest.starts_with(b"VP8L"))
        .ok_or_else(|| anyhow!("unexpected encoder output layout"))?;

    let mut flags = 0u32;
    if has_alpha {
        flags |= 0x10;
    }
    let mut canvas = [0u8; 10];
    for (fourcc, _) in metadata {
        flags |= match fourcc {
            b"ICCP" => 0x20,
            b"EXIF" => 0x08,
            b"XMP " => 0x04,
            _ => 0,
        };
    }
    canvas[..4].copy_from_slice(&flags.to_le_bytes());

    let features =
        BitstreamFeatures::new(encoded).ok_or_else(|| anyhow!("unreadable encoder output"))?;
    let (w, h) = (features.width() - 1, features.height() - 1);
    canvas[4..7].copy_from_slice(&w.to_le_bytes()[..3]);
    canvas[7..10].copy_from_slice(&h.to_le_bytes()[..3]);

    let mut body = Vec::new();
    write_chunk(&mut body, b"VP8X", &canvas);
    for (fourcc, payload) in metadata {
        if fourcc == b"ICCP" {
            write_chunk(&mut body, fourcc, payload);
        }
    }
    body.extend_from_slice(image_chunk);
    if image_chunk.len() % 2 == 1 {
        body.push(0);
    }
    for (fourcc, payload) in metadata {
        if fourcc != b"ICCP" {
            write_chunk(&mut body, fourcc, payload);
        }
    }

    let mut out = Vec::with_capacity(body.len() + 12);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&((body.len() + 4) as u32).to_le_bytes());
    out.extend_from_slice(b"WEBP");
    out.extend_from_slice(&body);
    Ok(out)
}

fn write_chunk(out: &mut Vec<u8>, fourcc: &[u8; 4], payload: &[u8]) {
    out.extend_from_slice(fourcc);
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(payload);
    if payload.len() % 2 == 1 {
        out.push(0);
    }
}
