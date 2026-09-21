use anyhow::{bail, Context, Result};
use freetype::face::{KerningMode, LoadFlag};
use freetype::{Face, Library};
use image::imageops::{self, FilterType};
use image::{GrayImage, Luma, Rgb, RgbImage};
use reqwest::blocking::Client;
use std::fs;
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

pub struct Options {
    pub width: u32,
    pub fps: u32,
    pub duration: f64,
    pub preset: String,
    pub threads: u32,
    pub strategy: String,
}

struct Glyph {
    x: i32,
    top: i32,
    width: usize,
    height: usize,
    mask: Vec<u8>,
}

struct Text {
    glyphs: Vec<Glyph>,
    advance: f64,
    bounds: [i32; 4],
    ascender: i32,
}

// Pillow's BASIC layout uses FreeType's hinted advances and ascender anchor.
fn pixel(value: i64) -> i32 {
    ((value + 32) >> 6) as i32
}

fn rounded(value: f64) -> i32 {
    value.round_ties_even() as i32
}

fn font(library: &Library, size: u32) -> Result<Face> {
    let mac_font = Path::new("/System/Library/Fonts/HelveticaNeue.ttc");
    let face = if mac_font.exists() {
        library.new_face(mac_font, 0)?
    } else {
        library.new_memory_face(include_bytes!("../assets/Aileron-Regular.ttf").to_vec(), 0)?
    };
    face.set_pixel_sizes(0, size.max(1))?;
    Ok(face)
}

fn text(face: &Face, value: &str) -> Result<Text> {
    let ascender = pixel(
        face.size_metrics()
            .context("font has no size metrics")?
            .ascender,
    );
    let mut glyphs = Vec::with_capacity(value.chars().count());
    let (mut x_min, mut x_max, mut y_min, mut y_max) = (0, 0, 0, 0);
    let mut position: i64 = 0;
    let mut previous = 0;
    for character in value.chars() {
        let index = face.get_char_index(character as usize).unwrap_or(0);
        if previous != 0 && index != 0 && face.has_kerning() {
            let delta = face.get_kerning(previous, index, KerningMode::KerningDefault)?;
            // Preserve the BASIC layout's conversion before adding kerning.
            position += i64::from(pixel(delta.x));
        }
        face.load_glyph(index, LoadFlag::DEFAULT)?;
        let slot = face.glyph();
        let x = pixel(position);
        position += slot.metrics().horiAdvance;
        x_max = x_max.max(pixel(position));
        let bounds = slot
            .get_glyph()?
            .get_cbox(freetype::ffi::FT_GLYPH_BBOX_PIXELS);
        x_min = x_min.min(x + bounds.xMin as i32);
        x_max = x_max.max(x + bounds.xMax as i32);
        y_min = y_min.min(bounds.yMin as i32);
        y_max = y_max.max(bounds.yMax as i32);
        face.load_glyph(index, LoadFlag::RENDER)?;
        let slot = face.glyph();
        let bitmap = slot.bitmap();
        let width = bitmap.width() as usize;
        let height = bitmap.rows() as usize;
        let pitch = bitmap.pitch().unsigned_abs() as usize;
        let mut mask = vec![0; width * height];
        for row in 0..height {
            let source_row = if bitmap.pitch() >= 0 {
                row
            } else {
                height - row - 1
            };
            mask[row * width..(row + 1) * width]
                .copy_from_slice(&bitmap.buffer()[source_row * pitch..source_row * pitch + width]);
        }
        glyphs.push(Glyph {
            x: x + slot.bitmap_left(),
            top: slot.bitmap_top(),
            width,
            height,
            mask,
        });
        previous = index;
    }
    Ok(Text {
        glyphs,
        advance: position as f64 / 64.0,
        bounds: [x_min, ascender - y_max, x_max, ascender - y_min],
        ascender,
    })
}

