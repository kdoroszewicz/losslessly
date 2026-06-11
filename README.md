# iopt

Lossless image optimizer for CI pipelines and pre-commit hooks.

`iopt` recompresses PNG and JPEG files in place without touching a single pixel — like [ImageOptim](https://imageoptim.com), but as a fast, dependency-free CLI you can drop into a CI job or a git hook. Files are only rewritten when the result is smaller, so running it twice is a no-op.

```console
$ iopt assets/
assets/icons/icon.png  27.3 KB → 23.4 KB  (-14.3%)
assets/photo.jpg  149.5 KB → 135.4 KB  (-9.5%)
assets/photo.png  653.1 KB → 472.0 KB  (-27.7%)
3 optimized, 0 already optimal, 199.1 KB saved
```

## Why

Images are usually the heaviest assets in a repo, and most of them ship with 10–30% of pure waste: unoptimized deflate streams, non-optimized Huffman tables, baseline instead of progressive encoding. Designers' export tools don't care; your bundle size does.

`iopt` removes that waste **losslessly** — the decoded pixels are bit-for-bit identical before and after. There is no quality slider because nothing is ever degraded.

- **PNG** — recompressed with [oxipng](https://github.com/oxipng/oxipng) (bit-depth/color-type reductions, filter trials, libdeflate or Zopfli).
- **JPEG** — transcoded with [mozjpeg](https://github.com/mozilla/mozjpeg) the same way `jpegtran -optimize` works: DCT coefficients are copied verbatim and only the entropy coding is rebuilt. Both optimized-baseline and progressive variants are tried; the smaller one wins.
- **Metadata is preserved by default** (EXIF, ICC profiles, comments survive byte-for-byte). Pass `--strip` if you want it gone.
- Parallel across files, atomic writes (temp file + rename — a crash can never leave a truncated image), and corrupt files are refused rather than silently "fixed".

## Install

```sh
cargo install --path .
```

Requires a Rust toolchain. The mozjpeg C library is built and statically linked automatically; no system dependencies are needed.

## Usage

```sh
iopt assets/ static/logo.png     # optimize files and directories in place
iopt --check assets/             # CI gate: write nothing, exit 1 if anything is optimizable
iopt --strip assets/             # also remove EXIF/ICC/comments
iopt --zopfli --level 6 assets/  # squeeze PNGs as hard as possible (slow)
```

| Option | Description |
| --- | --- |
| `--check` | Dry run. Exit `1` if any file could be smaller — fail the build, fix locally. |
| `--strip` | Strip metadata. JPEG: EXIF, ICC, comments. PNG: non-essential chunks. |
| `--level <0-6>` | PNG effort preset (default `2`; `6` is slowest/smallest). |
| `--zopfli` | Use Zopfli for PNG deflate. Much slower, usually a bit smaller. |
| `-j, --threads <N>` | Limit parallelism (default: all logical CPUs). |
| `-q, --quiet` | Only print the summary and errors. |

Exit codes: `0` success · `1` `--check` found optimizable files · `2` one or more files failed.

### CI (GitHub Actions)

Fail the build when someone commits an unoptimized image:

```yaml
- name: Check images are optimized
  run: iopt --check assets/
```

### pre-commit

With the [pre-commit](https://pre-commit.com) framework — `iopt` rewrites the files and pre-commit flags the modified ones:

```yaml
- repo: local
  hooks:
    - id: iopt
      name: optimize images
      entry: iopt
      language: system
      files: \.(png|jpe?g)$
```

Or as a plain git hook in `.git/hooks/pre-commit`:

```sh
#!/bin/sh
git diff --cached --name-only --diff-filter=ACM | grep -iE '\.(png|jpe?g)$' \
  | xargs -r iopt && git update-index --again
```

## Guarantees

- **Pixels are never modified.** PNGs are recompressed losslessly; JPEGs never go through a decode–encode cycle — the frequency-domain coefficients are copied untouched.
- **Files only shrink.** If recompression doesn't help, the file is left exactly as it was.
- **Writes are atomic.** Output goes to a temp file in the same directory and is renamed over the original, preserving permissions.
- **Corrupt input is rejected.** libjpeg normally pads truncated files with gray blocks and carries on; `iopt` treats decoder warnings as errors and refuses to rewrite such files (exit `2`).
- **Mislabeled files are skipped.** Content is sniffed by magic bytes, so a PNG named `.jpg` is reported instead of fed to the wrong codec.

## Non-goals

Lossy compression (quality reduction, resizing, chroma subsampling) is out of scope by design — `iopt` is meant to be safe to run automatically on every commit. GIF, WebP and SVG support may come later.

## Development

```sh
cargo build --release
cargo clippy --all-targets
```

`examples/jpegcmp.rs` decodes two JPEGs through libjpeg with no color management and verifies the pixel data is identical — useful for convincing yourself (or reviewers) that the JPEG path really is lossless:

```console
$ cargo run --release --example jpegcmp -- original.jpg optimized.jpg
a: 1200x1200, b: 1200x1200
PIXELS IDENTICAL
```

## License

MIT
