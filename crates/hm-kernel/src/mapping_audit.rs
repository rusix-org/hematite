//! Pure executable-alias membership for a window of 64 adjacent 4 KiB pages.
#![forbid(unsafe_code)]

/// Bit `i` is set exactly when an input equals `window_base + i * 4096`.
/// Offsets are relative to the base; overflowing, preceding and unaligned
/// candidates contribute nothing. Iterator work is controlled by the caller.
#[inline]
pub fn executable_mask(window_base: u64, physical: impl Iterator<Item = u64>) -> u64 {
    let mut mask = 0;
    for address in physical {
        let Some(offset) = address.checked_sub(window_base) else {
            continue;
        };
        if offset < 64 * 4096 && offset % 4096 == 0 {
            mask |= 1u64 << (offset / 4096);
        }
    }
    mask
}

#[cfg(test)]
mod tests {
    use super::executable_mask;

    fn reference(base: u64, addresses: &[u64]) -> u64 {
        let mut expected = 0;
        for bit in 0..64 {
            if base
                .checked_add(bit * 4096)
                .is_some_and(|pa| addresses.contains(&pa))
            {
                expected |= 1u64 << bit;
            }
        }
        expected
    }

    #[test]
    fn boundaries_duplicates_and_overflow() {
        let addresses = [
            0,
            1,
            4095,
            4096,
            4096,
            63 * 4096,
            64 * 4096,
            u64::MAX - 4096,
            u64::MAX - 4095,
            u64::MAX,
        ];
        for base in [
            0,
            1,
            4095,
            4096,
            u64::MAX - 64 * 4096,
            u64::MAX - 4095,
            u64::MAX,
        ] {
            assert_eq!(
                executable_mask(base, addresses.iter().copied()),
                reference(base, &addresses)
            );
            assert_eq!(executable_mask(base, [].into_iter()), 0);
        }
    }

    #[test]
    fn every_singleton_and_full_window() {
        for base in [0, 1, 0x4000_0000, u64::MAX - 63 * 4096] {
            let addresses = core::array::from_fn::<_, 64, _>(|bit| base + bit as u64 * 4096);
            assert_eq!(executable_mask(base, addresses.into_iter()), u64::MAX);
            for (bit, address) in addresses.into_iter().enumerate() {
                assert_eq!(executable_mask(base, [address].into_iter()), 1u64 << bit);
            }
        }
    }

    #[test]
    fn generated_owner_sets_match_original_existential_lookup() {
        let mut state = 0x3276_a9b1_04fd_628du64;
        for _ in 0..32 {
            let addresses = core::array::from_fn::<_, 512, _>(|index| {
                state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                0x4000_0000 + (state % 4096) * 4096 + u64::from(index % 17 == 0)
            });
            for window in 0..64 {
                let base = 0x4000_0000 + window * 64 * 4096;
                assert_eq!(
                    executable_mask(base, addresses.iter().copied()),
                    reference(base, &addresses)
                );
            }
        }
    }
}
