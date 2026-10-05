use image::{Rgba, RgbaImage};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "icon-normalizer-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        let mut img = RgbaImage::new(100, 60);
        for y in 20..46 {
            for x in 5..31 {
                img.put_pixel(x, y, Rgba([0, 200, 0, 255]));
            }
            for x in 65..91 {
                img.put_pixel(x, y, Rgba([0, 0, 200, 255]));
            }
        }
        img.save(path.join("sheet.png")).unwrap();
        Self(path)
    }
    fn command(&self) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_icon-normalizer"));
        cmd.current_dir(&self.0);
        cmd
    }
    fn split(&self, args: &[&str]) -> Output {
        self.command()
            .args(["split", "sheet.png", "-o", "icons"])
            .args(args)
            .output()
            .unwrap()
    }
    fn sheet(&self) -> PathBuf {
        self.0.join("sheet.png")
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn assert_success(out: &Output) {
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn split_exports_centered_transparent_webps_and_refuses_overwrite() {
    let fixture = Fixture::new();
    let input = std::fs::read(fixture.sheet()).unwrap();
    let out = fixture.split(&[
        "--grid",
        "2x1",
        "--count",
        "2",
        "--preview",
        "preview.png",
        "--report",
        "components.tsv",
    ]);
    assert_success(&out);
    for number in 1..=2 {
        let file = fixture.0.join(format!("icons/sticker-{number:02}.webp"));
        let img = image::open(&file).unwrap().to_rgba8();
        assert_eq!(img.dimensions(), (512, 512));
        assert_eq!(img.get_pixel(0, 0)[3], 0);
        assert!(img.get_pixel(256, 256)[3] > 0);
        let bbox = content_box(&img);
        assert_eq!(bbox, (16, 16, 495, 495));
    }
    assert!(fixture.0.join("preview.png").is_file());
    assert!(
        std::fs::read_to_string(fixture.0.join("components.tsv"))
            .unwrap()
            .contains("anchor")
    );
    let before = std::fs::read(fixture.0.join("icons/sticker-01.webp")).unwrap();
    let out = fixture.split(&[]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("already exists"));
    assert_eq!(
        before,
        std::fs::read(fixture.0.join("icons/sticker-01.webp")).unwrap()
    );
    assert_eq!(input, std::fs::read(fixture.sheet()).unwrap());
}

fn content_box(img: &RgbaImage) -> (u32, u32, u32, u32) {
    let mut bounds = (u32::MAX, u32::MAX, 0, 0);
    for (x, y, p) in img.enumerate_pixels() {
        if p[3] > 0 {
            bounds.0 = bounds.0.min(x);
            bounds.1 = bounds.1.min(y);
            bounds.2 = bounds.2.max(x);
            bounds.3 = bounds.3.max(y);
        }
    }
    bounds
}

#[test]
fn ambiguous_island_writes_requested_diagnostics_but_no_stickers() {
    let fixture = Fixture::new();
    let mut img = image::open(fixture.sheet()).unwrap().to_rgba8();
    for y in 26..34 {
        for x in 46..50 {
            img.put_pixel(x, y, Rgba([255, 0, 0, 255]));
        }
    }
    img.save(fixture.sheet()).unwrap();
    let out = fixture.split(&["--preview", "preview.png", "--report", "components.tsv"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("No individual stickers exported"));
    assert!(!fixture.0.join("icons").exists());
    let report = std::fs::read_to_string(fixture.0.join("components.tsv")).unwrap();
    let island = report
        .lines()
        .find(|line| line.ends_with("ambiguous"))
        .unwrap()
        .split('\t')
        .next()
        .unwrap();
    let override_value = format!("{island}=1");
    assert_success(&fixture.split(&["--assign", &override_value]));
}

#[test]
fn dry_run_and_invalid_counts_do_not_export() {
    let fixture = Fixture::new();
    assert_success(&fixture.split(&["--dry-run", "--count", "2"]));
    assert!(!fixture.0.join("icons").exists());
    let out = fixture.split(&["--count", "3"]);
    assert!(!out.status.success());
    assert!(!fixture.0.join("icons").exists());
    let out = fixture.split(&["--grid", "0x2"]);
    assert!(!out.status.success());
    assert!(!fixture.0.join("icons").exists());
}

#[test]
fn cannot_overwrite_source_or_reuse_diagnostic_path() {
    let fixture = Fixture::new();
    let before = std::fs::read(fixture.sheet()).unwrap();
    let out = fixture.split(&["--dry-run", "--preview", "sheet.png", "--overwrite"]);
    assert!(!out.status.success());
    assert_eq!(before, std::fs::read(fixture.sheet()).unwrap());
    let out = fixture.split(&["--preview", "diagnostic.png", "--report", "diagnostic.png"]);
    assert!(!out.status.success());
    assert!(!fixture.0.join("diagnostic.png").exists());
}

#[cfg(unix)]
#[test]
fn input_hard_links_are_protected_even_with_overwrite() {
    let fixture = Fixture::new();
    let before = std::fs::read(fixture.sheet()).unwrap();
    std::fs::hard_link(fixture.sheet(), fixture.0.join("alias.png")).unwrap();
    let out = fixture.split(&["--dry-run", "--preview", "alias.png", "--overwrite"]);
    assert!(!out.status.success());
    assert_eq!(before, std::fs::read(fixture.sheet()).unwrap());
}

#[test]
fn existing_single_icon_and_completions_still_work() {
    let fixture = Fixture::new();
    let out = fixture
        .command()
        .args(["sheet.png", "--no-flood", "-o", "icon.webp", "--size", "64"])
        .output()
        .unwrap();
    assert_success(&out);
    assert_eq!(
        image::open(fixture.0.join("icon.webp")).unwrap().width(),
        64
    );
    let out = fixture
        .command()
        .args(["completions", "bash"])
        .output()
        .unwrap();
    assert_success(&out);
    assert!(String::from_utf8_lossy(&out.stdout).contains("split"));
}

#[test]
fn explicit_background_removal_supports_opaque_sheets() {
    let fixture = Fixture::new();
    let mut img = image::open(fixture.sheet()).unwrap().to_rgba8();
    for p in img.pixels_mut() {
        if p[3] == 0 {
            *p = Rgba([255; 4]);
        }
    }
    img.save(fixture.sheet()).unwrap();
    assert!(!fixture.split(&[]).status.success());
    assert_success(&fixture.split(&["--remove-background", "--feather", "0", "--count", "2"]));
}

#[test]
fn preview_can_be_nested_but_input_aliases_remain_protected() {
    let fixture = Fixture::new();
    assert_success(&fixture.split(&["--dry-run", "--preview", "nested/preview.png"]));
    assert!(Path::new(&fixture.0.join("nested/preview.png")).exists());
    let out = fixture.split(&[
        "--dry-run",
        "--preview",
        "nested/../sheet.png",
        "--overwrite",
    ]);
    assert!(!out.status.success());
}
