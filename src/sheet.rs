//! Alpha-based sticker segmentation. Geometry can suggest ownership of detached
//! accents, but cannot establish their meaning; ambiguous assignments are exposed.

use image::{Rgba, RgbaImage};
use std::collections::{HashMap, VecDeque};

const INF: f64 = 1e20;
type Bounds = (u32, u32, u32, u32);

#[derive(Debug, Clone)]
pub struct SplitOptions {
    pub alpha_threshold: u8,
    pub min_area: usize,
    pub anchor_ratio: f64,
    pub expected_count: Option<usize>,
    pub grid: Option<(u32, u32)>,
    pub island_gap: f64,
    pub ambiguity_margin: f64,
    pub edge_radius: u32,
    pub erosion: u32,
    /// One-based component ID -> one-based sticker number; zero discards a component.
    pub assignments: Vec<(usize, usize)>,
}

impl Default for SplitOptions {
    fn default() -> Self {
        Self {
            alpha_threshold: 32,
            min_area: 32,
            anchor_ratio: 0.15,
            expected_count: None,
            grid: None,
            island_gap: 80.0,
            ambiguity_margin: 4.0,
            edge_radius: 2,
            erosion: 0,
            assignments: Vec::new(),
        }
    }
}

#[derive(Debug)]
struct Component {
    pixels: Vec<usize>,
    bbox: Bounds,
    center: (f64, f64),
}

impl Component {
    fn new(pixels: Vec<usize>, width: usize) -> Self {
        let mut bbox = (u32::MAX, u32::MAX, 0, 0);
        let (mut sx, mut sy) = (0.0, 0.0);
        for &p in &pixels {
            let (x, y) = ((p % width) as u32, (p / width) as u32);
            bbox.0 = bbox.0.min(x);
            bbox.1 = bbox.1.min(y);
            bbox.2 = bbox.2.max(x);
            bbox.3 = bbox.3.max(y);
            sx += x as f64;
            sy += y as f64;
        }
        let len = pixels.len() as f64;
        Self {
            pixels,
            bbox,
            center: (sx / len, sy / len),
        }
    }
}

#[derive(Debug)]
pub struct Region {
    pub crop: RgbaImage,
    pub bbox: Bounds,
}

#[derive(Debug)]
pub struct SplitResult {
    pub regions: Vec<Region>,
    pub issues: Vec<String>,
    pub warnings: Vec<String>,
    components: Vec<Component>,
    /// Zero-based group number, or None for ignored/unresolved components.
    owners: Vec<Option<usize>>,
    status: Vec<String>,
    anchors: Vec<usize>,
}

/// Four-connected components, matching SciPy's default `ndimage.label`.
fn label(mask: &[bool], width: usize, height: usize) -> (Vec<usize>, Vec<Component>) {
    let mut labels = vec![0; mask.len()];
    let mut components = Vec::new();
    let mut queue = VecDeque::new();
    for start in 0..mask.len() {
        if !mask[start] || labels[start] != 0 {
            continue;
        }
        let id = components.len() + 1;
        labels[start] = id;
        queue.push_back(start);
        let mut pixels = Vec::new();
        while let Some(p) = queue.pop_front() {
            pixels.push(p);
            let (x, y) = (p % width, p / width);
            for q in [
                (x > 0).then(|| p - 1),
                (x + 1 < width).then_some(p + 1),
                (y > 0).then(|| p - width),
                (y + 1 < height).then_some(p + width),
            ]
            .into_iter()
            .flatten()
            {
                if mask[q] && labels[q] == 0 {
                    labels[q] = id;
                    queue.push_back(q);
                }
            }
        }
        components.push(Component::new(pixels, width));
    }
    (labels, components)
}

