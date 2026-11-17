//! The Babel Image Archives permutation.
//!
//! babelia.libraryofbabel.info contains every 640x416 image whose pixels come
//! from a 4096-colour palette (4 bits per channel). Each image has a location:
//! a number of about a million digits. The mapping is
//!
//! ```text
//! location --[LCG mod m]--> x --[bit tempering]--> pixel bits
//! ```
//!
//! where the LCG is `x = (a*location + c) mod m` and the tempering is a
//! Mersenne-twister-like set of xor-shifts. Both directions are exact
//! inverses, so any palette image has exactly one address and any address
//! one image.
//!
//! This is a port of the author's published implementation
//! (<https://github.com/librarianofbabel/libraryofbabel.info-algo>, CC BY-SA-NC).

use crate::consts;
use rug::{Integer, integer::Order};

/// Canvas width in pixels.
pub const WIDTH: usize = 640;
/// Canvas height in pixels.
pub const HEIGHT: usize = 416;
/// Quadrant width (the image is stored as 4x2 quadrants).
pub const QUAD_W: usize = 160;
/// Quadrant height.
pub const QUAD_H: usize = 208;
/// Pixels per quadrant.
pub const QUAD_PIXELS: usize = QUAD_W * QUAD_H;
/// Number of quadrants.
pub const QUADS: usize = 8;
/// Total pixels on the canvas.
pub const PIXELS: usize = WIDTH * HEIGHT;

/// Bits one quadrant occupies in the working integer.
const QUAD_BITS: u32 = 12 * QUAD_PIXELS as u32; // 399360
/// Each quadrant sits in a 399361-bit slot (one spare bit, as upstream).
const QUAD_SHIFT: u32 = consts::DIVVER_EXP; // 399361

const SHIFT_1: u32 = 1_098_239;
const SHIFT_2: u32 = 1_797_118;
const LEFT_1: u32 = 698_879;
const LEFT_2: u32 = 1_497_599;

/// The 4096 palette entries, expanded to 8-bit RGB (each nibble * 17).
pub fn palette_rgb(index: u16) -> [u8; 3] {
    let i = index & 0xFFF;
    [
        (((i >> 8) & 0xF) * 17) as u8,
        (((i >> 4) & 0xF) * 17) as u8,
        ((i & 0xF) * 17) as u8,
    ]
}

/// Maps one 8-bit channel onto the palette lattice: `round(v * 15 / 255)`.
/// Kept for tests and for callers that already work in 8 bits.
#[allow(dead_code)]
#[inline]
pub fn quantize_channel(v: u8) -> u16 {
    // Upstream: (v + 2184) / 4369 with `v` widened to 16 bits. The division
    // must stay in 32 bits: 255 * 257 + 2184 overflows a u16.
    ((u32::from(v) * 257 + 2184) / 4369) as u16
}

/// Palette index of an 8-bit RGB triple.
#[allow(dead_code)]
#[inline]
pub fn quantize(rgb: [u8; 3]) -> u16 {
    (quantize_channel(rgb[0]) << 8) | (quantize_channel(rgb[1]) << 4) | quantize_channel(rgb[2])
}

/// Maps a 16-bit sample (ImageMagick's `Quantum`) onto the palette lattice.
///
/// The site reads the pixel as a 16-bit quantum and does
/// `(value + 2184) / 4369`, where one palette step is 4369. Working at this
/// depth instead of rounding to 8 bits first avoids shifting a sample across
/// a palette boundary, which would change the resulting location.
#[inline]
pub fn quantize_channel16(v: u16) -> u16 {
    ((u32::from(v) + 2184) / 4369) as u16
}

/// Palette index of a 16-bit RGB triple.
#[inline]
pub fn quantize16(rgb: [u16; 3]) -> u16 {
    (quantize_channel16(rgb[0]) << 8)
        | (quantize_channel16(rgb[1]) << 4)
        | quantize_channel16(rgb[2])
}

/// Row-major canvas position of pixel `i` within quadrant `q`.
#[inline]
fn quad_coords(q: usize) -> (usize, usize) {
    (QUAD_W * (q % 4), QUAD_H * (q / 4))
}

/// Applies the forward tempering stage.
fn temper(mut x: Integer) -> Integer {
    let c = consts::get();
    let a = x.clone() >> SHIFT_1;
    x ^= a;
    let low1 = x.clone() & &c.maskone;
    x ^= low1 << LEFT_1;
    let low2 = x.clone() & &c.masktwo;
    x ^= low2 << LEFT_2;
    let b = x.clone() >> SHIFT_2;
    x ^= b;
    x
}

/// Inverts [`temper`]. Mirrors the sequence in the upstream source, whose
/// repeated unfoldings are what make each step invertible.
fn detemper(mut x: Integer) -> Integer {
    let c = consts::get();
    let top = x.clone() >> SHIFT_2;
    x ^= top;

    let mut r = x.clone() ^ ((x.clone() & &c.masktwo) << LEFT_2);
    x ^= (r.clone() & &c.masktwo) << LEFT_2;

    r = x.clone() ^ ((x.clone() & &c.maskone) << LEFT_1);
    r = x.clone() ^ ((r.clone() & &c.maskone) << LEFT_1);
    r = x.clone() ^ ((r.clone() & &c.maskone) << LEFT_1);
    r = x.clone() ^ ((r.clone() & &c.maskone) << LEFT_1);
    x ^= (r.clone() & &c.maskone) << LEFT_1;

    r = x.clone() ^ (x.clone() >> SHIFT_1);
    let r2 = x.clone() ^ (r.clone() >> SHIFT_1);
    x ^= r2 >> SHIFT_1;
    x
}

