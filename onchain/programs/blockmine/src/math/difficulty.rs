#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DifficultyAdjustment {
    pub difficulty_bits: u8,
    pub target: [u8; 32],
    pub changed: bool,
}

const TARGET_SCALE_DENOMINATOR_BPS: u32 = 10_000;

pub fn target_from_difficulty_bits(bits: u8) -> [u8; 32] {
    if bits >= 255 {
        return [0u8; 32];
    }

    let mut target = [0xffu8; 32];
    let full_zero_bytes = (bits / 8) as usize;
    let remaining_bits = bits % 8;

    for byte in target.iter_mut().take(full_zero_bytes.min(32)) {
        *byte = 0;
    }

    if full_zero_bytes < 32 && remaining_bits > 0 {
        target[full_zero_bytes] = 0xffu8 >> remaining_bits;
    }

    target
}

pub fn hash_meets_target(hash: &[u8; 32], target: &[u8; 32]) -> bool {
    hash < target
}

pub fn difficulty_bits_from_target(target: &[u8; 32]) -> u8 {
    let mut bits: u16 = 0;
    for byte in target {
        if *byte == 0 {
            bits = bits.saturating_add(8);
            continue;
        }

        bits = bits.saturating_add(byte.leading_zeros() as u16);
        return bits.min(u8::MAX as u16) as u8;
    }

    u8::MAX
}

fn multiply_target_be(target: &[u8; 32], multiplier: u32) -> Vec<u8> {
    let mut product = vec![0u8; 32];
    let mut carry = 0u64;

    for index in (0..32).rev() {
        let value = (target[index] as u64)
            .saturating_mul(multiplier as u64)
            .saturating_add(carry);
        product[index] = (value & 0xff) as u8;
        carry = value >> 8;
    }

    let mut prefix = Vec::new();
    while carry > 0 {
        prefix.push((carry & 0xff) as u8);
        carry >>= 8;
    }
    prefix.reverse();
    prefix.extend(product);
    prefix
}

fn divide_target_be(bytes: &[u8], divisor: u32) -> [u8; 32] {
    let divisor = divisor.max(1) as u64;
    let mut quotient = Vec::with_capacity(bytes.len());
    let mut remainder = 0u64;

    for byte in bytes {
        let accumulator = (remainder << 8) | (*byte as u64);
        quotient.push((accumulator / divisor) as u8);
        remainder = accumulator % divisor;
    }

    let first_non_zero = quotient.iter().position(|byte| *byte != 0);
    let trimmed = match first_non_zero {
        Some(index) => &quotient[index..],
        None => &[][..],
    };

    if trimmed.len() > 32 {
        return [0xffu8; 32];
    }

    let mut out = [0u8; 32];
    if !trimmed.is_empty() {
        out[32 - trimmed.len()..].copy_from_slice(trimmed);
    }
    out
}

fn scale_target(target: &[u8; 32], numerator: u32, denominator: u32) -> [u8; 32] {
    if numerator == denominator {
        return *target;
    }

    let product = multiply_target_be(target, numerator.max(1));
    divide_target_be(&product, denominator.max(1))
}

fn clamp_target(target: [u8; 32], min_target: [u8; 32], max_target: [u8; 32]) -> [u8; 32] {
    if target < min_target {
        min_target
    } else if target > max_target {
        max_target
    } else {
        target
    }
}

