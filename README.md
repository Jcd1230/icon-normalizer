# icon-normalizer

`icon-normalizer` is a fast, lightweight CLI tool written in Rust to automatically clean up raw images (like AI-generated assets) and format them into high-quality transparent, squared, and centered icons.

## Features

- **4-Corner Flood Fill**: Detects and strips the background using BFS flood-fill starting from the corners, supporting color gradients.
- **Edge Feathering**: Antialiases borders with a configurable distance-based transparency gradient to eliminate jagged edges.
- **Autocrop & Squaring**: Automatically crops the image to the bounding box of non-transparent pixels and scales it to fit a square container.
- **Premultiplied Alpha Scaling**: Prevents color bleeding and "white/black halo" artifacts during interpolation when scaling down.
- **1.0% Default Padding**: Fits the icon tightly into its container, maximizing canvas space.
- **Cross-Platform Clipboard Support**: Copy directly from/to the clipboard with the `-c` / `--clipboard` flag on Linux, macOS, and Windows.
- **Configurable Output Formats**: Export to multiple file formats including PNG, JPEG, WebP, ICO, and BMP using the `--format` flag.
- **Shell Completions**: Easily generate shell completions for bash, zsh, fish, powershell, and elvish.
- **Sticker Sheet Splitting**: Isolate sticker bodies and detached accents by their alpha masks, then crop, center, and resize each group. Includes count checks, layout hints, diagnostic previews, and explicit island assignments.
- **Verbose & Quiet Logging Modes**: Track precise stage-by-stage timings with `-v` / `--verbose`, or run silently with `-q` / `--quiet`.

---

## Installation & Build

Ensure you have Rust and Cargo installed. Then run:

```bash
cargo build --release
```

The compiled binary will be located at `target/release/icon-normalizer`.

### Generating Shell Completions

You can generate completion scripts for your favorite shell and source them. For example, for the `fish` shell:

```bash
icon-normalizer completions fish > ~/.config/fish/completions/icon-normalizer.fish
```

Supports `bash`, `zsh`, `fish`, `powershell`, and `elvish`.

### Prerequisites for Clipboard Support
The clipboard feature is dependency-free at compile-time and routes clipboard actions dynamically to the platform's native tools:
- **Linux (Wayland)**: Requires `wl-clipboard` (runs `wl-paste`/`wl-copy`).
- **Linux (X11)**: Requires `xclip` (runs `xclip`).
- **macOS**: Built-in (routes hex PNG data natively via AppleScript/`osascript`).
- **Windows**: Built-in (routes PNG bytes natively via PowerShell).

To install requirements on Linux:
- **Arch Linux**: `sudo pacman -S wl-clipboard xclip`
- **Fedora/RHEL**: `sudo dnf install wl-clipboard xclip`
- **Ubuntu/Debian**: `sudo apt install wl-clipboard xclip`

---

## Usage

```bash
icon-normalizer [INPUT_PATH] [FLAGS] [OPTIONS]
icon-normalizer completions <SHELL>
icon-normalizer split <SHEET> [OPTIONS]
```

### Options
- `-o`, `--output <PATH>`: Path to the output image file (defaults to `<input_stem>_icon.<format>`).
- `-s`, `--size <PIXELS>`: Target output square size (default: `512`).
- `-t`, `--tolerance <0-255>`: Color distance tolerance for background flood fill (default: `30`).
- `-f`, `--feather <PIXELS>`: Radius for edge smoothing (default: `2`).
- `-p`, `--padding <0.0-45.0>`: Padding percentage around the icon content (default: `1.0%`).
- `--no-flood`: Skip the background removal step (useful if the image is already transparent).
- `-c`, `--clipboard`: Read input from and/or write output to the system clipboard.
- `--format <png|jpeg|webp|ico|bmp>`: Force output format (inferred from output extension if omitted, defaults to `png`).
- `-v`, `--verbose`: Show detailed debug logs with execution time metrics.
- `-q`, `--quiet`: Run silently, suppressing all logs except errors.

### Examples

1. **Full Clipboard Pipeline (Read Clipboard $\to$ Clean $\to$ Write Clipboard)**:
   ```bash
   icon-normalizer -c
   ```
2. **Read Clipboard $\to$ Clean $\to$ Save to File (automatically defaults name to webp)**:
   ```bash
   icon-normalizer -c --format webp
   ```
3. **Read File $\to$ Clean $\to$ Save to File as WebP**:
   ```bash
   icon-normalizer raw_image.png -o cleaned_icon.webp
   ```
4. **Generate ICO format at size 256**:
   ```bash
   icon-normalizer raw_image.png --format ico --size 256
   ```
5. **Run in Verbose Mode with stage timing metrics**:
   ```bash
   icon-normalizer raw_image.png -v
   ```