impl Text {
    fn draw(&self, canvas: &mut RgbImage, x: f64, y: i32, color: u8) {
        let x = x.round() as i32;
        for glyph in &self.glyphs {
            let top = y + self.ascender - glyph.top;
            for row in 0..glyph.height {
                let target_y = top + row as i32;
                if target_y < 0 || target_y >= canvas.height() as i32 {
                    continue;
                }
                for column in 0..glyph.width {
                    let target_x = x + glyph.x + column as i32;
                    if target_x < 0 || target_x >= canvas.width() as i32 {
                        continue;
                    }
                    let alpha = u32::from(glyph.mask[row * glyph.width + column]);
                    if alpha != 0 {
                        let target = canvas.get_pixel_mut(target_x as u32, target_y as u32);
                        for channel in &mut target.0 {
                            *channel = ((u32::from(*channel) * (255 - alpha)
                                + u32::from(color) * alpha
                                + 127)
                                / 255) as u8;
                        }
                    }
                }
            }
        }
    }
}

fn avatar(login: &str, client: &Client) -> Result<RgbImage> {
    let cache = Path::new(".cache/avatars-512");
    fs::create_dir_all(cache)?;
    let name: String = login
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let path = cache.join(format!("{name}.png"));
    if path.exists() {
        return Ok(image::open(path)?.to_rgb8());
    }
    let mut url = reqwest::Url::parse("https://github.com/")?;
    url.path_segments_mut()
        .map_err(|_| anyhow::anyhow!("invalid avatar URL"))?
        .push(&format!("{login}.png"));
    url.query_pairs_mut().append_pair("size", "512");
    let response = client.get(url).send()?.error_for_status()?;
    let mut content = Vec::new();
    response.take(2_000_000).read_to_end(&mut content)?;
    let source = image::load_from_memory(&content)?.to_rgb8();
    static CACHE_SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let sequence = CACHE_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let temporary = cache.join(format!(".{name}.{}.{sequence}.png", std::process::id()));
    if let Err(error) = source.save_with_format(&temporary, image::ImageFormat::Png) {
        let _ = fs::remove_file(&temporary);
        return Err(error.into());
    }
    if let Err(error) = fs::rename(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        return Err(error.into());
    }
    Ok(source)
}

fn fallback_avatar(login: &str, library: &Library) -> Result<RgbImage> {
    let mut image = RgbImage::from_pixel(128, 128, Rgb([228, 228, 231]));
    let initials: String = login.chars().take(2).flat_map(char::to_uppercase).collect();
    text(&font(library, 36)?, &initials)?.draw(&mut image, 28.0, 42, 0);
    Ok(image)
}

struct Sprite {
    pixels: Vec<u8>,
    diameter: usize,
    y: i32,
    radius: i32,
    count: u64,
}

fn sprite(source: &RgbImage, radius: i32, y: i32, count: u64) -> Sprite {
    let diameter = (radius * 2) as u32;
    let side = source.width().min(source.height());
    let crop = imageops::crop_imm(
        source,
        (source.width() - side) / 2,
        (source.height() - side) / 2,
        side,
        side,
    )
    .to_image();
    let mut ball = imageops::resize(&crop, diameter, diameter, FilterType::Lanczos3);
    let mask_side = diameter * 2;
    let center = (f64::from(mask_side) - 1.0) / 2.0;
    let mask_radius = f64::from(mask_side) / 2.0;
    let mask = GrayImage::from_fn(mask_side, mask_side, |x, y| {
        let inside = (f64::from(x) - center).powi(2) + (f64::from(y) - center).powi(2)
            <= mask_radius * mask_radius;
        Luma([if inside { 255 } else { 0 }])
    });
    let mask = imageops::resize(&mask, diameter, diameter, FilterType::Lanczos3);
    // The accepted layout keeps the entire arena white, so flatten once.
    for (pixel, alpha) in ball.pixels_mut().zip(mask.pixels()) {
        let alpha = u32::from(alpha[0]);
        for channel in &mut pixel.0 {
            *channel = ((u32::from(*channel) * alpha + 255 * (255 - alpha) + 127) / 255) as u8;
        }
    }
    Sprite {
        pixels: ball.into_raw(),
        diameter: diameter as usize,
        y,
        radius,
        count,
    }
}

