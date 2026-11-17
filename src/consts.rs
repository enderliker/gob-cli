//! Embedded constants of the Babel Image Archives permutation.
//!
//! These are the exact multi-precision values used by babelia.libraryofbabel.info,
//! taken from the author's published algorithm:
//! <https://github.com/librarianofbabel/libraryofbabel.info-algo> (CC BY-SA-NC).
//! Regenerate them with `tools/gen_consts.py`.

use rug::{Integer, integer::Order};
use std::sync::OnceLock;

/// Modulus of the linear congruential generator, 3194897 bits.
const M_BYTES: &[u8] = include_bytes!("../consts/m.bin");
/// Multiplier of the LCG, 1597519 bits.
const A_BYTES: &[u8] = include_bytes!("../consts/a.bin");
/// Increment of the LCG, 1628205 bits.
const C_BYTES: &[u8] = include_bytes!("../consts/c.bin");
/// Modular inverse of `a` modulo `m`, 3194897 bits.
const AINV_BYTES: &[u8] = include_bytes!("../consts/ainverse.bin");

/// `maskone` is 2^2496009.
const MASKONE_EXP: u32 = const_exponent(include_bytes!("../consts/maskone.txt"));
/// `masktwo` is 2^1697289.
const MASKTWO_EXP: u32 = const_exponent(include_bytes!("../consts/masktwo.txt"));
/// `divver` is 2^399361, the bit width of one image quadrant.
pub const DIVVER_EXP: u32 = const_exponent(include_bytes!("../consts/divver.txt"));

/// Parses a decimal exponent at compile time.
const fn const_exponent(bytes: &[u8]) -> u32 {
    let mut v = 0u32;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b < b'0' || b > b'9' {
            break;
        }
        v = v * 10 + (b - b'0') as u32;
        i += 1;
    }
    v
}

fn parse_be(bytes: &'static [u8]) -> Integer {
    Integer::from_digits(bytes, Order::MsfBe)
}

pub struct Consts {
    pub m: Integer,
    pub a: Integer,
    pub c: Integer,
    pub ainverse: Integer,
    /// Low-bit mask `2^2496009 - 1`.
    pub maskone: Integer,
    /// Low-bit mask `2^1697289 - 1`.
    pub masktwo: Integer,
}

static CONSTS: OnceLock<Consts> = OnceLock::new();

pub fn get() -> &'static Consts {
    CONSTS.get_or_init(|| Consts {
        m: parse_be(M_BYTES),
        a: parse_be(A_BYTES),
        c: parse_be(C_BYTES),
        ainverse: parse_be(AINV_BYTES),
        // Stored as low-bit masks: `x % 2^k` == `x & (2^k - 1)`.
        maskone: (Integer::from(1) << MASKONE_EXP) - 1u32,
        masktwo: (Integer::from(1) << MASKTWO_EXP) - 1u32,
    })
}