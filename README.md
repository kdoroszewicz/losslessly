# losslessly

Fast, lossless image optimizer for the command line.

`losslessly` recompresses PNG, JPEG, GIF, WebP and SVG files in place without touching a single pixel — like [ImageOptim](https://imageoptim.com), but as a small, dependency-free CLI that works anywhere: a folder of photos, a website's asset directory, a script, a build pipeline. Files are only rewritten when the result is smaller, so running it twice is a no-op.

```console
$ losslessly assets/
assets/icons/icon.png  27.3 KB → 23.4 KB  (-14.3%)
assets/photo.jpg  149.5 KB → 135.4 KB  (-9.5%)
assets/photo.png  653.1 KB → 472.0 KB  (-27.7%)
assets/demo.gif  47.0 KB → 4.6 KB  (-90.1%)
assets/texture.webp  1.9 KB → 760 B  (-60.7%)
assets/icon.svg  1.5 KB → 767 B  (-50.1%)
6 optimized, 0 already optimal, 244.1 KB saved
```

## Why

Most images ship with 10–30% of pure waste: unoptimized deflate streams, non-optimized Huffman tables, baseline instead of progressive encoding. Export tools don't care; your disk space and bandwidth do.

`losslessly` removes that waste **losslessly** — the decoded pixels are bit-for-bit identical before and after. There is no quality slider because nothing is ever degraded.

- **PNG** — recompressed with [oxipng](https://github.com/oxipng/oxipng) (bit-depth/color-type reductions, filter trials, libdeflate or Zopfli).
- **JPEG** — transcoded with [mozjpeg](https://github.com/mozilla/mozjpeg) the same way `jpegtran -optimize` works: DCT coefficients are copied verbatim and only the entropy coding is rebuilt. Both optimized-baseline and progressive variants are tried; the smaller one wins.
- **GIF** — re-encoded as interframe deltas (the same idea as `gifsicle -O2`): a single global palette built from the exact on-screen colors, a full first frame, then per-frame bounding boxes where unchanged pixels are transparent. Rendered frames, timing and loop count are preserved exactly; animations exported as stacks of full frames routinely shrink by 80–90%. If a GIF can't be re-encoded with that guarantee (over 256 distinct on-screen colors, or frames that erase pixels back to transparent), it is left untouched.
- **WebP** — lossless (VP8L) stills are re-encoded at libwebp's maximum effort in `exact` mode, so even the RGB values of fully transparent pixels survive bit-for-bit. Lossy and animated WebP files are left untouched — re-encoding those without quality loss isn't possible.
- **SVG** — markup minification via [oxvg](https://github.com/noahbald/oxvg) (a Rust port of svgo) using its correctness-focused preset: doctype/whitespace removal, path and transform compaction, style minification. This is the one format without a bit-level guarantee — output is rendering-equivalent at svgo's default numeric precision rather than byte-identical markup.
- **Metadata is preserved by default** (EXIF, ICC profiles, XMP, comments survive byte-for-byte). Pass `--strip` if you want it gone.
- Parallel across files, atomic writes (temp file + rename — a crash can never leave a truncated image), and corrupt files are refused rather than silently "fixed".

## Install

```sh
cargo install --path .
```

Requires a Rust toolchain. The mozjpeg and libwebp C libraries are built and statically linked automatically; no system dependencies are needed.

## Usage

```sh
losslessly photos/ logo.png            # optimize files and directories in place
losslessly --check assets/             # write nothing, exit 1 if anything is optimizable
losslessly --strip photos/             # also remove EXIF/ICC/comments
losslessly --zopfli --level 6 assets/  # squeeze PNGs as hard as possible (slow)
```

| Option | Description |
| --- | --- |
| `--check` | Dry run: report potential savings without writing, exit `1` if any file could be smaller. |
| `--strip` | Strip metadata. JPEG: EXIF, ICC, comments. PNG: non-essential chunks. GIF: comments. WebP: ICC, EXIF, XMP. SVG: comments, `<metadata>`, editor attributes. |
| `--level <0-6>` | PNG effort preset (default `2`; `6` is slowest/smallest). |
| `--zopfli` | Use Zopfli for PNG deflate. Much slower, usually a bit smaller. |
| `-j, --threads <N>` | Limit parallelism (default: all logical CPUs). |
| `-q, --quiet` | Only print the summary and errors. |

Exit codes: `0` success · `1` `--check` found optimizable files · `2` one or more files failed.

## Guarantees

- **Pixels are never modified.** PNGs and lossless WebPs are recompressed losslessly; JPEGs never go through a decode–encode cycle — the frequency-domain coefficients are copied untouched; GIFs are restructured only in ways proven to render identically (and left alone otherwise). The sole exception is SVG, where the guarantee is rendering-equivalence rather than bit-identity (see Formats).
- **Files only shrink.** If recompression doesn't help, the file is left exactly as it was.
- **Writes are atomic.** Output goes to a temp file in the same directory and is renamed over the original, preserving permissions.
- **Corrupt input is rejected.** libjpeg normally pads truncated files with gray blocks and carries on; `losslessly` treats decoder warnings as errors and refuses to rewrite such files (exit `2`).
- **Mislabeled files are skipped.** Content is sniffed by magic bytes, so a PNG named `.jpg` is reported instead of fed to the wrong codec.

## Formats

| Format | Status | How |
| --- | --- | --- |
| PNG / APNG | ✅ Supported | oxipng recompression (reductions, filter trials, libdeflate/Zopfli) |
| JPEG | ✅ Supported | mozjpeg entropy-coding transcode (`jpegtran -optimize` equivalent) |
| GIF | ✅ Supported | interframe delta re-encoding with exact-color global palette |
| WebP | ✅ Supported | lossless (VP8L) re-encode at maximum effort in exact mode; lossy/animated untouched |
| SVG | ✅ Supported | oxvg markup minification (svgo port) — rendering-equivalent, not byte-identical |
| AVIF / JPEG XL | ❌ Not planned | their encoders are huge native dependencies, and files produced by modern encoders rarely shrink under lossless re-encode — poor trade-off for a tool meant to stay lean |

## Automation

Because `losslessly` is idempotent, only ever shrinks files, and reports through exit codes, it is safe to run unattended. A few recipes:

**Fail a CI build when someone commits an unoptimized image:**

```yaml
- name: Check images are optimized
  run: losslessly --check assets/
```

**Optimize staged images on commit** with [lefthook](https://github.com/evilmartians/lefthook) (files are re-staged automatically):

```yaml
# lefthook.yml
pre-commit:
  commands:
    losslessly:
      glob: "*.{png,apng,jpg,jpeg,gif,webp,svg}"
      run: losslessly {staged_files}
      stage_fixed: true
```

Or as a plain git hook in `.git/hooks/pre-commit`:

```sh
#!/bin/sh
git diff --cached --name-only --diff-filter=ACM | grep -iE '\.(a?png|jpe?g|gif|webp|svg)$' \
  | xargs -r losslessly && git update-index --again
```

## Non-goals

Lossy compression (quality reduction, resizing, chroma subsampling) is out of scope by design — `losslessly` is meant to be safe to run unattended. Converting between formats (e.g. PNG → WebP) is also out of scope: `losslessly` makes the files you have smaller, it doesn't change what they are.

## Development

```sh
cargo build --release
cargo clippy --all-targets
```

Verification helpers exist for convincing yourself (or reviewers) that the optimizations really are lossless. `examples/jpegcmp.rs` decodes two JPEGs through libjpeg with no color management and verifies the pixel data is identical; `examples/webpcmp.rs` does the same through libwebp; `examples/gifcmp.rs` compares GIFs at the rendering level — composited frames, delays and loop count; `examples/svgcmp.rs` rasterizes SVGs with resvg at 2x and compares the pixels. `examples/gifgen.rs` and `examples/webpgen.rs` generate test fixtures, including pathological ones:

```console
$ cargo run --release --example jpegcmp -- original.jpg optimized.jpg
a: 1200x1200, b: 1200x1200
PIXELS IDENTICAL

$ cargo run --release --example gifcmp -- original.gif optimized.gif
20 frames, 200x200, repeat Infinite: RENDERS IDENTICAL
```

## License

MIT