fn grouped_number(value: u64) -> String {
    let digits = value.to_string();
    let mut result = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index != 0 && (digits.len() - index).is_multiple_of(3) {
            result.push(',');
        }
        result.push(digit);
    }
    result
}

fn artwork(
    repo: &str,
    subtitle: &str,
    rows: &[(String, u64)],
    metric: &str,
    width: u32,
) -> Result<(Vec<u8>, Vec<Sprite>, i32)> {
    let library = Library::init()?;
    let scale = f64::from(width) * 2.0 / 1080.0;
    let n = |value: f64| rounded(value * scale);
    let mut canvas = RgbImage::from_pixel(width * 2, width * 2, Rgb([255; 3]));
    let title = format!(
        "{} in {subtitle}",
        if metric == "changes" {
            "Lines changed"
        } else {
            "Pull requests merged"
        }
    );
    let mut title_size = n(48.0) as u32;
    let title = loop {
        let shaped = text(&font(&library, title_size)?, &title)?;
        if shaped.advance <= f64::from(n(1000.0)) || title_size <= 1 {
            break shaped;
        }
        title_size -= 1;
    };
    title.draw(
        &mut canvas,
        (f64::from(width * 2) - f64::from(title.bounds[2] - title.bounds[0])) / 2.0,
        n(80.0),
        0,
    );
    let watermark = text(
        &font(&library, n(22.0) as u32)?,
        &format!("github.com/{repo}"),
    )?;
    let watermark_y = (width * 2) as i32 - n(36.0) - watermark.bounds[3];
    let watermark_top = watermark_y + watermark.bounds[1];
    let center = f64::from(n(80.0) + title.bounds[3] + watermark_top) / (2.0 * scale);
    watermark.draw(
        &mut canvas,
        f64::from(width * 2) - f64::from(n(36.0)) - watermark.advance.round(),
        watermark_y,
        163,
    );
    let name_font = font(&library, n(44.0) as u32)?;
    let count_font = font(&library, n(25.0) as u32)?;
    let mut names = Vec::with_capacity(rows.len());
    for (login, _) in rows {
        let mut label = login.clone();
        loop {
            let shaped = text(&name_font, &label)?;
            if shaped.advance <= f64::from(n(360.0)) {
                names.push(shaped);
                break;
            }
            label = label.trim_end_matches('…').to_owned();
            label.pop();
            label.push('…');
        }
    }
    let label_width = names
        .iter()
        .map(|name| name.advance)
        .fold(0.0_f64, f64::max)
        / scale;
    let left = rounded((70.0 + label_width + 24.0 + 76.0).max(350.0) * f64::from(width) / 1080.0);
    let row_height = if rows.len() <= 3 {
        250.0
    } else if rows.len() <= 6 {
        500.0 / (rows.len() - 1) as f64
    } else {
        700.0 / (rows.len() - 1) as f64
    };
    let client = Client::builder()
        .user_agent("release-balls")
        .timeout(Duration::from_secs(15))
        .build()?;
    let sources: Vec<Result<RgbImage>> = std::thread::scope(|scope| {
        let handles: Vec<_> = rows
            .iter()
            .map(|(login, _)| {
                let client = &client;
                scope.spawn(move || avatar(login, client))
            })
            .collect();
        handles
            .into_iter()
            .map(|handle| {
                handle
                    .join()
                    .unwrap_or_else(|_| Err(anyhow::anyhow!("avatar download panicked")))
            })
            .collect()
    });
    let name_bottom = text(&name_font, "Ag")?.bounds[3];
    let mut sprites = Vec::with_capacity(rows.len());
    for (index, ((login, count), source)) in rows.iter().zip(sources).enumerate() {
        let y = center + (index as f64 - (rows.len() as f64 - 1.0) / 2.0) * row_height;
        let radius = rounded(76.0_f64.min(row_height * 0.4) * f64::from(width) / 1080.0);
        names[index].draw(&mut canvas, f64::from(n(70.0)), n(y - 43.0), 0);
        let count_text = text(
            &count_font,
            &format!(
                "{} {}",
                grouped_number(*count),
                if metric == "changes" {
                    "lines"
                } else {
                    "merges"
                }
            ),
        )?;
        count_text.draw(
            &mut canvas,
            f64::from(n(72.0)),
            n(y - 43.0) + name_bottom + n(12.0) - count_text.bounds[1],
            0,
        );
        let source = match source {
            Ok(image) => image,
            Err(_) => fallback_avatar(login, &library)?,
        };
        sprites.push(sprite(
            &source,
            radius,
            rounded(y * f64::from(width) / 1080.0),
            *count,
        ));
    }
    if rows.is_empty() {
        text(&name_font, "No matching contributions")?.draw(
            &mut canvas,
            f64::from(n(70.0)),
            n(340.0),
            0,
        );
    }
    Ok((downsample_artwork(&canvas, width).into_raw(), sprites, left))
}

