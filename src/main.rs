use clap::Parser;
use std::io::Write;
use std::path::PathBuf;
use std::process;

mod floodfill;
mod processor;

#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
enum OutputFormat {
    Png,
    Jpeg,
    Webp,
    Ico,
    Bmp,
}

impl OutputFormat {
    fn to_image_format(self) -> image::ImageFormat {
        match self {
            OutputFormat::Png => image::ImageFormat::Png,
            OutputFormat::Jpeg => image::ImageFormat::Jpeg,
            OutputFormat::Webp => image::ImageFormat::WebP,
            OutputFormat::Ico => image::ImageFormat::Ico,
            OutputFormat::Bmp => image::ImageFormat::Bmp,
        }
    }

    fn extension(self) -> &'static str {
        match self {
            OutputFormat::Png => "png",
            OutputFormat::Jpeg => "jpg",
            OutputFormat::Webp => "webp",
            OutputFormat::Ico => "ico",
            OutputFormat::Bmp => "bmp",
        }
    }
}

#[derive(clap::Subcommand, Debug)]
enum Commands {
    /// Generate shell completions for the specified shell.
    Completions {
        /// The shell to generate completions for.
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
}

#[derive(Parser, Debug)]
#[command(name = "icon-normalizer")]
#[command(version)]
#[command(
    about = "Clean up and resize images to create high-quality square transparent icons.",
    long_about = "icon-normalizer takes an image, removes the background using flood fill, crops it to the content boundaries, pads/centers it, and resizes it to a square icon."
)]
struct Args {
    /// Path to the input image file. If omitted, and --clipboard is set, reads from clipboard.
    input: Option<PathBuf>,

    /// Path to the output image file (defaults to <input_stem>_icon.<format>).
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Target output square size in pixels (e.g., 256, 512, 1024).
    #[arg(short, long, default_value_t = 512)]
    size: u32,

    /// Color distance tolerance for flood-fill background removal (0 to 255).
    #[arg(short, long, default_value_t = 30)]
    tolerance: u32,

    /// Radius in pixels for edge feathering/smoothing (0 for no smoothing).
    #[arg(short, long, default_value_t = 2)]
    feather: u32,

    /// Padding percentage around the icon content (relative to square side, 0.0 to 45.0).
    #[arg(short, long, default_value_t = 1.0)]
    padding: f32,

    /// Skip background removal step (if the image already has transparency).
    #[arg(long)]
    no_flood: bool,

    /// Read input from and/or write output to the Wayland system clipboard (using wl-paste/wl-copy).
    #[arg(short, long)]
    clipboard: bool,

    /// Force output format (png, jpeg, webp, ico, bmp). Defaults to png if output format cannot be inferred.
    #[arg(long, value_enum)]
    format: Option<OutputFormat>,

    /// Show verbose debug output, including timing information.
    #[arg(short, long, conflicts_with = "quiet")]
    verbose: bool,

    /// Suppress all output except errors.
    #[arg(short, long, conflicts_with = "verbose")]
    quiet: bool,

    #[command(subcommand)]
    command: Option<Commands>,
}

struct Logger {
    verbose: bool,
    quiet: bool,
}

impl Logger {
    fn new(verbose: bool, quiet: bool) -> Self {
        Self { verbose, quiet }
    }

    fn info(&self, msg: impl AsRef<str>) {
        if !self.quiet {
            println!("{}", msg.as_ref());
        }
    }

    fn debug(&self, msg: impl AsRef<str>) {
        if self.verbose && !self.quiet {
            println!("[DEBUG] {}", msg.as_ref());
        }
    }

    fn error(&self, msg: impl AsRef<str>) {
        eprintln!("Error: {}", msg.as_ref());
    }
}

#[cfg(target_os = "macos")]
fn hex_decode(s: &str) -> Result<Vec<u8>, std::num::ParseIntError> {
    let cleaned: String = s.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    (0..cleaned.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&cleaned[i..i + 2], 16))
        .collect()
}

