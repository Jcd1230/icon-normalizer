use image::{GrayImage, RgbaImage, Luma};
use std::collections::VecDeque;

/// Calculates the squared Euclidean distance between two RGBA pixels (ignoring alpha).
fn color_distance_sq(p1: &image::Rgba<u8>, p2: &image::Rgba<u8>) -> u32 {
    let r_diff = p1[0] as i32 - p2[0] as i32;
    let g_diff = p1[1] as i32 - p2[1] as i32;
    let b_diff = p1[2] as i32 - p2[2] as i32;
    (r_diff * r_diff + g_diff * g_diff + b_diff * b_diff) as u32
}

/// Generates a binary mask of the foreground (255) and background (0)
/// by performing flood fill from the 4 corners.
pub fn generate_flood_fill_mask(img: &RgbaImage, tolerance: u32) -> GrayImage {
    let (width, height) = img.dimensions();
    let mut mask = GrayImage::from_pixel(width, height, Luma([255]));
    let mut visited = vec![vec![false; height as usize]; width as usize];
    let mut queue = VecDeque::new();

    let tolerance_sq = tolerance * tolerance;

    // Define starting points (the 4 corners)
    let corners = [
        (0, 0),
        (width - 1, 0),
        (0, height - 1),
        (width - 1, height - 1),
    ];

    for &(cx, cy) in &corners {
        if cx < width && cy < height && !visited[cx as usize][cy as usize] {
            let start_color = img.get_pixel(cx, cy);
            queue.push_back((cx, cy, *start_color));
            visited[cx as usize][cy as usize] = true;
            mask.put_pixel(cx, cy, Luma([0]));
        }
    }

    while let Some((x, y, start_color)) = queue.pop_front() {
        // Check 4-connected neighbors
        let neighbors = [
            (x as i32 - 1, y as i32),
            (x as i32 + 1, y as i32),
            (x as i32, y as i32 - 1),
            (x as i32, y as i32 + 1),
        ];

        for &(nx, ny) in &neighbors {
            if nx >= 0 && nx < width as i32 && ny >= 0 && ny < height as i32 {
                let ux = nx as u32;
                let uy = ny as u32;
                if !visited[ux as usize][uy as usize] {
                    let pixel_color = img.get_pixel(ux, uy);
                    if color_distance_sq(pixel_color, &start_color) <= tolerance_sq {
                        visited[ux as usize][uy as usize] = true;
                        mask.put_pixel(ux, uy, Luma([0]));
                        queue.push_back((ux, uy, start_color));
                    }
                }
            }
        }
    }

    mask
}

/// Applies distance-based feathering to the alpha mask.
/// Foreground pixels (255) that are close to background pixels (0)
/// receive a smoothed alpha value between 0 and 255.
pub fn compute_feathered_mask(mask: &GrayImage, feather_radius: u32) -> GrayImage {
    if feather_radius == 0 {
        return mask.clone();
    }
    let (width, height) = mask.dimensions();
    let mut feathered = mask.clone();
    let radius_i = feather_radius as i32;

    for y in 0..height {
        for x in 0..width {
            if mask.get_pixel(x, y)[0] == 0 {
                // Background remains background
                continue;
            }

            // Search for nearest background pixel in the local neighborhood
            let mut min_dist_sq = f64::INFINITY;
            let x_start = (x as i32 - radius_i).max(0);
            let x_end = (x as i32 + radius_i).min(width as i32 - 1);
            let y_start = (y as i32 - radius_i).max(0);
            let y_end = (y as i32 + radius_i).min(height as i32 - 1);

            for ny in y_start..=y_end {
                for nx in x_start..=x_end {
                    if mask.get_pixel(nx as u32, ny as u32)[0] == 0 {
                        let dx = nx - x as i32;
                        let dy = ny - y as i32;
                        let dist_sq = (dx * dx + dy * dy) as f64;
                        if dist_sq < min_dist_sq {
                            min_dist_sq = dist_sq;
                        }
                    }
                }
            }

            if min_dist_sq < f64::INFINITY {
                let dist = min_dist_sq.sqrt();
                if dist <= feather_radius as f64 {
                    // Smooth transition from background (0) to foreground
                    // Dist is in range (0, feather_radius].
                    // Let's divide by (feather_radius + 0.5) to keep a smooth gradient.
                    let factor = dist / (feather_radius as f64 + 0.5);
                    let alpha = (255.0 * factor.min(1.0)) as u8;
                    feathered.put_pixel(x, y, Luma([alpha]));
                }
            }
        }
    }
    feathered
}

/// Applies the grayscale alpha mask to the image.
pub fn apply_mask(img: &RgbaImage, mask: &GrayImage) -> RgbaImage {
    let (width, height) = img.dimensions();
    let mut out = RgbaImage::new(width, height);
    for y in 0..height {
        for x in 0..width {
            let pixel = img.get_pixel(x, y);
            let mask_val = mask.get_pixel(x, y)[0];
            let new_alpha = ((pixel[3] as u16 * mask_val as u16) / 255) as u8;
            out.put_pixel(x, y, image::Rgba([pixel[0], pixel[1], pixel[2], new_alpha]));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn test_flood_fill_and_feather() {
        // Create 10x10 white image
        let mut img = RgbaImage::from_pixel(10, 10, Rgba([255, 255, 255, 255]));
        // Draw 4x4 red square in the center (from 3,3 to 6,6 inclusive)
        for y in 3..=6 {
            for x in 3..=6 {
                img.put_pixel(x, y, Rgba([255, 0, 0, 255]));
            }
        }

        // Run flood fill mask generator
        let mask = generate_flood_fill_mask(&img, 10);

        // Corners should be background (0)
        assert_eq!(mask.get_pixel(0, 0)[0], 0);
        assert_eq!(mask.get_pixel(9, 9)[0], 0);

        // Center should be foreground (255)
        assert_eq!(mask.get_pixel(3, 3)[0], 255);
        assert_eq!(mask.get_pixel(6, 6)[0], 255);

        // Feather the mask with radius 1
        let feathered = compute_feathered_mask(&mask, 1);

        // The center core (4,4) should still be fully foreground (255) because it's far from background
        assert_eq!(feathered.get_pixel(4, 4)[0], 255);

        // The edge of the red square (3,3) is next to background, so it should be feathered (between 0 and 255)
        let edge_val = feathered.get_pixel(3, 3)[0];
        assert!(edge_val > 0 && edge_val < 255, "Edge value {} should be feathered", edge_val);
    }
}

