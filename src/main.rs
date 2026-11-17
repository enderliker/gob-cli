//! Find where an image lives in the Gallery of Babel.
//!
//! ```text
//! babelia encode photo.jpg          # print the image's location
//! babelia decode 1234... out.png    # render a location back to an image
//! ```

mod babelia;
mod consts;

use image::{imageops, ImageBuffer, Rgb, RgbImage};
use rug::Integer;
use std::process::ExitCode;

const USAGE: &str = "\
babelia - locate images in the Gallery of Babel (babelia.libraryofbabel.info)

USAGE:
    babelia encode <image> [options]     print the image's location in the gallery
    babelia decode <location> <output>   render a location back into an image
    babelia palette                      print the 4096-colour palette (hex, one per line)

ENCODE OPTIONS:
    --fill bars|stretch     keep proportions with bars (default) or stretch to 640x416
    --orient auto|landscape|portrait   orientation handling (default: auto)
    --border <colour>       bar colour: hex like #222222, or grey/black/white/
                            red/green/blue/yellow/orange/purple
    --frame <n>             frame to use for multi-frame images such as GIFs
    --preview <path>        also write the prepared 640x416 palette image

DECODE OPTIONS:
    --portrait              rotate the result 90 degrees, as the site's portrait mode does

Note: the gallery only holds 640x416 images drawn from a 4096-colour palette,
so larger images are scaled down and quantized. The location printed is the
same one babelia.libraryofbabel.info's own image search would give for that
prepared image (modulo resampling differences).
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(code) => code,
        Err(msg) => {
            eprintln!("error: {msg}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[String]) -> Result<ExitCode, String> {
    let Some(command) = args.first().map(String::as_str) else {
        print!("{USAGE}");
        return Ok(ExitCode::FAILURE);
    };

    match command {
        "-h" | "--help" | "help" => {
            print!("{USAGE}");
            Ok(ExitCode::SUCCESS)
        }
        "encode" => encode_cmd(&args[1..]),
        "decode" => decode_cmd(&args[1..]),
        "palette" => {
            for i in 0..4096u16 {
                let [r, g, b] = babelia::palette_rgb(i);
                println!("#{r:02x}{g:02x}{b:02x}");
            }
            Ok(ExitCode::SUCCESS)
        }
        other => Err(format!("unknown command '{other}' (try --help)")),
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Fill {
    Bars,
    Stretch,
}

#[derive(Clone, Copy, PartialEq)]
enum Orient {
    Auto,
    Landscape,
    Portrait,
}

fn encode_cmd(args: &[String]) -> Result<ExitCode, String> {
    let mut path: Option<&str> = None;
    let mut fill = Fill::Bars;
    let mut orient = Orient::Auto;
    let mut border = [0x22u8, 0x22, 0x22];
    let mut frame = 0usize;
    let mut preview: Option<&str> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--fill" => {
                let v = args.get(i + 1).ok_or("--fill needs a value")?;
                fill = match v.as_str() {
                    "bars" => Fill::Bars,
                    "stretch" => Fill::Stretch,
                    other => return Err(format!("unknown fill '{other}'")),
                };
                i += 2;
            }
            "--orient" => {
                let v = args.get(i + 1).ok_or("--orient needs a value")?;
                orient = match v.as_str() {
                    "auto" => Orient::Auto,
                    "landscape" => Orient::Landscape,
                    "portrait" => Orient::Portrait,
                    other => return Err(format!("unknown orient '{other}'")),
                };
                i += 2;
            }
            "--border" => {
                let v = args.get(i + 1).ok_or("--border needs a value")?;
                border = parse_color(v)?;
                i += 2;
            }
            "--frame" => {
                let v = args.get(i + 1).ok_or("--frame needs a value")?;
                frame = v.parse().map_err(|_| format!("bad frame '{v}'"))?;
                i += 2;
            }
            "--preview" => {
                preview = Some(args.get(i + 1).ok_or("--preview needs a path")?);
                i += 2;
            }
            other if other.starts_with("--") => {
                return Err(format!("unknown option '{other}'"));
            }
            other => {
                if path.is_some() {
                    return Err("more than one input image given".into());
                }
                path = Some(other);
                i += 1;
            }
        }
    }

    let path = path.ok_or("no input image given")?;
    let prepared = prepare_image(path, fill, orient, border, frame)?;

    let mut pixels = Vec::with_capacity(babelia::PIXELS);
    for p in prepared.pixels() {
        pixels.push(babelia::quantize16([p[0], p[1], p[2]]));
    }

    if let Some(path) = preview {
        write_preview(&pixels, path)?;
    }

    let location = babelia::encode(&pixels);
    println!("{location}");
    Ok(ExitCode::SUCCESS)
}

