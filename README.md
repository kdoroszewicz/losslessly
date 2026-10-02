# losslessly

Lossless image optimizer for the command line.

`losslessly` recompresses PNG, JPEG, GIF and WebP files in place, and the new files decode to the same pixels as the old ones. For SVG it minifies the markup, keeping the rendering the same at svgo's default numeric precision.

```console
$ losslessly assets/
assets/demo.gif  47.0 KB → 4.6 KB  (-90.1%)
assets/icon.svg  1.5 KB → 767 B  (-50.1%)
assets/icons/icon.png  27.3 KB → 23.4 KB  (-14.3%)
assets/photo.jpg  149.5 KB → 135.4 KB  (-9.5%)
assets/photo.png  653.1 KB → 472.0 KB  (-27.7%)
assets/texture.webp  1.9 KB → 760 B  (-60.7%)
6 optimized, 0 already optimal, 244.1 KB saved
```

## Install

### Prebuilt binaries

Releases include binaries for macOS and Linux (x86_64, arm64) and Windows (x86_64). Installing them doesn't require Rust.

```sh
# macOS / Linux
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/kdoroszewicz/losslessly/releases/latest/download/losslessly-installer.sh | sh
```

```powershell
# Windows
powershell -ExecutionPolicy Bypass -c "irm https://github.com/kdoroszewicz/losslessly/releases/latest/download/losslessly-installer.ps1 | iex"
```

