//! Cover art as pixel art: decode a thumbnail to a tiny source, then reduce it to an
//! n x n grid of the active shell's own colors with ordered dithering.

use std::time::Duration;

use image::{DynamicImage, RgbImage, imageops::FilterType};
use ratatui::{layout::Size, style::Color};
use ratatui_image::{
    Resize,
    picker::{Picker, ProtocolType, cap_parser::QueryStdioOptions},
    protocol::Protocol,
};

use crate::theme;

/// What the cache keeps per track: a 40x40 RGB square.
pub type Source = RgbImage;
/// Rows of n colors.
pub type Grid = Vec<Vec<Color>>;

const SOURCE_SIZE: u32 = 40;
/// Dither strength on a 0-255 scale; larger means more visible dot patterns.
pub const DITHER_SPREAD: f32 = 32.0;

const BAYER: [[u8; 4]; 4] = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]];

/// Center-crop to a square and scale it to `size` x `size`.
fn center_square(img: &DynamicImage, size: u32) -> RgbImage {
    let side = img.width().min(img.height());
    img.crop_imm(
        (img.width() - side) / 2,
        (img.height() - side) / 2,
        side,
        side,
    )
    .resize_exact(size, size, FilterType::Triangle)
    .to_rgb8()
}

/// Decode an image (JPEG) into the 40x40 source; `None` on any failure.
pub fn decode(bytes: &[u8]) -> Option<Source> {
    let img = image::load_from_memory(bytes).ok()?;
    Some(center_square(&img, SOURCE_SIZE))
}

/// How the cover is drawn this session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArtPath {
    /// A terminal image protocol (kitty, sixel, iTerm2).
    Crisp,
    HalfBlocks,
    /// No art at all, whatever the setting says.
    Off,
}

/// `env` is `TERMUSIC_ART`, `query` what the terminal reported (`None`: it failed).
pub fn choose(env: Option<&str>, query: Option<ProtocolType>) -> ArtPath {
    match (env, query) {
        (Some("off"), _) => ArtPath::Off,
        (Some("halfblocks"), _) => ArtPath::HalfBlocks,
        (_, Some(ProtocolType::Kitty | ProtocolType::Sixel | ProtocolType::Iterm2)) => {
            ArtPath::Crisp
        }
        _ => ArtPath::HalfBlocks,
    }
}

/// Whether to run the terminal query: forced by `TERMUSIC_ART=detect`, else only when the
/// environment hints at an image-capable terminal (`off` and `halfblocks` never query).
pub fn should_query(
    env_art: Option<&str>,
    term_program: Option<&str>,
    term: Option<&str>,
    kitty_window_id: Option<&str>,
) -> bool {
    match env_art {
        Some("off" | "halfblocks") => return false,
        Some("detect") => return true,
        _ => {}
    }
    let known = ["ghostty", "iterm.app", "wezterm", "kitty"];
    term_program.is_some_and(|t| known.iter().any(|k| t.eq_ignore_ascii_case(k)))
        || term.is_some_and(|t| t.contains("kitty") || t.contains("ghostty"))
        || kitty_window_id.is_some_and(|k| !k.is_empty())
}

/// Decide the path once at startup (raw mode on); the picker is kept only for `Crisp`.
/// The query runs only where `should_query` says so; a silent terminal costs at most 250 ms.
pub fn detect() -> (ArtPath, Option<Picker>) {
    let var = |k| std::env::var(k).ok();
    let env = var("TERMUSIC_ART");
    let (tp, term, kw) = (var("TERM_PROGRAM"), var("TERM"), var("KITTY_WINDOW_ID"));
    if !should_query(
        env.as_deref(),
        tp.as_deref(),
        term.as_deref(),
        kw.as_deref(),
    ) {
        return (choose(env.as_deref(), None), None);
    }
    // The crate's stdin-reading query thread outlives its timeout and would eat every keystroke
    // if the terminal never answers, hence the hint check above.
    let picker = Picker::from_query_stdio_with_options(QueryStdioOptions {
        timeout: Duration::from_millis(250),
        ..Default::default()
    })
    .ok();
    let path = choose(env.as_deref(), picker.as_ref().map(Picker::protocol_type));
    (path, picker.filter(|_| path == ArtPath::Crisp))
}

/// The grid as hard-edged pixels: every grid cell becomes a solid block.
pub fn upscale(grid: &Grid, w: u32, h: u32) -> RgbImage {
    let n = grid.len() as u32;
    let small = RgbImage::from_fn(n, n, |x, y| match grid[y as usize][x as usize] {
        Color::Rgb(r, g, b) => image::Rgb([r, g, b]),
        _ => image::Rgb([0, 0, 0]),
    });
    image::imageops::resize(&small, w, h, FilterType::Nearest)
}

