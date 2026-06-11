//! Lossless PNG optimization via oxipng.

use anyhow::Result;
use oxipng::{Deflater, Options, StripChunks, ZopfliOptions};

pub fn optimize(data: &[u8], level: u8, zopfli: bool, strip: bool) -> Result<Vec<u8>> {
    let mut opts = Options::from_preset(level);
    if zopfli {
        opts.deflater = Deflater::Zopfli(ZopfliOptions::default());
    }
    opts.strip = if strip {
        StripChunks::Safe
    } else {
        StripChunks::None
    };
    Ok(oxipng::optimize_from_memory(data, &opts)?)
}