To install by hand, download an archive from the [releases page](https://github.com/kdoroszewicz/losslessly/releases). With [cargo-binstall](https://github.com/cargo-bins/cargo-binstall), `cargo binstall losslessly` fetches the same binaries.

### From source

Install the latest release from [crates.io](https://crates.io/crates/losslessly):

```sh
cargo install losslessly
```

Or install from a checkout of this repository:

```sh
cargo install --path .
```

You need a Rust toolchain and a C compiler. Cargo builds mozjpeg, libwebp and libdeflate from their bundled sources and links them into the binary. On x86 and x86_64, mozjpeg uses [nasm](https://www.nasm.us) to build its SIMD code; without nasm the build completes with SIMD turned off.

## Usage

```sh
losslessly photos/ logo.png            # optimize files and directories in place
losslessly --check assets/             # write nothing, exit 1 if any file could shrink
losslessly --strip photos/             # also remove metadata
losslessly --zopfli --level 6 assets/  # smallest PNGs, slowest run
```

`losslessly` searches directories and their subdirectories for files ending in `.png`, `.apng`, `.jpg`, `.jpeg`, `.gif`, `.webp` or `.svg` (upper or lower case) and processes them in parallel. Passing a file with any other extension is an error.

| Option | Description |
| --- | --- |
| `--check` | Write nothing. Report the files that could shrink and exit `1` if there are any. |
| `--strip` | Remove metadata (details below). |
| `--level <0-6>` | PNG effort preset: `0` is fastest, `6` is slowest with the smallest output. Default: `2`. |
| `--zopfli` | Compress PNGs with Zopfli (default: libdeflate). Many times slower; a little smaller on most files. |
| `-j, --threads <N>` | Number of worker threads. Default: one per logical CPU. |
| `-q, --quiet` | Print the summary and errors, without a line per file. |

`--strip` removes:

- JPEG: EXIF, XMP, ICC profiles and comments.
- PNG: chunks that don't affect display, such as text, timestamps and EXIF. Color profiles (`iCCP`, `sRGB`, `cICP`), pixel density (`pHYs`) and APNG animation chunks stay.
- GIF: comment and application extensions, such as XMP.
- WebP: ICC, EXIF and XMP chunks.
- SVG: comments, `<metadata>`, `<desc>` and editor namespace data from tools such as Inkscape and Illustrator.

Exit codes:

- `0`: success.
- `1`: `--check` found a file that could shrink.
- `2`: an error, such as a missing path or a file `losslessly` couldn't process.

## Formats

### PNG and APNG

[oxipng](https://github.com/oxipng/oxipng) reduces bit depth and color type where it can and tries several filter strategies. It compresses with libdeflate, or with Zopfli if you pass `--zopfli`.

### JPEG

`losslessly` transcodes JPEGs with [mozjpeg](https://github.com/mozilla/mozjpeg) the way `jpegtran -optimize` does: it copies the DCT coefficients unchanged and rebuilds the entropy coding, without decoding to pixels. It encodes a baseline and a progressive version, both with optimized Huffman tables, and keeps the smaller one.

### GIF

`losslessly` re-encodes the animation as interframe deltas, the approach `gifsicle -O2` takes. The new file holds one global palette of the exact on-screen colors and a full first frame. For each later frame, `losslessly` stores the bounding box of the pixels that changed and marks the unchanged pixels inside it transparent. Rendered frames, delays and loop count stay the same. Animations exported as stacks of full frames can shrink by 80 to 90%.

If the frames show more than 256 distinct colors in total (255 for animated or transparent GIFs, which need a palette slot for transparency), or a frame turns opaque pixels transparent again, `losslessly` can't re-encode the GIF this way and leaves it alone.

### WebP

For lossless (VP8L) still images, libwebp re-encodes the pixels at maximum effort (method 6, quality 100) in `exact` mode, which keeps the RGB values of pixels with zero alpha. `losslessly` skips lossy (VP8) files, since re-encoding them would cost quality, and it skips animated files.

### SVG

[oxvg](https://github.com/noahbald/oxvg), a Rust port of svgo, rewrites the markup with its correctness-focused `safe` preset: it drops the doctype and whitespace, compacts paths and transforms, minifies styles, and rounds numbers to svgo's default precision (three decimal places for most values). Because of the rounding, SVG is the one format without a bit-level guarantee.

## Guarantees

- PNG, JPEG and WebP output decodes to the same pixels as the input, and GIF output renders the same frames with the same timing. SVG output renders the same within svgo's default numeric precision; see [SVG](#svg).
- `losslessly` replaces a file when the result is smaller and leaves it byte-for-byte untouched otherwise. A second run over the same files changes nothing.
- Writes go through a temporary file in the same directory: `losslessly` copies the original's permissions onto it and renames it over the original. If the process dies mid-write, the original file stays intact.
- EXIF, ICC profiles, XMP and comments stay unless you pass `--strip`.
- `losslessly` reports an error for any file it can't decode and leaves the file untouched (exit `2`). That includes truncated JPEGs, which libjpeg would pad with gray blocks after a warning: `losslessly` treats any libjpeg warning as an error.
- A PNG renamed to `photo.jpg` fails with an error and stays untouched. `losslessly` checks each file's magic bytes against its extension before decoding.

## CI and git hooks

Fail CI when an image in `assets/` could be smaller:

```yaml
- name: Check images are optimized
  run: losslessly --check assets/
```

Optimize staged images on commit with [lefthook](https://github.com/evilmartians/lefthook), which re-stages the files `losslessly` rewrote:

```yaml
# lefthook.yml
pre-commit:
  commands:
    losslessly:
      glob: "*.{png,apng,jpg,jpeg,gif,webp,svg}"
      run: losslessly {staged_files}
      stage_fixed: true
```

Without lefthook, put this in `.git/hooks/pre-commit`:

```sh
#!/bin/sh
git diff --cached --name-only --diff-filter=ACM | grep -iE '\.(a?png|jpe?g|gif|webp|svg)$' \
  | xargs -r losslessly && git update-index --again
```

## Out of scope

`losslessly` doesn't do lossy compression (quality reduction, resizing, chroma subsampling), so you can run it unattended. Format conversion, such as PNG to WebP, is out of scope as well.

AVIF and JPEG XL are not on the roadmap. Their encoders are large native dependencies, and a lossless re-encode saves little on files from modern encoders.

## Development

```sh
cargo build --release
cargo clippy --all-targets
```

`examples/` has tools for checking an optimized file against the original:

- `jpegcmp` decodes two JPEGs with libjpeg, without color management, and compares the pixel data.
- `webpcmp` decodes two WebP files with libwebp and compares the pixels.
- `gifcmp` compares two GIFs as rendered: composited frames, delays and loop count.
- `svgcmp` rasterizes two SVGs with resvg at 2x and compares the pixels. It accepts antialiasing noise: under 0.5% of bytes differing, with no channel off by more than 16.
- `gifgen` and `webpgen` write test fixtures, including files `losslessly` must leave alone: a GIF that turns pixels transparent again and a lossy WebP.

```console
$ cargo run --release --example jpegcmp -- original.jpg optimized.jpg
a: 1200x1200, b: 1200x1200
PIXELS IDENTICAL

$ cargo run --release --example gifcmp -- original.gif optimized.gif
20 frames, 200x200, repeat Infinite: RENDERS IDENTICAL
```

## Acknowledgements

The idea comes from [ImageOptim](https://imageoptim.com), the Mac app that optimizes images you drop onto its window. `losslessly` uses [oxipng](https://github.com/oxipng/oxipng), [mozjpeg](https://github.com/mozilla/mozjpeg), [libwebp](https://chromium.googlesource.com/webm/libwebp) and [oxvg](https://github.com/noahbald/oxvg) for the compression itself.

## License

MIT