/// Reads the 12-bit value at index `i` from a little-endian packed byte slice.
#[inline]
fn read12(bytes: &[u8], i: usize) -> u16 {
    let bit = i * 12;
    let byte = bit / 8;
    let off = bit % 8;
    let mut acc = 0u32;
    for k in 0..3 {
        if let Some(&b) = bytes.get(byte + k) {
            acc |= u32::from(b) << (8 * k);
        }
    }
    ((acc >> off) & 0xFFF) as u16
}

/// Writes a 12-bit value at index `i` into a little-endian packed byte slice.
#[inline]
fn write12(bytes: &mut [u8], i: usize, value: u16) {
    let bit = i * 12;
    let byte = bit / 8;
    let off = bit % 8;
    let v = u32::from(value & 0xFFF) << off;
    for k in 0..3 {
        if let Some(slot) = bytes.get_mut(byte + k) {
            *slot |= ((v >> (8 * k)) & 0xFF) as u8;
        }
    }
}

/// Canvas index (row-major) for pixel `i` of quadrant `q`.
#[inline]
fn canvas_index(q: usize, i: usize) -> usize {
    let (x0, y0) = quad_coords(q);
    (y0 + i / QUAD_W) * WIDTH + x0 + i % QUAD_W
}

/// Expands a location into the 266,240 palette indices of its image.
pub fn decode(location: &Integer) -> Vec<u16> {
    let c = consts::get();
    let mut x = (Integer::from(&c.a * location) + &c.c) % &c.m;
    x = temper(x);

    let slot_mask = (Integer::from(1) << QUAD_SHIFT) - 1u32;
    let mut pixels = vec![0u16; PIXELS];
    for q in 0..QUADS {
        let slot = Integer::from(&x & &slot_mask);
        x >>= QUAD_SHIFT;
        let bytes = slot.to_digits::<u8>(Order::Lsf);
        for i in 0..QUAD_PIXELS {
            pixels[canvas_index(q, i)] = read12(&bytes, i);
        }
    }
    pixels
}

/// Packs palette indices into the working integer, precisely as the site does:
/// eight quadrants, each 399,360 bits, in 399,361-bit slots.
pub fn pack(pixels: &[u16]) -> Integer {
    assert_eq!(pixels.len(), PIXELS, "expected a {WIDTH}x{HEIGHT} canvas");

    let mut total = Integer::new();
    for q in (0..QUADS).rev() {
        let mut bytes = vec![0u8; QUAD_BITS as usize / 8];
        for i in 0..QUAD_PIXELS {
            write12(&mut bytes, i, pixels[canvas_index(q, i)]);
        }
        let packed = Integer::from_digits(&bytes, Order::Lsf);
        total <<= QUAD_SHIFT;
        total += packed;
    }
    total
}

/// Inverts the permutation: packed image bits -> gallery location.
pub fn location_from_packed(total: &Integer) -> Integer {
    let x = detemper(total.clone());
    let c = consts::get();
    let mut location = (Integer::from(&c.ainverse * (x - &c.c))) % &c.m;
    if location < 0 {
        location += &c.m;
    }
    location
}

/// Computes the location of an image given its 266,240 palette indices.
pub fn encode(pixels: &[u16]) -> Integer {
    location_from_packed(&pack(pixels))
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantize_covers_the_palette_lattice() {
        assert_eq!(quantize_channel(0), 0);
        assert_eq!(quantize_channel(255), 15);
        // Every palette level must map to itself.
        for level in 0..16u8 {
            assert_eq!(quantize_channel(level * 17), u16::from(level));
        }
        // And in between values round to the nearest level.
        for v in 0..=255u8 {
            let expected = (u32::from(v) * 15 + 127) / 255;
            assert_eq!(u32::from(quantize_channel(v)), expected, "channel {v}");
        }
    }

    #[test]
    fn palette_index_round_trips() {
        for i in 0..4096u16 {
            let [r, g, b] = palette_rgb(i);
            assert_eq!(quantize([r, g, b]), i);
        }
    }

    #[test]
    fn temper_and_detemper_are_inverse() {
        for seed in [1u32, 2, 7, 12345, 4096] {
            let x = Integer::from(seed) * Integer::from(1_000_003);
            let back = detemper(temper(x.clone()));
            assert_eq!(back, x, "seed {seed}");
        }
    }

    /// Upstream stores each quadrant in a 399,361-bit slot but only fills
    /// 399,360 bits with pixels, so the permutation is not onto the image
    /// space. The address recovered from an image therefore reproduces the
    /// image exactly, but need not be the original address (the site behaves
    /// the same way).
    #[test]
    fn decode_encode_round_trips_images() {
        for seed in [1u32, 2, 4097] {
            let location = Integer::from(seed) * Integer::from(7919);
            let pixels = decode(&location);
            let again = decode(&encode(&pixels));
            assert_eq!(again, pixels, "seed {seed}");
        }
    }

    /// Regression fixture: the first four pixels of location 1, as rendered by
    /// babelia.libraryofbabel.info (its JPEG agrees to within one palette step).
    #[test]
    fn location_one_matches_the_site() {
        let pixels = decode(&Integer::from(1));
        assert_eq!(&pixels[..4], &[0x21F, 0x6B8, 0x394, 0x32B]);
    }

    #[test]
    fn twelve_bit_packing_round_trips() {
        let values: Vec<u16> = (0..1000).map(|i| (i * 37 % 4096) as u16).collect();
        let mut bytes = vec![0u8; 1000 * 12 / 8];
        for (i, &v) in values.iter().enumerate() {
            write12(&mut bytes, i, v);
        }
        for (i, &v) in values.iter().enumerate() {
            assert_eq!(read12(&bytes, i), v);
        }
    }
}