fn decode_cmd(args: &[String]) -> Result<ExitCode, String> {
    let mut location: Option<&str> = None;
    let mut from_file: Option<&str> = None;
    let mut output: Option<&str> = None;
    let mut portrait = false;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--portrait" => {
                portrait = true;
                i += 1;
            }
            "--file" => {
                from_file = Some(args.get(i + 1).ok_or("--file needs a path")?);
                i += 2;
            }
            other if other.starts_with("--") => {
                return Err(format!("unknown option '{other}'"));
            }
            other => {
                if location.is_none() && from_file.is_none() {
                    location = Some(other);
                } else if output.is_none() {
                    output = Some(other);
                } else {
                    return Err("too many arguments".into());
                }
                i += 1;
            }
        }
    }

    // A location is ~1 million digits, far past the kernel's argument limit,
    // so reading it from a file (or stdin with `--file -`) is the normal path.
    let raw = match from_file {
        Some("-") => {
            let mut s = String::new();
            std::io::Read::read_to_string(&mut std::io::stdin(), &mut s)
                .map_err(|e| format!("could not read stdin: {e}"))?;
            s
        }
        Some(path) => {
            std::fs::read_to_string(path).map_err(|e| format!("could not read {path}: {e}"))?
        }
        None => location.ok_or("no location given (pass it directly or use --file)")?.to_string(),
    };
    let digits: String = raw.chars().filter(|c| c.is_ascii_digit()).collect();
    let output = output.ok_or("no output path given")?;
    if digits.is_empty() {
        return Err("a location is a decimal number".into());
    }

    let value = Integer::from_str_radix(&digits, 10)
        .map_err(|e| format!("bad location: {e}"))?;
    let pixels = babelia::decode(&value);

    let mut image = RgbImage::new(babelia::WIDTH as u32, babelia::HEIGHT as u32);
    for (i, &index) in pixels.iter().enumerate() {
        let [r, g, b] = babelia::palette_rgb(index);
        let x = (i % babelia::WIDTH) as u32;
        let y = (i / babelia::WIDTH) as u32;
        image.put_pixel(x, y, Rgb([r, g, b]));
    }
    if portrait {
        image = imageops::rotate270(&image);
    }

    image
        .save(output)
        .map_err(|e| format!("could not write {output}: {e}"))?;
    eprintln!("wrote {output}");
    Ok(ExitCode::SUCCESS)
}

/// Writes the prepared 640x416 palette image the location is computed from.
fn write_preview(pixels: &[u16], path: &str) -> Result<(), String> {
    let mut image = RgbImage::new(babelia::WIDTH as u32, babelia::HEIGHT as u32);
    for (i, &index) in pixels.iter().enumerate() {
        let [r, g, b] = babelia::palette_rgb(index);
        let x = (i % babelia::WIDTH) as u32;
        let y = (i / babelia::WIDTH) as u32;
        image.put_pixel(x, y, Rgb([r, g, b]));
    }
    image
        .save(path)
        .map_err(|e| format!("could not write preview {path}: {e}"))
}

fn parse_color(spec: &str) -> Result<[u8; 3], String> {
    let named = match spec.to_ascii_lowercase().as_str() {
        "grey" | "gray" => Some([0x22, 0x22, 0x22]),
        "black" => Some([0, 0, 0]),
        "white" => Some([255, 255, 255]),
        "red" => Some([255, 0, 0]),
        "green" => Some([0, 255, 0]),
        "blue" => Some([0, 0, 255]),
        "yellow" => Some([255, 255, 0]),
        "orange" => Some([255, 165, 0]),
        "purple" => Some([128, 0, 128]),
        _ => None,
    };
    if let Some(rgb) = named {
        return Ok(rgb);
    }

    let hex = spec.strip_prefix('#').unwrap_or(spec);
    let digits: Vec<u8> = hex
        .chars()
        .map(|c| c.to_digit(16).map(|d| d as u8))
        .collect::<Option<_>>()
        .ok_or_else(|| format!("bad colour '{spec}'"))?;

    match digits.len() {
        3 => {
            // 12-bit form, one nibble per channel.
            Ok([digits[0] * 17, digits[1] * 17, digits[2] * 17])
        }
        6 => Ok([
            digits[0] * 16 + digits[1],
            digits[2] * 16 + digits[3],
            digits[4] * 16 + digits[5],
        ]),
        _ => Err(format!("bad colour '{spec}'")),
    }
}

