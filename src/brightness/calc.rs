//! Pure, platform-independent brightness stepping math.

/// Default brightness change per key press, in percentage points.
pub const STEP: i8 = 10;

/// Compute the next brightness level.
///
/// `current` and the result are percentages in `0..=100`.
///
/// When `supported` is empty, the result is simply `current + delta` clamped to
/// `0..=100`.
///
/// When `supported` lists the discrete levels the panel accepts, the result
/// snaps to the next supported level in the direction of `delta`:
/// - `delta > 0` → the smallest supported level strictly greater than `current`
///   (or the highest supported level if already at/above the top).
/// - `delta < 0` → the largest supported level strictly less than `current`
///   (or the lowest supported level if already at/below the bottom).
pub fn step_level(current: u8, delta: i8, supported: &[u8]) -> u8 {
    if supported.is_empty() {
        let next = current as i16 + delta as i16;
        return next.clamp(0, 100) as u8;
    }

    let mut levels: Vec<u8> = supported.to_vec();
    levels.sort_unstable();
    levels.dedup();

    use std::cmp::Ordering;
    match delta.cmp(&0) {
        Ordering::Greater => levels
            .iter()
            .copied()
            .find(|&l| l > current)
            .unwrap_or_else(|| *levels.last().unwrap()),
        Ordering::Less => levels
            .iter()
            .copied()
            .rev()
            .find(|&l| l < current)
            .unwrap_or_else(|| *levels.first().unwrap()),
        Ordering::Equal => current.clamp(*levels.first().unwrap(), *levels.last().unwrap()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn continuous_steps_up_and_down() {
        assert_eq!(step_level(50, 10, &[]), 60);
        assert_eq!(step_level(50, -10, &[]), 40);
    }

    #[test]
    fn continuous_clamps_at_bounds() {
        assert_eq!(step_level(95, 10, &[]), 100);
        assert_eq!(step_level(3, -10, &[]), 0);
        assert_eq!(step_level(100, 10, &[]), 100);
        assert_eq!(step_level(0, -10, &[]), 0);
    }

    #[test]
    fn snaps_to_supported_levels() {
        let levels = [0u8, 25, 50, 75, 100];
        assert_eq!(step_level(50, 10, &levels), 75);
        assert_eq!(step_level(50, -10, &levels), 25);
    }

    #[test]
    fn snaps_from_off_grid_value() {
        let levels = [0u8, 25, 50, 75, 100];
        assert_eq!(step_level(55, 10, &levels), 75);
        assert_eq!(step_level(55, -10, &levels), 50);
    }

    #[test]
    fn supported_clamps_at_bounds() {
        let levels = [0u8, 25, 50, 75, 100];
        assert_eq!(step_level(100, 10, &levels), 100);
        assert_eq!(step_level(0, -10, &levels), 0);
    }

    #[test]
    fn handles_unsorted_supported_input() {
        let levels = [100u8, 0, 75, 25, 50];
        assert_eq!(step_level(40, 10, &levels), 50);
    }
}