### Splitting sticker sheets

For a transparent sheet with eight stickers arranged in four columns and two rows:

```bash
icon-normalizer split sheet.png -o stickers \
  --grid 4x2 --count 8 --size 512 --format webp \
  --preview grouping.png --report components.tsv
```

This produces `stickers/sticker-01.webp` through `sticker-08.webp`, in reading order.
Every file is transparent, cropped to its assigned pixels, scaled with premultiplied
alpha, and centered on a square canvas. The defaults are **512×512**, **lossless WebP**,
and **3.125% padding per side** (16 pixels at size 512, fitting content within 480×480).
Use `--prefix turtle` for names such as `turtle-01.webp`, or `--format png`.
Existing files are protected unless `--overwrite` is supplied. The input sheet is
always protected. Background removal is **off** for `split`; enable
`--remove-background` for an opaque sheet, with `--tolerance` and `--feather` if needed.

#### Detached hearts, stars, drops, and other islands

The splitter labels **four-connected regions with alpha > 32**. Large regions
(at least 15% of the largest region's pixel area) are main stickers. A main sticker
is never merged into another main sticker. Smaller regions are accessories or noise:

- Regions below `--min-area` (default 32 source pixels) are discarded as noise.
- Without a grid hint, an island is assigned **as a whole** to the nearest main
  sticker, measuring the shortest Euclidean distance between their actual pixels.
  This avoids using misleading bounding boxes or center distances.
- With `--grid COLSxROWS`, main sticker centers must occupy one cell each. An island's
  center determines its sticker cell. This is a **layout assumption**, useful when
  you know each sticker's decorations belong within its cell. It does not cut the
  image into rectangles: bodies may cross cell boundaries, and overlapping bounding
  boxes never include a neighbor's pixels. Omit the hint if decorations cross cells.
- Islands farther than `--island-gap` (default 80 source pixels) are discarded with
  a warning. Increase this for decorations that intentionally float farther away.
- Without a grid, if the two closest distances differ by at most
  `--ambiguity-margin` (default 4 source pixels), ownership is unresolved. No individual
  images are written until the ambiguity is resolved. Requested diagnostics are still
  written so you can review the candidates.
- Faint antialiasing is recovered within `--edge-radius` (default 2 source pixels)
  of retained content. Original RGBA colors and alpha are preserved. Opaque speckles
  and discarded islands are not recovered by this pass.

Pixel distance cannot prove which turtle a heart belongs to. The preview and explicit
assignments are the way to resolve semantic uncertainty, including a nearby island
that confidently belongs to the wrong sticker geometrically. `--count` is a useful
guard against unexpectedly merged or missing bodies; it does not validate emotions
or accessory ownership.

```bash
# Inspect segmentation before exporting; no individual icons are written.
icon-normalizer split sheet.png --count 8 --dry-run \
  --preview grouping.png --report components.tsv

# Send component 12 to sticker 3 and discard component 19.
icon-normalizer split sheet.png -o stickers --count 8 \
  --assign 12=3 --assign 19=0
```

The preview labels component IDs at their bounding boxes and sticker numbers near
the main bodies' centers. Box colors indicate groups; unresolved/discarded non-noise
regions are red. `components.tsv` records each component's pixel area, source bounds,
sticker number, and assignment/discard status. IDs are deterministic for the same
input and segmentation settings; changing threshold or erosion can change them.
`--assign` overrides are checked and apply only to islands, not main bodies. They
can also rescue an island otherwise excluded by the noise or distance limits.

#### Touching stickers and uneven layouts

If two sticker borders touch, they can become one connected component. With
`--count` or `--grid`, the splitter refuses an incorrect body count. Adjust the alpha
threshold for faint bridges, or use explicit seed erosion for thicker narrow bridges:

```bash
icon-normalizer split sheet.png -o stickers --grid 4x2 --count 8 \
  --erosion 12 --preview grouping.png
```

Erosion shrinks only the **detection seeds**, then restores the original pixels to
the nearest separated seed within that original connected region. Detached accents
erased by erosion are retained as candidate islands. Borders are not permanently
shrunk. Large erosion values may divide a single sticker into extra regions, so
inspect the preview. No geometric method can reliably separate substantially
overlapping artwork; such sheets need a manual mask or a new generation with spacing.

If the main bodies differ greatly in size, adjust `--anchor-ratio` (default `0.15`).
Without a grid, stickers are ordered by approximate rows and then left-to-right;
use a grid hint for explicit regular ordering. Get all options with
`icon-normalizer split --help`.

The feature is native Rust and uses the existing `image` and `clap` dependencies;
Python, NumPy, and SciPy are not runtime requirements.

---

## License

MIT or Apache 2.0.