/// Area-average downscale, matching ImageMagick's `scale()`, which is what the
/// site's own image search applies before quantising. Works in 16-bit
/// "quantum" space like ImageMagick, so no precision is lost before the
/// palette quantisation.
fn scale_area(src: &Rgb16Image, dw: u32, dh: u32) -> Rgb16Image {
    let (sw, sh) = src.dimensions();
    let mut out = Rgb16Image::new(dw, dh);
    if sw == 0 || sh == 0 || dw == 0 || dh == 0 {
        return out;
    }
    let sx = f64::from(sw) / f64::from(dw);
    let sy = f64::from(sh) / f64::from(dh);

    for oy in 0..dh {
        let y0 = f64::from(oy) * sy;
        let y1 = y0 + sy;
        for ox in 0..dw {
            let x0 = f64::from(ox) * sx;
            let x1 = x0 + sx;
            let mut acc = [0f64; 3];
            let mut total = 0f64;
            for py in y0.floor() as u32..y1.ceil().min(f64::from(sh)) as u32 {
                let wy = (y1.min(f64::from(py + 1)) - y0.max(f64::from(py))).max(0.0);
                for px in x0.floor() as u32..x1.ceil().min(f64::from(sw)) as u32 {
                    let wx = (x1.min(f64::from(px + 1)) - x0.max(f64::from(px))).max(0.0);
                    let w = wx * wy;
                    if w <= 0.0 {
                        continue;
                    }
                    let p = src.get_pixel(px, py);
                    acc[0] += f64::from(p[0]) * w;
                    acc[1] += f64::from(p[1]) * w;
                    acc[2] += f64::from(p[2]) * w;
                    total += w;
                }
            }
            let px = if total > 0.0 {
                [
                    (acc[0] / total).round().clamp(0.0, 65535.0) as u16,
                    (acc[1] / total).round().clamp(0.0, 65535.0) as u16,
                    (acc[2] / total).round().clamp(0.0, 65535.0) as u16,
                ]
            } else {
                [0, 0, 0]
            };
            out.put_pixel(ox, oy, Rgb(px));
        }
    }
    out
}

fn prepare_image(
    path: &str,
    fill: Fill,
    orient: Orient,
    border: [u8; 3],
    frame: usize,
) -> Result<Rgb16Image, String> {
    let source = open_frame(path, frame)?;
    let mut image = source;

    let (mut w, mut h) = image.dimensions();

    match orient {
        Orient::Portrait => {
            image = imageops::rotate270(&image);
            std::mem::swap(&mut w, &mut h);
        }
        Orient::Auto => {
            if h > 416 && h > w {
                image = imageops::rotate270(&image);
                std::mem::swap(&mut w, &mut h);
            }
        }
        Orient::Landscape => {}
    }

    let (cw, ch) = (babelia::WIDTH as u32, babelia::HEIGHT as u32);

    if fill == Fill::Stretch {
        return Ok(imageops::resize(&image, cw, ch, imageops::FilterType::Lanczos3));
    }

    // Keep proportions: shrink only if needed, then centre on a card.
    let card = Rgb([
        u16::from(border[0]) * 257,
        u16::from(border[1]) * 257,
        u16::from(border[2]) * 257,
    ]);
    let mut canvas = Rgb16Image::from_pixel(cw, ch, card);
    if w > cw || h > ch {
        let by_height = f64::from(h) / f64::from(ch);
        let by_width = f64::from(w) / f64::from(cw);
        // ImageMagick's Geometry(double) truncates rather than rounds here;
        // matching that quirk is what makes the location agree with the site
        // pixel for pixel.
        let (nw, nh) = if by_height > by_width {
            let s = f64::from(ch) / f64::from(h);
            ((f64::from(w) * s) as u32, ch)
        } else {
            let s = f64::from(cw) / f64::from(w);
            (cw, (f64::from(h) * s) as u32)
        };
        let resized = scale_area(&image, nw.max(1), nh.max(1));
        // ImageMagick's CenterGravity rounds a half-pixel gap up.
        imageops::overlay(
            &mut canvas,
            &resized,
            i64::from(cw.saturating_sub(nw).div_ceil(2)),
            i64::from(ch.saturating_sub(nh).div_ceil(2)),
        );
        return Ok(canvas);
    }

    imageops::overlay(
        &mut canvas,
        &image,
        i64::from((cw - w).div_ceil(2)),
        i64::from((ch - h).div_ceil(2)),
    );
    Ok(canvas)
}

fn open_frame(path: &str, frame: usize) -> Result<Rgb16Image, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("could not read {path}: {e}"))?;

    if frame == 0 {
        return image::load_from_memory(&bytes)
            .map(|i| i.to_rgb16())
            .map_err(|e| format!("could not decode {path}: {e}"));
    }

    use image::AnimationDecoder;
    let decoder = image::codecs::gif::GifDecoder::new(std::io::Cursor::new(bytes))
        .map_err(|e| format!("could not decode {path} as a multi-frame image: {e}"))?;
    let frames = decoder
        .into_frames()
        .collect_frames()
        .map_err(|e| format!("could not decode frames of {path}: {e}"))?;
    let count = frames.len();
    let chosen = frames
        .into_iter()
        .nth(frame.saturating_sub(1).min(count.saturating_sub(1)))
        .ok_or_else(|| format!("{path} has no frames"))?;
    Ok(image::DynamicImage::ImageRgba8(chosen.into_buffer()).to_rgb16())
}

/// ImageMagick-style 16-bit quantum buffer.
type Rgb16Image = ImageBuffer<Rgb<u16>, Vec<u16>>;