fn downsample_artwork(canvas: &RgbImage, width: u32) -> RgbImage {
    // At 2x downsampling Lanczos3 needs six source pixels of support.
    // Even crop bounds preserve the full canvas's sampling coordinates.
    let mut regions: Vec<(u32, u32, u32, u32)> = Vec::new();
    for (y, row) in canvas
        .as_raw()
        .chunks_exact(canvas.width() as usize * 3)
        .enumerate()
    {
        let Some(left) = row.chunks_exact(3).position(|pixel| pixel != [255; 3]) else {
            continue;
        };
        let right = row
            .chunks_exact(3)
            .rposition(|pixel| pixel != [255; 3])
            .unwrap()
            + 1;
        let (left, right, y) = (left as u32, right as u32, y as u32);
        if let Some(region) = regions.last_mut().filter(|region| y <= region.3 + 16) {
            region.0 = region.0.min(left);
            region.2 = region.2.max(right);
            region.3 = y + 1;
        } else {
            regions.push((left, y, right, y + 1));
        }
    }
    let mut output = RgbImage::from_pixel(width, width, Rgb([255; 3]));
    for (left, top, right, bottom) in regions {
        let left = left.saturating_sub(8) / 2 * 2;
        let top = top.saturating_sub(8) / 2 * 2;
        let right = ((right + 9) / 2 * 2).min(canvas.width());
        let bottom = ((bottom + 9) / 2 * 2).min(canvas.height());
        let crop = imageops::crop_imm(canvas, left, top, right - left, bottom - top).to_image();
        let resized = imageops::resize(
            &crop,
            (right - left) / 2,
            (bottom - top) / 2,
            FilterType::Lanczos3,
        );
        imageops::replace(
            &mut output,
            &resized,
            i64::from(left / 2),
            i64::from(top / 2),
        );
    }
    output
}

fn copy_rectangle(
    target: &mut [u8],
    source: &[u8],
    width: usize,
    x: i32,
    y: i32,
    diameter: usize,
    source_is_sprite: bool,
) {
    let left = x.max(0) as usize;
    let right = (x + diameter as i32).clamp(0, width as i32) as usize;
    let top = y.max(0) as usize;
    let bottom = (y + diameter as i32).clamp(0, width as i32) as usize;
    if left >= right || top >= bottom {
        return;
    }
    for row in top..bottom {
        let target_start = (row * width + left) * 3;
        let source_start = if source_is_sprite {
            ((row as i32 - y) as usize * diameter + (left as i32 - x) as usize) * 3
        } else {
            target_start
        };
        let length = (right - left) * 3;
        target[target_start..target_start + length]
            .copy_from_slice(&source[source_start..source_start + length]);
    }
}

// BT.601 limited-range planes. Chroma is the average of a complete 2x2 cell.
fn luma(rgb: &[u8]) -> u8 {
    ((66 * i32::from(rgb[0]) + 129 * i32::from(rgb[1]) + 25 * i32::from(rgb[2]) + 128) / 256 + 16)
        as u8
}