fn read_clipboard_image() -> Vec<u8> {
    #[cfg(target_os = "linux")]
    {
        let is_wayland = std::env::var("WAYLAND_DISPLAY").is_ok();
        if is_wayland {
            let output = process::Command::new("wl-paste")
                .arg("-t")
                .arg("image/png")
                .output();
            if let Ok(out) = output {
                if out.status.success() {
                    return out.stdout;
                }
            }
        }

        // Fallback to xclip
        let output = process::Command::new("xclip")
            .arg("-selection")
            .arg("clipboard")
            .arg("-t")
            .arg("image/png")
            .arg("-o")
            .output();
        match output {
            Ok(out) if out.status.success() => return out.stdout,
            Ok(out) => {
                let err_msg = String::from_utf8_lossy(&out.stderr);
                eprintln!("xclip failed: {}", err_msg);
            }
            Err(e) => {
                eprintln!("Failed to execute xclip: {}", e);
            }
        }

        eprintln!("Error: Clipboard access failed. Make sure wl-clipboard (for Wayland) or xclip (for X11) is installed.");
        process::exit(1);
    }

    #[cfg(target_os = "macos")]
    {
        // AppleScript outputs hex string starting with "«data PNGf..." and ending with "»"
        let output = process::Command::new("osascript")
            .arg("-e")
            .arg("get the clipboard as «class PNGf»")
            .output();
        if let Ok(out) = output {
            if out.status.success() {
                let hex_str = String::from_utf8_lossy(&out.stdout);
                if let Some(start) = hex_str.find("«data PNGf") {
                    let hex_content = &hex_str[start + 10..];
                    if let Some(end) = hex_content.find('»') {
                        let hex = &hex_content[..end];
                        if let Ok(bytes) = hex_decode(hex) {
                            return bytes;
                        }
                    }
                }
            }
        }
        eprintln!("Error: Failed to read image from macOS clipboard using AppleScript.");
        process::exit(1);
    }

    #[cfg(target_os = "windows")]
    {
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("icon_normalizer_clip.png");
        let ps_script = format!(
            "Add-Type -AssemblyName System.Windows.Forms; \
             $img = [System.Windows.Forms::Clipboard]::GetImage(); \
             if ($img) {{ $img.Save('{}', [System.Drawing.Imaging.ImageFormat]::Png) }}",
            temp_file.to_string_lossy().replace('\'', "''")
        );
        let output = process::Command::new("powershell")
            .arg("-Command")
            .arg(&ps_script)
            .output();
        if let Ok(out) = output {
            if out.status.success() && temp_file.exists() {
                if let Ok(bytes) = std::fs::read(&temp_file) {
                    let _ = std::fs::remove_file(&temp_file);
                    return bytes;
                }
            }
        }
        eprintln!("Error: Failed to read image from Windows clipboard using PowerShell.");
        process::exit(1);
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        eprintln!("Error: Clipboard integration not supported on this platform.");
        process::exit(1);
    }
}