/// Exact squared Euclidean distance transform, using the lower envelope of
/// one-dimensional parabolas. Two linear passes avoid pairwise boundary scans.
fn edt_line(input: &[f64], output: &mut [f64], vertices: &mut [usize], cuts: &mut [f64]) {
    let mut count = 0;
    for (q, &value) in input.iter().enumerate() {
        if value >= INF {
            continue;
        }
        let mut cut = f64::NEG_INFINITY;
        while count > 0 {
            let p = vertices[count - 1];
            cut = ((value + (q as f64).powi(2)) - (input[p] + (p as f64).powi(2)))
                / (2.0 * (q - p) as f64);
            if cut > cuts[count - 1] {
                break;
            }
            count -= 1;
        }
        if count == 0 {
            cut = f64::NEG_INFINITY;
        }
        vertices[count] = q;
        cuts[count] = cut;
        count += 1;
    }
    if count == 0 {
        output.fill(INF);
        return;
    }
    let mut k = 0;
    for (q, value) in output.iter_mut().enumerate() {
        while k + 1 < count && cuts[k + 1] < q as f64 {
            k += 1;
        }
        let p = vertices[k];
        *value = (q as f64 - p as f64).powi(2) + input[p];
    }
}

fn distances(pixels: &[usize], width: usize, height: usize) -> Vec<f64> {
    let mut field = vec![INF; width * height];
    for &p in pixels {
        field[p] = 0.0;
    }
    let n = width.max(height);
    let mut input = vec![0.0; n];
    let mut output = vec![0.0; n];
    let mut vertices = vec![0; n];
    let mut cuts = vec![0.0; n];
    for x in 0..width {
        for y in 0..height {
            input[y] = field[y * width + x];
        }
        edt_line(
            &input[..height],
            &mut output[..height],
            &mut vertices,
            &mut cuts,
        );
        for y in 0..height {
            field[y * width + x] = output[y];
        }
    }
    for row in field.chunks_exact_mut(width) {
        input[..width].copy_from_slice(row);
        edt_line(
            &input[..width],
            &mut output[..width],
            &mut vertices,
            &mut cuts,
        );
        row.copy_from_slice(&output[..width]);
    }
    field
}

/// Manhattan erosion is only used to find seeds. Original RGBA pixels are
/// restored afterward; this does not shrink the exported sticker borders.
fn erode(mask: &[bool], width: usize, height: usize, radius: u32) -> Vec<bool> {
    let mut distance = vec![u32::MAX; mask.len()];
    let mut queue = VecDeque::new();
    for p in 0..mask.len() {
        let (x, y) = (p % width, p / width);
        if !mask[p] {
            distance[p] = 0;
            queue.push_back(p);
        } else if x == 0 || y == 0 || x + 1 == width || y + 1 == height {
            distance[p] = 1;
            queue.push_back(p);
        }
    }
    while let Some(p) = queue.pop_front() {
        if distance[p] >= radius {
            continue;
        }
        let (x, y) = (p % width, p / width);
        for q in [
            (x > 0).then(|| p - 1),
            (x + 1 < width).then_some(p + 1),
            (y > 0).then(|| p - width),
            (y + 1 < height).then_some(p + width),
        ]
        .into_iter()
        .flatten()
        {
            if distance[q] > distance[p] + 1 {
                distance[q] = distance[p] + 1;
                queue.push_back(q);
            }
        }
    }
    mask.iter()
        .zip(distance)
        .map(|(&on, d)| on && d > radius)
        .collect()
}

fn components(img: &RgbaImage, options: &SplitOptions) -> (Vec<usize>, Vec<Component>) {
    let (width, height) = (img.width() as usize, img.height() as usize);
    let mask: Vec<_> = img
        .pixels()
        .map(|p| p[3] > options.alpha_threshold)
        .collect();
    let (original_labels, original) = label(&mask, width, height);
    if options.erosion == 0 {
        return (original_labels, original);
    }
    let (_, seeds) = label(&erode(&mask, width, height, options.erosion), width, height);
    let mut by_original: Vec<Vec<Component>> = (0..original.len()).map(|_| Vec::new()).collect();
    for seed in seeds {
        if seed.pixels.len() >= options.min_area {
            by_original[original_labels[seed.pixels[0]] - 1].push(seed);
        }
    }
    let mut rebuilt = Vec::new();
    for (component, seeds) in original.into_iter().zip(by_original) {
        // Keep small detached accents intact even if erosion erased them entirely.
        if seeds.len() < 2 {
            rebuilt.push(component);
            continue;
        }
        let mut best = vec![INF; component.pixels.len()];
        let mut owners = vec![0; component.pixels.len()];
        for (id, seed) in seeds.iter().enumerate() {
            let field = distances(&seed.pixels, width, height);
            for (i, &p) in component.pixels.iter().enumerate() {
                if field[p] < best[i] {
                    best[i] = field[p];
                    owners[i] = id;
                }
            }
        }
        let mut restored = vec![Vec::new(); seeds.len()];
        for (p, owner) in component.pixels.into_iter().zip(owners) {
            restored[owner].push(p);
        }
        for pixels in restored {
            rebuilt.push(Component::new(pixels, width));
        }
    }
    // Stable IDs in raster order, including when seeds have split a bridge.
    rebuilt.sort_by_key(|c| *c.pixels.iter().min().unwrap());
    let mut labels = vec![0; mask.len()];
    for (id, component) in rebuilt.iter().enumerate() {
        for &p in &component.pixels {
            labels[p] = id + 1;
        }
    }
    (labels, rebuilt)
}