fn chroma(sum: [i32; 3]) -> (u8, u8) {
    let [r, g, b] = sum;
    let u = ((-38 * r - 74 * g + 112 * b + 512) >> 10) + 128;
    let v = ((112 * r - 94 * g - 18 * b + 512) >> 10) + 128;
    (u as u8, v as u8)
}

fn yuv_background(rgb: &[u8], width: usize) -> Vec<u8> {
    let pixels = width * width;
    let mut planes = vec![128; pixels * 3 / 2];
    for (target, source) in planes[..pixels].iter_mut().zip(rgb.chunks_exact(3)) {
        *target = luma(source);
    }
    for y in (0..width).step_by(2) {
        for x in (0..width).step_by(2) {
            let mut sum = [0; 3];
            for dy in 0..2 {
                for dx in 0..2 {
                    let start = ((y + dy) * width + x + dx) * 3;
                    for channel in 0..3 {
                        sum[channel] += i32::from(rgb[start + channel]);
                    }
                }
            }
            let (u, v) = chroma(sum);
            let cell = (y / 2) * (width / 2) + x / 2;
            planes[pixels + cell] = u;
            planes[pixels + pixels / 4 + cell] = v;
        }
    }
    planes
}

struct ChromaPatch {
    u: Vec<u8>,
    v: Vec<u8>,
    width: usize,
    height: usize,
}
struct YuvSprite {
    y: Vec<u8>,
    variants: [ChromaPatch; 4],
}

impl YuvSprite {
    fn new(sprite: &Sprite) -> Self {
        let diameter = sprite.diameter;
        let variants = std::array::from_fn(|parity| {
            let (px, py) = (parity & 1, parity >> 1);
            let (width, height) = (diameter / 2 + px, diameter / 2 + py);
            let mut patch = ChromaPatch {
                u: vec![128; width * height],
                v: vec![128; width * height],
                width,
                height,
            };
            for y in 0..height {
                for x in 0..width {
                    let mut sum = [0; 3];
                    for dy in 0..2 {
                        for dx in 0..2 {
                            let sx = (x * 2 + dx) as isize - px as isize;
                            let sy = (y * 2 + dy) as isize - py as isize;
                            let rgb = if sx >= 0
                                && sy >= 0
                                && sx < diameter as isize
                                && sy < diameter as isize
                            {
                                let start = (sy as usize * diameter + sx as usize) * 3;
                                &sprite.pixels[start..start + 3]
                            } else {
                                &[255, 255, 255]
                            };
                            for channel in 0..3 {
                                sum[channel] += i32::from(rgb[channel]);
                            }
                        }
                    }
                    let (u, v) = chroma(sum);
                    patch.u[y * width + x] = u;
                    patch.v[y * width + x] = v;
                }
            }
            patch
        });
        Self {
            y: sprite.pixels.chunks_exact(3).map(luma).collect(),
            variants,
        }
    }
}

fn copy_plane(
    target: &mut [u8],
    source: &[u8],
    target_stride: usize,
    source_stride: usize,
    offset: usize,
    width: usize,
    height: usize,
) {
    for row in 0..height {
        let start = offset + row * target_stride;
        target[start..start + width]
            .copy_from_slice(&source[row * source_stride..row * source_stride + width]);
    }
}