fn write_clipboard_image(img: &image::RgbaImage, format: OutputFormat, logger: &Logger) {
    let mut img_bytes = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut img_bytes);
    let image_format = format.to_image_format();
    if let Err(e) = img.write_to(&mut cursor, image_format) {
        eprintln!("Failed to encode output image: {}", e);
        process::exit(1);
    }

    #[cfg(target_os = "linux")]
    {
        let is_wayland = std::env::var("WAYLAND_DISPLAY").is_ok();
        let cmd = if is_wayland { "wl-copy" } else { "xclip" };
        let mut child_cmd = process::Command::new(cmd);
        
        let mime_type = match format {
            OutputFormat::Png => "image/png",
            OutputFormat::Jpeg => "image/jpeg",
            OutputFormat::Webp => "image/webp",
            OutputFormat::Ico => "image/x-icon",
            OutputFormat::Bmp => "image/bmp",
        };

        if is_wayland {
            child_cmd.arg("-t").arg(mime_type);
        } else {
            child_cmd.arg("-selection").arg("clipboard").arg("-t").arg(mime_type);
        }

        let mut child = child_cmd
            .stdin(process::Stdio::piped())
            .spawn()
            .unwrap_or_else(|e| {
                eprintln!("Failed to execute {}: {}", cmd, e);
                process::exit(1);
            });

        {
            let stdin = child.stdin.as_mut().unwrap();
            if let Err(e) = stdin.write_all(&img_bytes) {
                eprintln!("Failed to write image data to {} stdin: {}", cmd, e);
                process::exit(1);
            }
        }

        match child.wait() {
            Ok(status) if status.success() => {
                logger.info("Output successfully copied to clipboard.");
            }
            Ok(status) => {
                eprintln!("{} exited with error status: {}", cmd, status);
                process::exit(1);
            }
            Err(e) => {
                eprintln!("Failed to wait for {}: {}", cmd, e);
                process::exit(1);
            }
        }
        return;
    }

    #[cfg(target_os = "macos")]
    {
        let (temp_ext, apple_class, encoded_bytes) = match format {
            OutputFormat::Jpeg => ("jpg", "JPEG picture", img_bytes),
            _ => {
                if format == OutputFormat::Png {
                    ("png", "«class PNGf»", img_bytes)
                } else {
                    let mut png_fallback = Vec::new();
                    let mut cursor = std::io::Cursor::new(&mut png_fallback);
                    let _ = img.write_to(&mut cursor, image::ImageFormat::Png);
                    ("png", "«class PNGf»", png_fallback)
                }
            }
        };

        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join(format!("icon_normalizer_out.{}", temp_ext));
        if std::fs::write(&temp_file, &encoded_bytes).is_ok() {
            let script = format!(
                "set the clipboard to (read (POSIX file \"{}\") as {})",
                temp_file.to_string_lossy(),
                apple_class
            );
            let output = process::Command::new("osascript")
                .arg("-e")
                .arg(&script)
                .output();
            let _ = std::fs::remove_file(&temp_file);
            if let Ok(out) = output {
                if out.status.success() {
                    logger.info("Output successfully copied to clipboard.");
                    return;
                }
            }
        }
        eprintln!("Error: Failed to copy image to macOS clipboard using AppleScript.");
        process::exit(1);
    }

    #[cfg(target_os = "windows")]
    {
        let (temp_ext, encoded_bytes) = match format {
            OutputFormat::Jpeg => ("jpg", img_bytes),
            OutputFormat::Bmp => ("bmp", img_bytes),
            _ => {
                if format == OutputFormat::Png {
                    ("png", img_bytes)
                } else {
                    let mut png_fallback = Vec::new();
                    let mut cursor = std::io::Cursor::new(&mut png_fallback);
                    let _ = img.write_to(&mut cursor, image::ImageFormat::Png);
                    ("png", png_fallback)
                }
            }
        };

        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join(format!("icon_normalizer_out.{}", temp_ext));
        if std::fs::write(&temp_file, &encoded_bytes).is_ok() {
            let ps_script = format!(
                "Add-Type -AssemblyName System.Windows.Forms; \
                 $img = [System.Drawing.Image]::FromFile('{}'); \
                 [System.Windows.Forms::Clipboard]::SetImage($img); \
                 $img.Dispose();",
                temp_file.to_string_lossy().replace('\'', "''")
            );
            let output = process::Command::new("powershell")
                .arg("-Command")
                .arg(&ps_script)
                .output();
            let _ = std::fs::remove_file(&temp_file);
            if let Ok(out) = output {
                if out.status.success() {
                    logger.info("Output successfully copied to clipboard.");
                    return;
                }
            }
        }
        eprintln!("Error: Failed to copy image to Windows clipboard using PowerShell.");
        process::exit(1);
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        eprintln!("Error: Clipboard integration not supported on this platform.");
        process::exit(1);
    }
}