fn cell(component: &Component, grid: (u32, u32), width: u32, height: u32) -> usize {
    let col = ((component.center.0 * grid.0 as f64 / width as f64) as u32).min(grid.0 - 1);
    let row = ((component.center.1 * grid.1 as f64 / height as f64) as u32).min(grid.1 - 1);
    (row * grid.0 + col) as usize
}

fn reading_order(anchors: &mut [usize], components: &[Component]) {
    anchors.sort_by(|&a, &b| components[a].center.1.total_cmp(&components[b].center.1));
    let mut heights: Vec<_> = anchors
        .iter()
        .map(|&i| components[i].bbox.3 - components[i].bbox.1 + 1)
        .collect();
    heights.sort_unstable();
    let tolerance = heights[heights.len() / 2] as f64 * 0.5;
    let mut start = 0;
    while start < anchors.len() {
        let row_y = components[anchors[start]].center.1;
        let mut end = start + 1;
        while end < anchors.len() && components[anchors[end]].center.1 - row_y <= tolerance {
            end += 1;
        }
        anchors[start..end]
            .sort_by(|&a, &b| components[a].center.0.total_cmp(&components[b].center.0));
        start = end;
    }
}

pub fn split(img: &RgbaImage, options: &SplitOptions) -> Result<SplitResult, String> {
    if img.width() == 0 || img.height() == 0 {
        return Err("The sheet is empty.".into());
    }
    if options.alpha_threshold == 255
        || options.min_area == 0
        || !options.anchor_ratio.is_finite()
        || !(0.0..=1.0).contains(&options.anchor_ratio)
        || options.anchor_ratio == 0.0
        || !options.island_gap.is_finite()
        || options.island_gap < 0.0
        || !options.ambiguity_margin.is_finite()
        || options.ambiguity_margin < 0.0
        || options.edge_radius > 16
        || options.erosion > 64
        || options.expected_count == Some(0)
    {
        return Err("Invalid segmentation options; see split --help.".into());
    }
    if let Some((cols, rows)) = options.grid
        && (cols == 0
            || rows == 0
            || cols > img.width()
            || rows > img.height()
            || cols.checked_mul(rows).is_none())
    {
        return Err("The grid must be nonzero and fit within the sheet.".into());
    }
    if !img.pixels().any(|p| p[3] <= options.alpha_threshold) {
        return Err(
            "No transparent background found. Use --remove-background for an opaque sheet.".into(),
        );
    }
    let (labels, components) = components(img, options);
    let largest = components.iter().map(|c| c.pixels.len()).max().unwrap_or(0);
    if largest < options.min_area {
        return Err("No foreground component meets --min-area.".into());
    }
    let mut anchors: Vec<_> = components
        .iter()
        .enumerate()
        .filter(|(_, c)| {
            c.pixels.len() >= options.min_area
                && c.pixels.len() as f64 >= largest as f64 * options.anchor_ratio
        })
        .map(|(i, _)| i)
        .collect();
    let expected = options
        .expected_count
        .or(options.grid.map(|(c, r)| (c * r) as usize));
    if expected.is_some_and(|count| count != anchors.len()) {
        return Err(format!(
            "Expected {} stickers, but found {} large components. Adjust --anchor-ratio / --alpha-threshold; try --erosion for thin touching borders. Refusing to merge or invent stickers.",
            expected.unwrap(),
            anchors.len()
        ));
    }
    if let Some(grid) = options.grid {
        if anchors.len() != (grid.0 * grid.1) as usize {
            return Err("--grid requires exactly one large sticker per cell.".into());
        }
        anchors.sort_by_key(|&i| cell(&components[i], grid, img.width(), img.height()));
        for (index, &id) in anchors.iter().enumerate() {
            if cell(&components[id], grid, img.width(), img.height()) != index {
                return Err("--grid requires one large component center in every cell; it never clips shapes at cell boundaries.".into());
            }
        }
    } else {
        reading_order(&mut anchors, &components);
    }

    let mut overrides = HashMap::new();
    for &(id, target) in &options.assignments {
        if id == 0 || id > components.len() || target > anchors.len() {
            return Err(format!(
                "Invalid --assign {id}={target}: component IDs are 1..{}, sticker numbers 1..{} (0 discards).",
                components.len(),
                anchors.len()
            ));
        }
        if overrides.insert(id - 1, target.checked_sub(1)).is_some() {
            return Err(format!(
                "Component {id} has more than one --assign override."
            ));
        }
    }
    let mut owners = vec![None; components.len()];
    let mut status = vec![String::new(); components.len()];
    for (group, &id) in anchors.iter().enumerate() {
        if overrides
            .get(&id)
            .is_some_and(|target| *target != Some(group))
        {
            return Err(format!(
                "Component {} is a main sticker; --assign can only move or discard islands.",
                id + 1
            ));
        }
        owners[id] = Some(group);
        status[id] = "anchor".into();
    }
    // One distance field per main sticker; retain only component-to-anchor gaps.
    // Large bodies remain separate even if an island is assigned nearby.
    let mut gaps = vec![vec![INF; anchors.len()]; components.len()];
    for (group, &id) in anchors.iter().enumerate() {
        let field = distances(
            &components[id].pixels,
            img.width() as usize,
            img.height() as usize,
        );
        for (i, component) in components.iter().enumerate() {
            if component.pixels.len() >= options.min_area || overrides.contains_key(&i) {
                gaps[i][group] = component
                    .pixels
                    .iter()
                    .map(|&p| field[p])
                    .fold(INF, f64::min)
                    .sqrt();
            }
        }
    }
    let mut issues = Vec::new();
    let mut warnings = Vec::new();
    for (id, component) in components.iter().enumerate() {
        if owners[id].is_some() {
            continue;
        }
        if let Some(&owner) = overrides.get(&id) {
            owners[id] = owner;
            status[id] = if owner.is_some() {
                "manual"
            } else {
                "discarded-manual"
            }
            .into();
            continue;
        }
        if component.pixels.len() < options.min_area {
            status[id] = "discarded-noise".into();
            continue;
        }
        let mut candidates: Vec<_> = (0..anchors.len()).collect();
        if let Some(grid) = options.grid {
            // A layout hint governs only whole-component ownership. No rectangular
            // crop is used, so overlapping bounding boxes do not copy neighbors.
            candidates.retain(|&group| group == cell(component, grid, img.width(), img.height()));
        }
        candidates.sort_by(|&a, &b| gaps[id][a].total_cmp(&gaps[id][b]));
        let best = candidates[0];
        let gap = gaps[id][best];
        if gap > options.island_gap {
            status[id] = "discarded-distant".into();
            warnings.push(format!("Component {} ({} pixels) is {:.1}px from its candidate sticker; discarded by --island-gap.", id + 1, component.pixels.len(), gap));
        } else if candidates
            .get(1)
            .is_some_and(|&next| gaps[id][next] - gap <= options.ambiguity_margin)
        {
            let next = candidates[1];
            status[id] = "ambiguous".into();
            issues.push(format!("Component {} at {:?} could belong to sticker {} ({:.1}px) or {} ({:.1}px). Use --assign {}={} / {}={} or an accurate --grid hint.", id + 1, component.bbox, best + 1, gap, next + 1, gaps[id][next], id + 1, best + 1, id + 1, next + 1));
        } else {
            owners[id] = Some(best);
            status[id] = if options.grid.is_some() {
                "island-grid"
            } else {
                "island-nearest"
            }
            .into();
        }
    }

    let (width, height) = (img.width(), img.height());
    let mut groups = vec![0; labels.len()];
    for (p, &id) in labels.iter().enumerate() {
        if id > 0 {
            groups[p] = owners[id - 1].map_or(0, |group| group + 1);
        }
    }
    // Recover faint antialiasing only near retained pixels. Opaque noise and
    // discarded islands are never reintroduced by this halo pass.
    let core = groups.clone();
    let radius = options.edge_radius as i32;
    for (p, pixel) in img.pixels().enumerate() {
        if pixel[3] == 0 || pixel[3] > options.alpha_threshold {
            continue;
        }
        let (x, y) = ((p % width as usize) as i32, (p / width as usize) as i32);
        let mut best = radius * radius + 1;
        for ny in (y - radius).max(0)..=(y + radius).min(height as i32 - 1) {
            for nx in (x - radius).max(0)..=(x + radius).min(width as i32 - 1) {
                let q = (ny as u32 * width + nx as u32) as usize;
                let d = (nx - x).pow(2) + (ny - y).pow(2);
                if core[q] > 0 && d < best {
                    best = d;
                    groups[p] = core[q];
                }
            }
        }
    }
    let mut boxes = vec![None; anchors.len()];
    for (p, &group) in groups.iter().enumerate() {
        if group == 0 {
            continue;
        }
        let (x, y) = ((p % width as usize) as u32, (p / width as usize) as u32);
        let bounds = boxes[group - 1].get_or_insert((x, y, x, y));
        bounds.0 = bounds.0.min(x);
        bounds.1 = bounds.1.min(y);
        bounds.2 = bounds.2.max(x);
        bounds.3 = bounds.3.max(y);
    }
    let regions = boxes
        .into_iter()
        .enumerate()
        .map(|(group, bbox)| {
            let bbox = bbox.expect("Every anchor retains pixels");
            let mut crop = RgbaImage::new(bbox.2 - bbox.0 + 1, bbox.3 - bbox.1 + 1);
            for (x, y, pixel) in crop.enumerate_pixels_mut() {
                let p = ((y + bbox.1) * width + x + bbox.0) as usize;
                if groups[p] == group + 1 {
                    *pixel = *img.get_pixel(x + bbox.0, y + bbox.1);
                }
            }
            Region { crop, bbox }
        })
        .collect();
    Ok(SplitResult {
        regions,
        issues,
        warnings,
        components,
        owners,
        status,
        anchors,
    })
}

