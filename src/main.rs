use clap::Parser;
use std::path::PathBuf;
use std::process;

mod floodfill;
mod processor;

#[derive(Parser, Debug)]
#[command(name = "auto-icon")]
#[command(version)]
#[command(
    about = "Clean up and resize images to create high-quality square transparent icons.",
    long_about = "auto-icon takes an image, removes the background using flood fill, crops it to the content boundaries, pads/centers it, and resizes it to a square icon."
)]
struct Args {
    /// Path to the input image file.
    input: PathBuf,

    /// Path to the output image file (defaults to <input_stem>_icon.png).
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
    #[arg(short, long, default_value_t = 10.0)]
    padding: f32,

    /// Skip background removal step (if the image already has transparency).
    #[arg(long)]
    no_flood: bool,
}

fn main() {
    let args = Args::parse();

    // 1. Validate inputs
    if args.size == 0 {
        eprintln!("Error: Target size must be greater than 0.");
        process::exit(1);
    }
    if args.padding < 0.0 || args.padding >= 50.0 {
        eprintln!("Error: Padding must be between 0.0 and 50.0 percent.");
        process::exit(1);
    }

    // 2. Load input image
    println!("Loading image: {}", args.input.display());
    let img_source = match image::open(&args.input) {
        Ok(img) => img,
        Err(e) => {
            eprintln!("Error loading image: {}", e);
            process::exit(1);
        }
    };

    let mut img = img_source.to_rgba8();
    let (orig_w, orig_h) = img.dimensions();
    println!("Original dimensions: {}x{}", orig_w, orig_h);

    // 3. Background Removal
    if !args.no_flood {
        println!(
            "Running background removal (flood fill tolerance: {}, feather radius: {})...",
            args.tolerance, args.feather
        );
        let mask = floodfill::generate_flood_fill_mask(&img, args.tolerance);
        let feathered = floodfill::compute_feathered_mask(&mask, args.feather);
        img = floodfill::apply_mask(&img, &feathered);
    } else {
        println!("Skipping background removal.");
    }

    // 4. Bounding Box & Autocrop
    println!("Locating content bounding box...");
    let bbox = match processor::find_bounding_box(&img, 5) {
        Some(box_coords) => box_coords,
        None => {
            eprintln!("Error: Image became completely transparent (no content found).");
            process::exit(1);
        }
    };
    let (min_x, min_y, max_x, max_y) = bbox;
    println!(
        "Content bounding box: ({}, {}) to ({}, {}), width: {}, height: {}",
        min_x,
        min_y,
        max_x,
        max_y,
        max_x - min_x + 1,
        max_y - min_y + 1
    );

    let cropped = processor::crop_image(&img, bbox);

    // 5. Square, Center, Resize
    println!(
        "Centering and resizing to {}x{} with {}% padding...",
        args.size, args.size, args.padding
    );
    let result = processor::square_center_and_resize(&cropped, args.size, args.padding);

    // 6. Determine output file path
    let output_path = match args.output {
        Some(path) => path,
        None => {
            let mut parent = args.input.parent().unwrap_or(&PathBuf::new()).to_path_buf();
            let stem = args
                .input
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("output");
            parent.push(format!("{}_icon.png", stem));
            parent
        }
    };

    // 7. Save output image
    println!("Saving output to: {}", output_path.display());
    match result.save(&output_path) {
        Ok(_) => {
            println!("Done! Successfully created icon.");
        }
        Err(e) => {
            eprintln!("Error saving output image: {}", e);
            process::exit(1);
        }
    }
}