fn main() {
    let args = Args::parse();

    // Handle completions subcommand first before any other validation or execution
    if let Some(Commands::Completions { shell }) = args.command {
        use clap::CommandFactory;
        let mut cmd = Args::command();
        let name = cmd.get_name().to_string();
        clap_complete::generate(shell, &mut cmd, name, &mut std::io::stdout());
        return;
    }

    let logger = Logger::new(args.verbose, args.quiet);

    // 1. Validate inputs
    if args.size == 0 {
        logger.error("Target size must be greater than 0.");
        process::exit(1);
    }
    if args.padding < 0.0 || args.padding >= 50.0 {
        logger.error("Padding must be between 0.0 and 50.0 percent.");
        process::exit(1);
    }

    let read_from_clipboard = args.clipboard && args.input.is_none();
    let write_to_clipboard = args.clipboard && args.output.is_none();

    if !read_from_clipboard && args.input.is_none() {
        logger.error("Please specify an input image path, or use the --clipboard / -c flag to read from clipboard.");
        process::exit(1);
    }

    // Determine the target output format
    let format = args.format.unwrap_or_else(|| {
        if let Some(ref out_path) = args.output {
            if let Ok(inferred) = image::ImageFormat::from_path(out_path) {
                match inferred {
                    image::ImageFormat::Png => OutputFormat::Png,
                    image::ImageFormat::Jpeg => OutputFormat::Jpeg,
                    image::ImageFormat::WebP => OutputFormat::Webp,
                    image::ImageFormat::Ico => OutputFormat::Ico,
                    image::ImageFormat::Bmp => OutputFormat::Bmp,
                    _ => OutputFormat::Png, // Default fallback
                }
            } else {
                OutputFormat::Png
            }
        } else {
            OutputFormat::Png
        }
    });

    // 2. Load input image
    let load_start = std::time::Instant::now();
    let img_source = if read_from_clipboard {
        logger.info("Reading image from clipboard...");
        let bytes = read_clipboard_image();
        match image::load_from_memory(&bytes) {
            Ok(img) => img,
            Err(e) => {
                logger.error(format!("Error decoding image from clipboard: {}", e));
                process::exit(1);
            }
        }
    } else {
        let input_path = args.input.as_ref().unwrap();
        logger.info(format!("Loading image: {}", input_path.display()));
        match image::open(input_path) {
            Ok(img) => img,
            Err(e) => {
                logger.error(format!("Error loading image: {}", e));
                process::exit(1);
            }
        }
    };
    logger.debug(format!("Image loaded in {:?}", load_start.elapsed()));

    let mut img = img_source.to_rgba8();
    let (orig_w, orig_h) = img.dimensions();
    logger.debug(format!("Original dimensions: {}x{}", orig_w, orig_h));

    // 3. Background Removal
    if !args.no_flood {
        logger.info(format!(
            "Running background removal (flood fill tolerance: {}, feather radius: {})...",
            args.tolerance, args.feather
        ));
        let flood_start = std::time::Instant::now();
        let mask = floodfill::generate_flood_fill_mask(&img, args.tolerance);
        let feathered = floodfill::compute_feathered_mask(&mask, args.feather);
        img = floodfill::apply_mask(&img, &feathered);
        logger.debug(format!("Background removal completed in {:?}", flood_start.elapsed()));
    } else {
        logger.info("Skipping background removal.");
    }

    // 4. Bounding Box & Autocrop
    logger.info("Locating content bounding box...");
    let crop_start = std::time::Instant::now();
    let bbox = match processor::find_bounding_box(&img, 5) {
        Some(box_coords) => box_coords,
        None => {
            logger.error("Image became completely transparent (no content found).");
            process::exit(1);
        }
    };
    let (min_x, min_y, max_x, max_y) = bbox;
    logger.debug(format!(
        "Content bounding box: ({}, {}) to ({}, {}), width: {}, height: {}",
        min_x,
        min_y,
        max_x,
        max_y,
        max_x - min_x + 1,
        max_y - min_y + 1
    ));

    let cropped = processor::crop_image(&img, bbox);
    logger.debug(format!("Autocrop completed in {:?}", crop_start.elapsed()));

    // 5. Square, Center, Resize
    logger.info(format!(
        "Centering and resizing to {}x{} with {}% padding...",
        args.size, args.size, args.padding
    ));
    let resize_start = std::time::Instant::now();
    let result = processor::square_center_and_resize(&cropped, args.size, args.padding);
    logger.debug(format!("Centering and resizing completed in {:?}", resize_start.elapsed()));

    // 6. Save or copy output
    let save_start = std::time::Instant::now();
    if write_to_clipboard {
        write_clipboard_image(&result, format, &logger);
        logger.debug(format!("Clipboard write completed in {:?}", save_start.elapsed()));
    } else {
        let output_path = match &args.output {
            Some(path) => path.clone(),
            None => {
                let input_path = args.input.as_ref().unwrap();
                let mut parent = input_path.parent().unwrap_or(&PathBuf::new()).to_path_buf();
                let stem = input_path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("output");
                parent.push(format!("{}_icon.{}", stem, format.extension()));
                parent
            }
        };

        logger.info(format!("Saving output to: {}", output_path.display()));
        let image_format = format.to_image_format();
        match result.save_with_format(&output_path, image_format) {
            Ok(_) => {
                logger.info("Done! Successfully created icon.");
            }
            Err(e) => {
                logger.error(format!("Error saving output image: {}", e));
                process::exit(1);
            }
        }
        logger.debug(format!("File save completed in {:?}", save_start.elapsed()));
    }
}