impl SplitResult {
    pub fn report(&self) -> String {
        let mut report = String::from("component\tpixels\tx0\ty0\tx1\ty1\tsticker\tstatus\n");
        for (id, c) in self.components.iter().enumerate() {
            let owner = self.owners[id].map_or(String::new(), |g| (g + 1).to_string());
            report.push_str(&format!(
                "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
                id + 1,
                c.pixels.len(),
                c.bbox.0,
                c.bbox.1,
                c.bbox.2,
                c.bbox.3,
                owner,
                self.status[id]
            ));
        }
        report
    }

    /// Diagnostic preview on a checkerboard. Boxes and numbers identify component
    /// IDs for --assign. A shared box color means a shared exported sticker.
    pub fn preview(&self, img: &RgbaImage) -> RgbaImage {
        let mut preview = RgbaImage::from_fn(img.width(), img.height(), |x, y| {
            let bg = if (x / 16 + y / 16) % 2 == 0 { 220 } else { 245 };
            let p = img.get_pixel(x, y);
            let a = p[3] as u32;
            Rgba([
                ((p[0] as u32 * a + bg * (255 - a)) / 255) as u8,
                ((p[1] as u32 * a + bg * (255 - a)) / 255) as u8,
                ((p[2] as u32 * a + bg * (255 - a)) / 255) as u8,
                255,
            ])
        });
        let palette = [
            [0, 100, 220],
            [0, 160, 80],
            [160, 80, 220],
            [220, 130, 0],
            [0, 155, 165],
            [200, 65, 140],
            [130, 140, 0],
            [80, 75, 220],
            [210, 70, 30],
            [0, 85, 85],
            [130, 35, 90],
            [145, 100, 50],
            [40, 115, 130],
            [70, 70, 110],
            [40, 140, 35],
            [185, 85, 195],
        ];
        for (id, component) in self.components.iter().enumerate() {
            if self.status[id] == "discarded-noise" {
                continue;
            }
            let color = self.owners[id].map_or([230, 30, 30], |g| palette[g % palette.len()]);
            let color = Rgba([color[0], color[1], color[2], 255]);
            let (x0, y0, x1, y1) = component.bbox;
            for x in x0..=x1 {
                preview.put_pixel(x, y0, color);
                preview.put_pixel(x, y1, color);
            }
            for y in y0..=y1 {
                preview.put_pixel(x0, y, color);
                preview.put_pixel(x1, y, color);
            }
            draw_number(&mut preview, id + 1, x0, y0.saturating_sub(16), color);
        }
        for (group, &id) in self.anchors.iter().enumerate() {
            let c = &self.components[id];
            draw_number(
                &mut preview,
                group + 1,
                c.center.0 as u32,
                c.center.1 as u32,
                Rgba([20, 20, 20, 255]),
            );
        }
        preview
    }
}

