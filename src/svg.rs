//! SVG minification via oxvg (a Rust port of svgo).
//!
//! Unlike the raster formats there is no bit-level pixel guarantee here:
//! minification rewrites markup (paths, transforms, styles) in ways that are
//! rendering-equivalent at svgo's default numeric precision. The plugin set
//! is oxvg's correctness-focused `safe` preset; documents that fail its
//! prechecks (scripts, unusual structure) are left untouched by oxvg itself.

use anyhow::{Result, anyhow};
use oxvg_ast::{
    parse::roxmltree::{ParsingOptions, parse_with_options},
    serialize::Node as _,
    visitor::Info,
};
use oxvg_optimiser::Jobs;

pub fn optimize(data: &[u8], strip: bool) -> Result<Vec<u8>> {
    let input = std::str::from_utf8(data).map_err(|_| anyhow!("SVG is not valid UTF-8"))?;

    // Editor exports (Inkscape, Illustrator) routinely carry a DOCTYPE;
    // roxmltree refuses DTDs unless asked (it still caps entity expansion).
    let options = ParsingOptions {
        allow_dtd: true,
        ..ParsingOptions::default()
    };
    let output = parse_with_options(input, options, |dom, allocator| {
        let jobs = preset(strip);
        jobs.run(dom, &Info::new(allocator))
            .map_err(|e| anyhow!("SVG optimization failed: {e}"))?;
        dom.serialize()
            .map_err(|e| anyhow!("SVG serialization failed: {e}"))
    })
    .map_err(|e| anyhow!("SVG parsing failed: {e}"))??;

    Ok(output.into_bytes())
}

/// oxvg's `safe` preset, minus the metadata-removing plugins unless the user
/// asked for `--strip`: comments, `<metadata>`, `<desc>` and editor-namespace
/// data are content, not waste, under losslessly's default contract.
fn preset(strip: bool) -> Jobs {
    let mut jobs = Jobs::safe();
    if !strip {
        jobs.remove_comments = None;
        jobs.remove_metadata = None;
        jobs.remove_desc = None;
        jobs.remove_editors_n_s_data = None;
    }
    jobs
}
