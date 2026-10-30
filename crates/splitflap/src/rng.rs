//! A small random number source for the board's refreshes, its lamps and
//! its sound.

/// Xorshift: nothing here needs more.
pub struct Rng(u64);

impl Rng {
    pub fn seeded() -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9E37_79B9_7F4A_7C15);
        Self(nanos | 1)
    }

    /// The same numbers every time, for tests and screenshots.
    pub fn with_seed(seed: u64) -> Self {
        Self(seed | 1)
    }

    /// A number from `lo` up to `hi`.
    pub fn between(&mut self, lo: f64, hi: f64) -> f64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        lo + (hi - lo) * ((x >> 11) as f64 / (1u64 << 53) as f64)
    }

    /// When a card sets off, in seconds from now, and how long its quick
    /// flips take as a share of the usual. Every card on a real board gets
    /// the word at once, but each starts a moment apart and runs at its own
    /// speed.
    pub fn scatter(&mut self) -> (f64, f64) {
        (self.between(0.0, 0.9), self.between(0.8, 1.25))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn random_numbers_stay_in_range() {
        let mut rng = Rng::with_seed(12345);
        let draws: Vec<f64> = (0..1000).map(|_| rng.between(10.0, 60.0)).collect();
        assert!(draws.iter().all(|&x| (10.0..60.0).contains(&x)));
        // Spread across the range, not bunched at one end.
        assert!(draws.iter().any(|&x| x < 20.0) && draws.iter().any(|&x| x > 50.0));
    }
}