pub fn calculate_next_difficulty(
    current_target: [u8; 32],
    observed_seconds: u64,
    expected_seconds: u64,
    min_bits: u8,
    max_bits: u8,
) -> DifficultyAdjustment {
    let observed = observed_seconds as u128;
    let expected = expected_seconds.max(1) as u128;
    // Per-block retarget on the full 256-bit target. Fast blocks get a more
    // aggressive reaction, slow blocks stay smoother, and extreme outliers are
    // clamped so the target converges without wild oscillation.
    let smoothed_observed = if observed <= expected {
        observed
            .checked_mul(7)
            .and_then(|value| value.checked_add(expected))
            .unwrap_or(u128::MAX)
            / 8
    } else {
        observed
            .checked_mul(3)
            .and_then(|value| value.checked_add(expected))
            .unwrap_or(u128::MAX)
            / 4
    };
    let lower_bound = (expected / 8).max(1);
    let upper_bound = expected.saturating_mul(8).max(1);
    let mut clamped_observed = smoothed_observed.clamp(lower_bound, upper_bound);

    // Emergency fast-ramp: if a farm lands on the network and blocks collapse,
    // force a much smaller target for the next block so we recover within a
    // handful of blocks instead of drifting for minutes.
    if observed.saturating_mul(100) <= expected.saturating_mul(25) {
        clamped_observed = clamped_observed.min((expected.saturating_mul(15) / 100).max(1));
    } else if observed.saturating_mul(100) <= expected.saturating_mul(50) {
        clamped_observed = clamped_observed.min((expected.saturating_mul(35) / 100).max(1));
    }

    let ratio_bps = ((clamped_observed
        .saturating_mul(TARGET_SCALE_DENOMINATOR_BPS as u128))
        / expected)
        .clamp(1, u32::MAX as u128) as u32;
    let min_target = target_from_difficulty_bits(max_bits);
    let max_target = target_from_difficulty_bits(min_bits);
    let next_target = clamp_target(
        scale_target(&current_target, ratio_bps, TARGET_SCALE_DENOMINATOR_BPS),
        min_target,
        max_target,
    );
    let next_bits = difficulty_bits_from_target(&next_target);

    DifficultyAdjustment {
        difficulty_bits: next_bits,
        target: next_target,
        changed: next_target != current_target,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_is_all_ff_at_zero_bits() {
        assert_eq!(target_from_difficulty_bits(0), [0xffu8; 32]);
    }

    #[test]
    fn target_zeroes_prefix_bits() {
        let target = target_from_difficulty_bits(12);
        assert_eq!(target[0], 0);
        assert_eq!(target[1], 0x0f);
        assert_eq!(target[2], 0xff);
    }

    #[test]
    fn difficulty_bits_round_trip_from_target() {
        let target = target_from_difficulty_bits(28);
        assert_eq!(difficulty_bits_from_target(&target), 28);
    }

    #[test]
    fn target_changes_without_needing_a_full_bit_jump() {
        let current_target = target_from_difficulty_bits(28);
        let adjustment = calculate_next_difficulty(current_target, 18, 20, 8, 40);
        assert_eq!(adjustment.difficulty_bits, 28);
        assert_ne!(adjustment.target, current_target);
        assert!(adjustment.target < current_target);
        assert!(adjustment.changed);
    }

    #[test]
    fn difficulty_increases_aggressively_when_blocks_are_extremely_fast() {
        let current_target = target_from_difficulty_bits(28);
        let adjustment = calculate_next_difficulty(current_target, 2, 20, 8, 40);
        assert!(adjustment.target < current_target);
        assert!(adjustment.difficulty_bits >= 30);
        assert!(adjustment.changed);
    }

    #[test]
    fn difficulty_decreases_when_blocks_are_slow() {
        let current_target = target_from_difficulty_bits(28);
        let adjustment = calculate_next_difficulty(current_target, 40, 20, 8, 40);
        assert!(adjustment.target > current_target);
        assert!(adjustment.difficulty_bits <= 27);
        assert!(adjustment.changed);
    }

    #[test]
    fn difficulty_stays_stable_near_target() {
        let current_target = target_from_difficulty_bits(28);
        let adjustment = calculate_next_difficulty(current_target, 20, 20, 8, 40);
        assert_eq!(adjustment.difficulty_bits, 28);
        assert_eq!(adjustment.target, current_target);
        assert!(!adjustment.changed);
    }

    #[test]
    fn difficulty_respects_bounds() {
        let current_target = target_from_difficulty_bits(40);
        let adjustment = calculate_next_difficulty(current_target, 1, 20, 12, 40);
        assert_eq!(adjustment.target, target_from_difficulty_bits(40));
        assert_eq!(adjustment.difficulty_bits, 40);
    }
}
