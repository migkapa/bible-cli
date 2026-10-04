//! Small hash functions that are stable across runs, platforms, and versions,
//! for deterministic choices like the verse of the day. `std`'s hashers are
//! randomly seeded per process, so they cannot be used for this.

/// The SplitMix64 finalizer: maps nearby inputs (like consecutive day
/// numbers) to well-scattered outputs.
pub fn splitmix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

/// 64-bit FNV-1a over a byte string.
pub fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_values_never_change() {
        // Reference outputs: changing these would silently change every
        // user's verse of the day and memorization patterns.
        assert_eq!(splitmix64(0), 0xE220_A839_7B1D_CDAF);
        assert_eq!(fnv1a(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a(b"a"), 0xaf63_dc4c_8601_ec8c);
    }

    #[test]
    fn consecutive_inputs_scatter() {
        let len = 31_102u64;
        let picks: Vec<u64> = (739_000..739_030).map(|d| splitmix64(d) % len).collect();
        // No two consecutive days land on neighboring verses.
        assert!(
            picks.windows(2).all(|w| w[0].abs_diff(w[1]) > 1),
            "{:?}",
            picks
        );
    }
}