fn yuv_rectangle(
    target: &mut [u8],
    background: &[u8],
    width: usize,
    x: i32,
    y: i32,
    diameter: usize,
    sprite: Option<&YuvSprite>,
) {
    let (x, y) = (x as usize, y as usize);
    let pixels = width * width;
    let offset = y * width + x;
    let uv_offset = pixels + (y / 2) * (width / 2) + x / 2;
    let (uv_width, uv_height) = (diameter / 2 + x % 2, diameter / 2 + y % 2);
    if let Some(sprite) = sprite {
        copy_plane(
            target, &sprite.y, width, diameter, offset, diameter, diameter,
        );
        let patch = &sprite.variants[(x % 2) + (y % 2) * 2];
        copy_plane(
            target,
            &patch.u,
            width / 2,
            patch.width,
            uv_offset,
            patch.width,
            patch.height,
        );
        copy_plane(
            target,
            &patch.v,
            width / 2,
            patch.width,
            uv_offset + pixels / 4,
            patch.width,
            patch.height,
        );
    } else {
        copy_plane(
            target,
            &background[offset..],
            width,
            width,
            offset,
            diameter,
            diameter,
        );
        copy_plane(
            target,
            &background[uv_offset..],
            width / 2,
            width / 2,
            uv_offset,
            uv_width,
            uv_height,
        );
        let v_offset = uv_offset + pixels / 4;
        copy_plane(
            target,
            &background[v_offset..],
            width / 2,
            width / 2,
            v_offset,
            uv_width,
            uv_height,
        );
    }
}

