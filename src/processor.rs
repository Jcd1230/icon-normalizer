use image::{imageops::FilterType, RgbaImage};

/// Premultiplies the alpha channel of an RGBA image in place.
/// This prevents color bleeding (halos) at transparent borders when resizing.
pub fn premultiply_alpha(img: &mut RgbaImage) {
    for pixel in img.pixels_mut() {
        let a = pixel[3] as u32;
        if a < 255 {
            pixel[0] = ((pixel[0] as u32 * a) / 255) as u8;
            pixel[1] = ((pixel[1] as u32 * a) / 255) as u8;
            pixel[2] = ((pixel[2] as u32 * a) / 255) as u8;
        }
    }
}

/// Un-premultiplies the alpha channel of an RGBA image in place.
pub fn demultiply_alpha(img: &mut RgbaImage) {
    for pixel in img.pixels_mut() {
        let a = pixel[3] as u32;
        if a > 0 && a < 255 {
            pixel[0] = ((pixel[0] as u32 * 255) / a).min(255) as u8;
            pixel[1] = ((pixel[1] as u32 * 255) / a).min(255) as u8;
            pixel[2] = ((pixel[2] as u32 * 255) / a).min(255) as u8;
        } else if a == 0 {
            pixel[0] = 0;
            pixel[1] = 0;
            pixel[2] = 0;
        }
    }
}

/// Finds the bounding box of non-transparent pixels (alpha > threshold).
/// Returns Some((min_x, min_y, max_x, max_y)), or None if no pixels match.
pub fn find_bounding_box(img: &RgbaImage, alpha_threshold: u8) -> Option<(u32, u32, u32, u32)> {
    let (width, height) = img.dimensions();
    let mut min_x = width;
    let mut max_x = 0;
    let mut min_y = height;
    let mut max_y = 0;
    let mut found = false;

    for y in 0..height {
        for x in 0..width {
            let pixel = img.get_pixel(x, y);
            if pixel[3] > alpha_threshold {
                if x < min_x { min_x = x; }
                if x > max_x { max_x = x; }
                if y < min_y { min_y = y; }
                if y > max_y { max_y = y; }
                found = true;
            }
        }
    }

    if found {
        Some((min_x, min_y, max_x, max_y))
    } else {
        None
    }
}

/// Crops the image to the specified bounding box.
pub fn crop_image(img: &RgbaImage, bbox: (u32, u32, u32, u32)) -> RgbaImage {
    let (min_x, min_y, max_x, max_y) = bbox;
    let width = max_x - min_x + 1;
    let height = max_y - min_y + 1;
    let mut cropped = RgbaImage::new(width, height);
    for y in 0..height {
        for x in 0..width {
            cropped.put_pixel(x, y, *img.get_pixel(min_x + x, min_y + y));
        }
    }
    cropped
}

/// Scales, centers, and pads a cropped image into a square canvas of the target size.
/// padding_percent specifies the border on each side (e.g. 10.0 for 10% padding).
pub fn square_center_and_resize(
    cropped_img: &RgbaImage,
    target_size: u32,
    padding_percent: f32,
) -> RgbaImage {
    // 1. Calculate target dimensions for the active content box
    let padding_fraction = padding_percent / 100.0;
    let inner_box_size = (target_size as f32 * (1.0 - 2.0 * padding_fraction))
        .max(1.0)
        .round() as u32;

    // 2. Scale cropped image to fit within the inner_box_size, preserving aspect ratio
    let (w, h) = cropped_img.dimensions();
    let scale_factor = (inner_box_size as f32 / w as f32)
        .min(inner_box_size as f32 / h as f32);

    let new_w = (w as f32 * scale_factor).round().max(1.0) as u32;
    let new_h = (h as f32 * scale_factor).round().max(1.0) as u32;

    // Make sure we premultiply before resizing
    let mut to_resize = cropped_img.clone();
    premultiply_alpha(&mut to_resize);

    // Resize using high-quality Lanczos3 filter
    let resized = image::imageops::resize(&to_resize, new_w, new_h, FilterType::Lanczos3);

    // 3. Create destination canvas (transparent square)
    let mut canvas = RgbaImage::new(target_size, target_size);

    // 4. Paste resized image centered in the canvas
    let x_offset = (target_size - new_w) / 2;
    let y_offset = (target_size - new_h) / 2;

    for y in 0..new_h {
        for x in 0..new_w {
            canvas.put_pixel(x_offset + x, y_offset + y, *resized.get_pixel(x, y));
        }
    }

    // 5. Un-premultiply alpha to get standard colors back
    demultiply_alpha(&mut canvas);

    canvas
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn test_premultiply_demultiply() {
        let mut img = RgbaImage::new(1, 1);
        img.put_pixel(0, 0, Rgba([200, 100, 50, 128]));
        premultiply_alpha(&mut img);
        // A=128: R' = (200 * 128) / 255 = 100.39 -> 100
        assert_eq!(img.get_pixel(0, 0)[0], 100);
        assert_eq!(img.get_pixel(0, 0)[3], 128);

        demultiply_alpha(&mut img);
        // R'' = (100 * 255) / 128 = 199.21 -> 199 (due to rounding/truncation)
        assert!(img.get_pixel(0, 0)[0] >= 198 && img.get_pixel(0, 0)[0] <= 200);
    }

    #[test]
    fn test_bounding_box_and_crop() {
        let mut img = RgbaImage::new(20, 20);
        // Place content at (5, 5) to (14, 9)
        for y in 5..=9 {
            for x in 5..=14 {
                img.put_pixel(x, y, Rgba([255, 0, 0, 255]));
            }
        }

        let bbox = find_bounding_box(&img, 0).expect("Should find bbox");
        assert_eq!(bbox, (5, 5, 14, 9));

        let cropped = crop_image(&img, bbox);
        assert_eq!(cropped.width(), 10);
        assert_eq!(cropped.height(), 5);
        assert_eq!(cropped.get_pixel(0, 0)[0], 255);
    }

    #[test]
    fn test_square_center_and_resize() {
        let mut img = RgbaImage::new(10, 10);
        // Fill completely
        for y in 0..10 {
            for x in 0..10 {
                img.put_pixel(x, y, Rgba([0, 255, 0, 255]));
            }
        }

        // Square and resize to 100x100 with 10% padding
        // Content should occupy inner 80x80 box
        let squared = square_center_and_resize(&img, 100, 10.0);
        assert_eq!(squared.width(), 100);
        assert_eq!(squared.height(), 100);

        // Check that the borders are transparent due to 10% padding
        // 10% of 100 is 10 pixels on each side, so border is 0..10 and 90..100
        assert_eq!(squared.get_pixel(5, 5)[3], 0);
        assert_eq!(squared.get_pixel(95, 95)[3], 0);

        // Center pixel should be opaque green
        assert_eq!(squared.get_pixel(50, 50)[1], 255);
        assert_eq!(squared.get_pixel(50, 50)[3], 255);
    }
}