fn draw_number(img: &mut RgbaImage, number: usize, x: u32, y: u32, color: Rgba<u8>) {
    const DIGITS: [[u8; 5]; 10] = [
        [7, 5, 5, 5, 7],
        [2, 6, 2, 2, 7],
        [7, 1, 7, 4, 7],
        [7, 1, 7, 1, 7],
        [5, 5, 7, 1, 1],
        [7, 4, 7, 1, 7],
        [7, 4, 7, 5, 7],
        [7, 1, 1, 1, 1],
        [7, 5, 7, 5, 7],
        [7, 5, 7, 1, 7],
    ];
    for (offset, digit) in number.to_string().bytes().enumerate() {
        let x = x + offset as u32 * 8;
        for dy in 0..12 {
            for dx in 0..8 {
                if x + dx < img.width() && y + dy < img.height() {
                    let on = (1..11).contains(&dy)
                        && dx < 6
                        && DIGITS[(digit - b'0') as usize][((dy - 1) / 2) as usize]
                            & (4 >> (dx / 2))
                            != 0;
                    img.put_pixel(
                        x + dx,
                        y + dy,
                        if on {
                            Rgba([255, 255, 255, 255])
                        } else {
                            color
                        },
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(img: &mut RgbaImage, bounds: Bounds, color: Rgba<u8>) {
        for y in bounds.1..=bounds.3 {
            for x in bounds.0..=bounds.2 {
                img.put_pixel(x, y, color);
            }
        }
    }
    fn fixture() -> RgbaImage {
        let mut img = RgbaImage::new(100, 60);
        rect(&mut img, (5, 20, 30, 45), Rgba([0, 200, 0, 255]));
        rect(&mut img, (65, 20, 90, 45), Rgba([0, 0, 200, 255]));
        img
    }
    fn options() -> SplitOptions {
        SplitOptions {
            min_area: 4,
            ..Default::default()
        }
    }

    #[test]
    fn exact_distances_match_brute_force_on_rectangular_image() {
        let points = [2, 19, 33];
        let field = distances(&points, 7, 6);
        for (p, &value) in field.iter().enumerate() {
            let expected = points
                .iter()
                .map(|&q| {
                    let dx = (p % 7) as f64 - (q % 7) as f64;
                    let dy = (p / 7) as f64 - (q / 7) as f64;
                    dx * dx + dy * dy
                })
                .fold(INF, f64::min);
            assert_eq!(value, expected);
        }
    }

    #[test]
    fn detached_heart_is_one_island_and_does_not_merge_bodies() {
        let mut img = fixture();
        rect(&mut img, (25, 8, 29, 12), Rgba([255, 0, 0, 255]));
        let result = split(&img, &options()).unwrap();
        assert_eq!(result.regions.len(), 2);
        assert!(result.issues.is_empty());
        assert_eq!(result.regions[0].bbox, (5, 8, 30, 45));
        assert_eq!(
            result.regions[0]
                .crop
                .pixels()
                .filter(|p| p[0] == 255 && p[3] > 0)
                .count(),
            25
        );
        assert!(result.regions[1].crop.pixels().all(|p| p[0] == 0));
    }

    #[test]
    fn ambiguous_island_requires_override_and_stays_whole() {
        let mut img = fixture();
        rect(&mut img, (46, 28, 49, 31), Rgba([255, 0, 0, 255]));
        let mut opts = options();
        let result = split(&img, &opts).unwrap();
        assert_eq!(result.issues.len(), 1);
        let island = result.status.iter().position(|s| s == "ambiguous").unwrap();
        opts.assignments.push((island + 1, 1));
        let result = split(&img, &opts).unwrap();
        assert!(result.issues.is_empty());
        assert_eq!(
            result.regions[0]
                .crop
                .pixels()
                .filter(|p| p[0] == 255 && p[3] > 0)
                .count(),
            16
        );
        assert!(result.regions[1].crop.pixels().all(|p| p[0] == 0));
    }

    #[test]
    fn grid_resolves_ownership_without_rectangular_clipping() {
        let mut img = fixture();
        rect(&mut img, (46, 28, 49, 31), Rgba([255, 0, 0, 255]));
        let opts = SplitOptions {
            grid: Some((2, 1)),
            ..options()
        };
        let result = split(&img, &opts).unwrap();
        assert!(result.issues.is_empty());
        assert_eq!(result.regions[0].bbox.2, 49);
    }

    #[test]
    fn overlapping_boxes_do_not_leak_pixels() {
        let mut img = RgbaImage::new(80, 80);
        rect(&mut img, (10, 10, 60, 15), Rgba([255, 0, 0, 255]));
        rect(&mut img, (10, 10, 15, 60), Rgba([255, 0, 0, 255]));
        rect(&mut img, (30, 30, 50, 50), Rgba([0, 0, 255, 255]));
        let result = split(&img, &options()).unwrap();
        assert_eq!(result.regions.len(), 2);
        assert!(result.regions[0].crop.pixels().all(|p| p[2] == 0));
        assert!(result.regions[1].crop.pixels().all(|p| p[0] == 0));
    }

    #[test]
    fn noise_and_distant_islands_are_excluded_but_faint_edges_preserved() {
        let mut img = fixture();
        img.put_pixel(4, 25, Rgba([0, 200, 0, 20]));
        img.put_pixel(98, 58, Rgba([255, 0, 0, 255]));
        rect(&mut img, (46, 2, 49, 5), Rgba([255, 0, 0, 255]));
        let opts = SplitOptions {
            island_gap: 5.0,
            ..options()
        };
        let result = split(&img, &opts).unwrap();
        assert!(!result.warnings.is_empty());
        assert_eq!(result.regions[0].bbox.0, 4);
        assert_eq!(result.regions[0].crop.get_pixel(0, 5)[3], 20);
        assert!(
            result
                .regions
                .iter()
                .all(|r| r.crop.pixels().all(|p| p[0] == 0))
        );
    }

    #[test]
    fn erosion_separates_thin_bridge_and_restores_original_pixels() {
        let mut img = fixture();
        rect(&mut img, (30, 30, 65, 30), Rgba([0, 200, 0, 255]));
        rect(&mut img, (10, 5, 12, 7), Rgba([255, 0, 0, 255]));
        let mut opts = SplitOptions {
            expected_count: Some(2),
            ..options()
        };
        assert!(split(&img, &opts).is_err());
        opts.erosion = 2;
        let result = split(&img, &opts).unwrap();
        assert!(result.issues.is_empty());
        let before = img.pixels().filter(|p| p[3] > 0).count();
        let after: usize = result
            .regions
            .iter()
            .map(|r| r.crop.pixels().filter(|p| p[3] > 0).count())
            .sum();
        assert_eq!(before, after);
        assert_eq!(
            result.regions[0]
                .crop
                .pixels()
                .filter(|p| p[0] == 255 && p[3] > 0)
                .count(),
            9
        );
    }

    #[test]
    fn missing_sticker_or_wrong_grid_is_not_silently_accepted() {
        let opts = SplitOptions {
            expected_count: Some(3),
            ..options()
        };
        assert!(split(&fixture(), &opts).unwrap_err().contains("Expected 3"));
        let opts = SplitOptions {
            grid: Some((1, 2)),
            ..options()
        };
        assert!(split(&fixture(), &opts).is_err());
    }

    #[test]
    fn rejects_opaque_empty_and_invalid_settings() {
        assert!(split(&RgbaImage::from_pixel(10, 10, Rgba([255; 4])), &options()).is_err());
        assert!(split(&RgbaImage::new(10, 10), &options()).is_err());
        assert!(
            split(
                &fixture(),
                &SplitOptions {
                    edge_radius: 17,
                    ..options()
                }
            )
            .is_err()
        );
        assert!(
            split(
                &fixture(),
                &SplitOptions {
                    grid: Some((0, 2)),
                    ..options()
                }
            )
            .is_err()
        );
    }
}