pub fn render(
    repo: &str,
    subtitle: &str,
    rows: &[(String, u64)],
    metric: &str,
    output: &Path,
    formats: &[&str],
    options: &Options,
) -> Result<Vec<String>> {
    if options.width < 2
        || options.fps == 0
        || !options.duration.is_finite()
        || options.duration <= 0.0
    {
        bail!("invalid render dimensions or timing");
    }
    if formats.is_empty()
        || formats
            .iter()
            .any(|format| !matches!(*format, "mp4" | "gif"))
    {
        bail!("expected mp4 and/or gif output formats");
    }
    let yuv = options.strategy == "yuv" && formats == ["mp4"];
    if yuv && !options.width.is_multiple_of(2) {
        bail!("YUV420 requires an even width");
    }
    // Keep only one page's artwork in memory. Every contributor appears; text
    // sizes stay fixed and speeds use the maximum across the entire ranking.
    const ROWS_PER_PAGE: usize = 8;
    let page_count = rows.len().max(1).div_ceil(ROWS_PER_PAGE);
    let prepare_page = |page: usize| -> Result<_> {
        let start = page * ROWS_PER_PAGE;
        let end = (start + ROWS_PER_PAGE).min(rows.len());
        let (background, sprites, mut left) =
            artwork(repo, subtitle, &rows[start..end], metric, options.width)?;
        if page_count > 1 {
            // Reserve the maximum label width so every page has the same track.
            left = rounded(530.0 * f64::from(options.width) / 1080.0);
        }
        let yuv_sprites: Vec<_> = if yuv {
            sprites.iter().map(YuvSprite::new).collect()
        } else {
            Vec::new()
        };
        let background = if yuv {
            yuv_background(&background, options.width as usize)
        } else {
            background
        };
        Ok((background, sprites, left, yuv_sprites))
    };
    let (mut background, mut sprites, mut left, mut yuv_sprites) = prepare_page(0)?;
    let mut command = Command::new("ffmpeg");
    command
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "rawvideo",
            "-pix_fmt",
            if yuv { "yuv420p" } else { "rgb24" },
            "-s",
        ])
        .arg(format!("{}x{}", options.width, options.width))
        .args(["-r", &options.fps.to_string(), "-i", "-", "-an"]);
    let stem = output.to_string_lossy();
    if formats.contains(&"mp4") {
        command.args([
            "-c:v",
            "libx264",
            "-threads",
            &options.threads.to_string(),
            "-preset",
            &options.preset,
            "-crf",
            "23",
            "-pix_fmt",
            "yuv420p",
            "-movflags",
            "+faststart",
        ]);
        if yuv {
            command.args([
                "-colorspace",
                "smpte170m",
                "-color_primaries",
                "smpte170m",
                "-color_trc",
                "smpte170m",
                "-color_range",
                "tv",
                "-chroma_sample_location",
                "center",
            ]);
        }
        command.arg(format!("{stem}.mp4"));
    }
    if formats.contains(&"gif") {
        command.args(["-filter_threads", "1", "-vf",
            "fps=15,scale=480:-1:flags=bilinear,split[a][b];[a]palettegen=stats_mode=diff[p];[b][p]paletteuse=dither=none:diff_mode=rectangle",
            "-loop", "0"]).arg(format!("{stem}.gif"));
    }
    if let Some(parent) = output.parent().filter(|path| !path.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    let mut process = command
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("ffmpeg is required; install it using your system package manager")?;
    let mut stdin = process.stdin.take().context("ffmpeg stdin unavailable")?;
    // Reduce pipe wakeups while streaming full frames. Restricted kernels may
    // reject enlargement; their default pipe remains usable.
    #[cfg(target_os = "linux")]
    if formats.contains(&"mp4") {
        let _ = rustix::pipe::fcntl_setpipe_size(&stdin, 1024 * 1024);
    }
    let mut stderr = process.stderr.take().context("ffmpeg stderr unavailable")?;
    let errors = std::thread::spawn(move || {
        let mut message = String::new();
        let _ = stderr.read_to_string(&mut message);
        message
    });
    let mut canvas = background.clone();
    let mut previous = Vec::with_capacity(sprites.len());
    let mut span = rounded(f64::from(options.width) * 980.0 / 1080.0) - left;
    let maximum = rows
        .iter()
        .map(|(_, count)| *count)
        .max()
        .unwrap_or(0)
        .max(1);
    let duration = if page_count > 1 {
        options.duration.max(page_count as f64 * 4.0)
    } else {
        options.duration
    };
    let frames = rounded(f64::from(options.fps) * duration).max(1);
    let mut write_result: Result<()> = Ok(());
    let mut current_page = 0;
    let mut page_start = 0;
    for frame in 0..frames {
        let page = frame as usize * page_count / frames as usize;
        if page != current_page {
            match prepare_page(page) {
                Ok(artwork) => (background, sprites, left, yuv_sprites) = artwork,
                Err(error) => {
                    write_result = Err(error);
                    break;
                }
            }
            canvas.copy_from_slice(&background);
            previous.clear();
            span = rounded(f64::from(options.width) * 980.0 / 1080.0) - left;
            current_page = page;
            page_start = frame;
        }
        if options.strategy == "cached" {
            canvas.copy_from_slice(&background);
        } else {
            for &(x, y, diameter) in &previous {
                if yuv {
                    yuv_rectangle(
                        &mut canvas,
                        &background,
                        options.width as usize,
                        x,
                        y,
                        diameter,
                        None,
                    );
                } else {
                    copy_rectangle(
                        &mut canvas,
                        &background,
                        options.width as usize,
                        x,
                        y,
                        diameter,
                        false,
                    );
                }
            }
        }
        previous.clear();
        for (index, sprite) in sprites.iter().enumerate() {
            let distance = f64::from(span)
                * (sprite.count as f64 / maximum as f64)
                * f64::from(frame - page_start)
                / f64::from(options.fps);
            let phase = distance % f64::from(2 * span);
            let x = left
                + rounded(if phase <= f64::from(span) {
                    phase
                } else {
                    f64::from(2 * span) - phase
                });
            let x = x - sprite.radius;
            let y = sprite.y - sprite.radius;
            if yuv {
                yuv_rectangle(
                    &mut canvas,
                    &background,
                    options.width as usize,
                    x,
                    y,
                    sprite.diameter,
                    Some(&yuv_sprites[index]),
                );
            } else {
                copy_rectangle(
                    &mut canvas,
                    &sprite.pixels,
                    options.width as usize,
                    x,
                    y,
                    sprite.diameter,
                    true,
                );
            }
            previous.push((x, y, sprite.diameter));
        }
        if let Err(error) = stdin.write_all(&canvas) {
            write_result = Err(error.into());
            break;
        }
    }
    drop(stdin);
    let status = process.wait()?;
    let error = errors
        .join()
        .unwrap_or_else(|_| "could not collect ffmpeg output".to_owned());
    if !status.success() {
        bail!("ffmpeg failed: {error}");
    }
    write_result.context("writing frames to ffmpeg")?;
    Ok(formats
        .iter()
        .map(|format| format!("{stem}.{format}"))
        .collect())
}
