//! A tiny digest that is identical on every platform, for comparing the native
//! and browser builds byte for byte.

/// FNV-1a, 64 bit.
#[must_use]
pub fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// The digest as 16 hex digits.
#[must_use]
pub fn hex(bytes: &[u8]) -> String {
    format!("{:016x}", fnv1a64(bytes))
}
