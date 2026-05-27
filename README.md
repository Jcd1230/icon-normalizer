# auto-icon

`auto-icon` is a fast, lightweight CLI tool written in Rust to automatically clean up raw images (like AI-generated assets) and format them into high-quality transparent, squared, and centered icons.

## Features

- **4-Corner Flood Fill**: Detects and strips the background using BFS flood-fill starting from the corners, supporting color gradients.
- **Edge Feathering**: Antialiases borders with a configurable distance-based transparency gradient to eliminate jagged edges.
- **Autocrop & Squaring**: Automatically crops the image to the bounding box of non-transparent pixels and scales it to fit a square container.
- **Premultiplied Alpha Scaling**: Prevents color bleeding and "white/black halo" artifacts during interpolation when scaling down.
- **Wayland Clipboard Support**: Copy directly from/to the clipboard with the `-c` / `--clipboard` flag (powered by `wl-clipboard`).

---

## Installation & Build

Ensure you have Rust and Cargo installed. Then run:

```bash
cargo build --release
```

The compiled binary will be located at `target/release/auto-icon`.

### Prerequisites for Clipboard Support (Wayland)
To use the clipboard features, you must have `wl-clipboard` installed on your system:
- **Arch Linux**: `sudo pacman -S wl-clipboard`
- **Fedora/Ubuntu/Debian**: `sudo apt install wl-clipboard` or `sudo dnf install wl-clipboard`

---

## Usage

```bash
auto-icon [INPUT_PATH] [FLAGS] [OPTIONS]
```

### Options
- `-o`, `--output <PATH>`: Path to the output image file.
- `-s`, `--size <PIXELS>`: Target output square size (default: `512`).
- `-t`, `--tolerance <0-255>`: Color distance tolerance for background flood fill (default: `30`).
- `-f`, `--feather <PIXELS>`: Radius for edge smoothing (default: `2`).
- `-p`, `--padding <0.0-45.0>`: Padding percentage around the icon content (default: `1.0%`).
- `--no-flood`: Skip the background removal step (useful if the image is already transparent).
- `-c`, `--clipboard`: Read input from and/or write output to the Wayland system clipboard.

### Examples

1. **Read Clipboard $\to$ Clean $\to$ Write Clipboard**:
   ```bash
   auto-icon -c
   ```
2. **Read Clipboard $\to$ Clean $\to$ Save to File**:
   ```bash
   auto-icon -c -o cleaned_icon.png
   ```
3. **Read File $\to$ Clean $\to$ Copy to Clipboard**:
   ```bash
   auto-icon raw_image.png -c
   ```
4. **Read File $\to$ Clean $\to$ Save to File (with custom size and padding)**:
   ```bash
   auto-icon raw_image.png -o cleaned_icon.png --size 1024 --padding 5.0
   ```

---

## License

MIT or Apache 2.0.
