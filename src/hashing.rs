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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_values_never_change() {
        // A reference output: changing it would silently change every user's
        // verse of the day.
        assert_eq!(splitmix64(0), 0xE220_A839_7B1D_CDAF);
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
