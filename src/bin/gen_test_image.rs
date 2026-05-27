use image::{Rgba, RgbaImage};

fn main() {
    let width = 512;
    let height = 512;
    let mut img = RgbaImage::new(width, height);

    // Draw background (light grey/white with a subtle gradient)
    for y in 0..height {
        for x in 0..width {
            let val = (240 + (x + y) / 32).min(255) as u8;
            img.put_pixel(x, y, Rgba([val, val, val, 255]));
        }
    }

    // Draw a dark blue circle in the center (radius 180)
    let cx = 256i32;
    let cy = 256i32;
    let radius = 180i32;
    let radius_sq = radius * radius;

    for y in 0..height {
        for x in 0..width {
            let dx = x as i32 - cx;
            let dy = y as i32 - cy;
            if dx * dx + dy * dy <= radius_sq {
                // Blue color
                img.put_pixel(x, y, Rgba([30, 80, 200, 255]));
            }
        }
    }

    // Save as test_input.png
    let path = "test_input.png";
    img.save(path).unwrap();
    println!("Generated test image at: {}", path);
}
