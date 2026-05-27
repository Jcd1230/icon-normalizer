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

---

## License

MIT or Apache 2.0.