/// The protocol image for the 10 x 5 cell label slot; `None` when the crate cannot build it.
pub fn protocol(picker: &Picker, src: &Source, shell: &theme::Shell) -> Option<Protocol> {
    let grid = pixelize(src, 20, &shell_palette(shell));
    let font = picker.font_size();
    let img = upscale(&grid, 10 * font.width as u32, 5 * font.height as u32);
    picker
        .new_protocol(img.into(), Size::new(10, 5), Resize::Fit(None))
        .ok()
}

/// A YouTube video id: exactly 11 characters of `[A-Za-z0-9_-]`.
pub fn valid_id(id: &str) -> bool {
    id.len() == 11
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// Download the track's thumbnail and decode it; `None` on any failure (art is optional, so
/// errors stay silent). The caller validates `id`.
pub async fn fetch(id: &str) -> Option<Source> {
    const MAX: u64 = 1 << 20;
    let resp = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .ok()?
        .get(format!("https://i.ytimg.com/vi/{id}/mqdefault.jpg"))
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?;
    if resp.content_length().is_some_and(|n| n > MAX) {
        return None;
    }
    let bytes = resp.bytes().await.ok()?;
    if bytes.len() as u64 > MAX {
        return None;
    }
    tokio::task::spawn_blocking(move || decode(&bytes))
        .await
        .ok()?
}

/// The shell's inks, duplicates removed, order kept.
pub fn shell_palette(s: &theme::Shell) -> Vec<Color> {
    let mut out: Vec<Color> = Vec::new();
    for c in [
        s.body,
        s.select_bg,
        s.dim,
        s.frame,
        s.text,
        s.accent,
        s.label,
        s.label_ink,
        s.meter_lo,
        s.meter_hi,
    ] {
        if !out.contains(&c) {
            out.push(c);
        }
    }
    out
}

/// Reduce `src` to an n x n grid of palette colors with the default dither.
pub fn pixelize(src: &Source, n: usize, palette: &[Color]) -> Grid {
    pixelize_with(src, n, palette, DITHER_SPREAD)
}

/// Like [`pixelize`] with an explicit dither spread (the contact sheet compares spreads).
pub fn pixelize_with(src: &Source, n: usize, palette: &[Color], spread: f32) -> Grid {
    let inks: Vec<(Color, [i32; 3])> = palette
        .iter()
        .filter_map(|&c| match c {
            Color::Rgb(r, g, b) => Some((c, [r as i32, g as i32, b as i32])),
            _ => None,
        })
        .collect();
    let mut small = image::imageops::resize(src, n as u32, n as u32, FilterType::Triangle);
    stretch(&mut small, &inks);
    (0..n)
        .map(|y| {
            (0..n)
                .map(|x| {
                    let t = (BAYER[y % 4][x % 4] as f32 + 0.5) / 16.0 - 0.5;
                    let off = t * spread;
                    let p = small.get_pixel(x as u32, y as u32).0;
                    let p = p.map(|v| (v as f32 + off).clamp(0.0, 255.0) as i32);
                    inks.iter()
                        .min_by_key(|(_, c)| (0..3).map(|i| (c[i] - p[i]).pow(2)).sum::<i32>())
                        .map_or(Color::Reset, |(c, _)| *c)
                })
                .collect()
        })
        .collect()
}

fn luma([r, g, b]: [i32; 3]) -> f32 {
    0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32
}

/// Map the image's brightness range onto the palette's, so a dark photo still uses every ink
/// instead of collapsing into the darkest one (same affine map on all channels keeps hue).
fn stretch(img: &mut RgbImage, inks: &[(Color, [i32; 3])]) {
    let range = |it: &mut dyn Iterator<Item = f32>| {
        it.fold((f32::MAX, f32::MIN), |(lo, hi), v| (lo.min(v), hi.max(v)))
    };
    let (lo, hi) = range(&mut img.pixels().map(|p| luma(p.0.map(i32::from))));
    let (plo, phi) = range(&mut inks.iter().map(|(_, c)| luma(*c)));
    if hi - lo < 1.0 || phi - plo < 1.0 {
        return; // flat image or palette: nothing to stretch
    }
    let k = (phi - plo) / (hi - lo);
    for p in img.pixels_mut() {
        p.0 =
            p.0.map(|v| (plo + (v as f32 - lo) * k).clamp(0.0, 255.0) as u8);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageFormat, Rgb};
    use std::io::Cursor;

    const BLACK: Color = Color::Rgb(0, 0, 0);
    const RED: Color = Color::Rgb(255, 0, 0);
    const GREEN: Color = Color::Rgb(0, 255, 0);
    const BLUE: Color = Color::Rgb(0, 0, 255);
    const WHITE: Color = Color::Rgb(255, 255, 255);

    fn jpeg(img: &RgbImage) -> Vec<u8> {
        let mut buf = Cursor::new(Vec::new());
        img.write_to(&mut buf, ImageFormat::Jpeg).unwrap();
        buf.into_inner()
    }

    #[test]
    fn solid_color_maps_to_nearest_palette_color() {
        let src = RgbImage::from_pixel(40, 40, Rgb([250, 10, 10]));
        let grid = pixelize(&src, 10, &[BLACK, RED, WHITE]);
        assert!(grid.iter().flatten().all(|&c| c == RED));
    }

    #[test]
    fn a_dark_image_still_uses_the_light_ink() {
        // Left half near black, right half dark grey: without the stretch both map to black.
        let img = RgbImage::from_fn(40, 40, |x, _| Rgb(if x < 20 { [5; 3] } else { [45; 3] }));
        let g = pixelize_with(&img, 10, &[BLACK, WHITE], 0.0);
        assert_eq!(g[5][0], BLACK);
        assert_eq!(g[5][9], WHITE);
    }

    #[test]
    fn gradient_uses_both_colors_and_only_those() {
        let src = RgbImage::from_fn(40, 40, |x, _| Rgb([(x * 255 / 39) as u8; 3]));
        let grid = pixelize(&src, 20, &[BLACK, WHITE]);
        let flat: Vec<Color> = grid.iter().flatten().copied().collect();
        assert!(flat.contains(&BLACK) && flat.contains(&WHITE));
        assert!(flat.iter().all(|&c| c == BLACK || c == WHITE));
        let dark = |cols: std::ops::Range<usize>| {
            grid.iter()
                .flat_map(|r| &r[cols.clone()])
                .filter(|&&c| c == BLACK)
                .count()
        };
        assert!(dark(0..10) > dark(10..20));
    }

    #[test]
    fn non_square_input_is_center_cropped() {
        let img = RgbImage::from_fn(120, 40, |x, _| match x / 40 {
            0 => Rgb([255, 0, 0]),
            1 => Rgb([0, 255, 0]),
            _ => Rgb([0, 0, 255]),
        });
        let src = decode(&jpeg(&img)).unwrap();
        // Spread 0: JPEG noise cannot flip a nearest-color choice into a flaky exact assertion.
        let grid = pixelize_with(&src, 10, &[RED, GREEN, BLUE], 0.0);
        assert!(grid.iter().flatten().all(|&c| c == GREEN));
    }

    #[test]
    fn choose_covers_every_row() {
        use ProtocolType::*;
        let rows = [
            (Some("off"), Some(Kitty), ArtPath::Off),
            (Some("off"), None, ArtPath::Off),
            (Some("halfblocks"), Some(Kitty), ArtPath::HalfBlocks),
            (Some("halfblocks"), None, ArtPath::HalfBlocks),
            (Some("bogus"), Some(Sixel), ArtPath::Crisp), // unknown value: as unset
            (Some("bogus"), None, ArtPath::HalfBlocks),
            (None, Some(Kitty), ArtPath::Crisp),
            (None, Some(Sixel), ArtPath::Crisp),
            (None, Some(Iterm2), ArtPath::Crisp),
            (None, Some(Halfblocks), ArtPath::HalfBlocks),
            (None, None, ArtPath::HalfBlocks),
        ];
        for (env, q, want) in rows {
            assert_eq!(choose(env, q), want, "{env:?} {q:?}");
        }
    }

    #[test]
    fn should_query_only_for_hinted_terminals() {
        for tp in ["ghostty", "iTerm.app", "WezTerm", "kitty", "GHOSTTY"] {
            assert!(should_query(None, Some(tp), None, None), "{tp}");
        }
        for tp in [Some("Apple_Terminal"), Some("rex"), None] {
            assert!(
                !should_query(None, tp, Some("xterm-256color"), None),
                "{tp:?}"
            );
        }
        assert!(should_query(None, None, Some("xterm-kitty"), None));
        assert!(should_query(None, None, Some("xterm-ghostty"), None));
        assert!(should_query(None, None, None, Some("7")));
        assert!(!should_query(None, None, None, Some("")));
        assert!(should_query(Some("detect"), Some("rex"), None, None));
        assert!(should_query(Some("bogus"), Some("kitty"), None, None));
        assert!(!should_query(Some("bogus"), Some("rex"), None, None));
        for v in ["off", "halfblocks"] {
            assert!(!should_query(
                Some(v),
                Some("kitty"),
                Some("xterm-kitty"),
                Some("7")
            ));
        }
    }

    #[test]
    fn upscale_gives_solid_blocks() {
        let grid: Grid = (0..4)
            .map(|y| {
                (0..4)
                    .map(|x| Color::Rgb((x * 60) as u8, (y * 60) as u8, 7))
                    .collect()
            })
            .collect();
        let img = upscale(&grid, 40, 20); // 10 x 5 pixels per grid cell
        assert_eq!(img.dimensions(), (40, 20));
        for (x, y, p) in img.enumerate_pixels() {
            assert_eq!(p.0, [(x / 10 * 60) as u8, (y / 5 * 60) as u8, 7], "{x},{y}");
        }
    }

    #[test]
    fn output_dimensions() {
        let src = RgbImage::new(40, 40);
        for n in [10, 20] {
            let grid = pixelize(&src, n, &[BLACK, WHITE]);
            assert_eq!(grid.len(), n);
            assert!(grid.iter().all(|r| r.len() == n));
        }
    }

    #[test]
    fn garbage_bytes_give_none() {
        assert!(decode(b"not an image at all").is_none());
        assert!(decode(&[]).is_none());
    }

    #[test]
    fn one_pixel_image_decodes() {
        let src = decode(&jpeg(&RgbImage::from_pixel(1, 1, Rgb([200, 0, 0])))).unwrap();
        assert_eq!(src.dimensions(), (40, 40));
    }

    #[test]
    fn valid_id_is_eleven_url_safe_characters() {
        assert!(valid_id("dQw4w9WgXcQ"));
        assert!(valid_id("a-_Z0-_9aaa"));
        assert!(!valid_id("dQw4w9WgXc")); // 10
        assert!(!valid_id("dQw4w9WgXcQQ")); // 12
        assert!(!valid_id("dQw4w9WgX/Q"));
        assert!(!valid_id("dQw4w9WgXé"));
        assert!(!valid_id(""));
    }

    #[test]
    fn shell_palettes_have_no_duplicates() {
        for s in [&theme::TPS_L2, &theme::SUNSET, &theme::AMBER] {
            let p = shell_palette(s);
            assert!(!p.is_empty());
            assert!(p.iter().enumerate().all(|(i, c)| !p[..i].contains(c)));
        }
    }

    /// `TERMUSIC_THUMBS=dir cargo test contact_sheet -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn contact_sheet() {
        let Ok(dir) = std::env::var("TERMUSIC_THUMBS") else {
            println!("TERMUSIC_THUMBS not set; skipping");
            return;
        };
        let mut files: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "jpg"))
            .collect();
        files.sort();
        let shells = [&theme::TPS_L2, &theme::SUNSET, &theme::AMBER];
        let spreads = [0.0, 32.0, 64.0];
        const TILE: u32 = 160;
        const GAP: u32 = 8;
        let cols = 1 + (shells.len() * spreads.len()) as u32;
        let mut sheet = RgbImage::from_pixel(
            cols * (TILE + GAP) + GAP,
            files.len() as u32 * (TILE + GAP) + GAP,
            Rgb([40, 40, 40]),
        );
        for (row, path) in files.iter().enumerate() {
            let bytes = std::fs::read(path).unwrap();
            let y0 = GAP + row as u32 * (TILE + GAP);
            let orig = center_square(&image::load_from_memory(&bytes).unwrap(), TILE);
            image::imageops::replace(&mut sheet, &orig, GAP as i64, y0 as i64);
            let src = decode(&bytes).unwrap();
            let mut col = 1;
            for shell in shells {
                for spread in spreads {
                    let grid = pixelize_with(&src, 10, &shell_palette(shell), spread);
                    let x0 = GAP + col * (TILE + GAP);
                    for (gy, r) in grid.iter().enumerate() {
                        for (gx, c) in r.iter().enumerate() {
                            let Color::Rgb(r, g, b) = *c else { continue };
                            for (dx, dy) in (0..256).map(|i| (i % 16, i / 16)) {
                                let (px, py) = (x0 + gx as u32 * 16 + dx, y0 + gy as u32 * 16 + dy);
                                sheet.put_pixel(px, py, Rgb([r, g, b]));
                            }
                        }
                    }
                    col += 1;
                }
            }
            // Half-block preview in a truecolor terminal: TPS-L2 at spread 32.
            println!("{}", path.display());
            let grid = pixelize_with(&src, 10, &shell_palette(&theme::TPS_L2), 32.0);
            for pair in grid.chunks(2) {
                for (up, lo) in pair[0].iter().zip(&pair[1]) {
                    if let (Color::Rgb(a, b, c), Color::Rgb(d, e, f)) = (up, lo) {
                        print!("\x1b[38;2;{a};{b};{c}m\x1b[48;2;{d};{e};{f}m▀");
                    }
                }
                println!("\x1b[0m");
            }
        }
        let out = std::path::Path::new(&dir).join("contact-sheet.png");
        sheet.save(&out).unwrap();
        println!("wrote {}", out.display());
    }
}
