//! A small seeded random number generator, so a result depends only on its seed.
//!
//! SplitMix64: tiny and well distributed, which is all k-means seeding needs. It is ours
//! rather than a dependency's so that the stream never changes under us.

pub(crate) struct Rng(u64);

impl Rng {
    pub(crate) fn new(seed: u64) -> Self {
        Rng(seed)
    }

    pub(crate) fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// Uniform in `[0, 1)`.
    #[allow(clippy::cast_precision_loss, reason = "53 bits fit an f64 exactly")]
    pub(crate) fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1_u64 << 53) as f64
    }

    /// Uniform below `n`, which must be positive.
    #[allow(clippy::cast_possible_truncation, reason = "the product shifted down is below n")]
    pub(crate) fn below(&mut self, n: usize) -> usize {
        ((u128::from(self.next_u64()) * n as u128) >> 64) as usize
    }

    /// An index drawn with probability proportional to its weight. The weights must be
    /// non-negative with a positive sum.
    pub(crate) fn weighted(&mut self, weights: &[f64]) -> usize {
        let total: f64 = weights.iter().sum();
        let target = self.next_f64() * total;
        let mut sum = 0.0;
        for (i, w) in weights.iter().enumerate() {
            sum += w;
            if target < sum {
                return i;
            }
        }
        // Rounding can leave the target at the very end: take the last index with weight.
        weights.iter().rposition(|&w| w > 0.0).unwrap_or(0)
    }

    /// `count` distinct indices below `n`, in random order.
    pub(crate) fn sample(&mut self, n: usize, count: usize) -> Vec<usize> {
        let mut indices: Vec<usize> = (0..n).collect();
        let count = count.min(n);
        for i in 0..count {
            let j = i + self.below(n - i);
            indices.swap(i, j);
        }
        indices.truncate(count);
        indices
    }
}

#[cfg(test)]
mod tests {
    use super::Rng;

    #[test]
    fn same_seed_same_stream() {
        let (mut a, mut b) = (Rng::new(7), Rng::new(7));
        assert!((0..100).all(|_| a.next_u64() == b.next_u64()));
        assert_ne!(Rng::new(7).next_u64(), Rng::new(8).next_u64());
    }

    #[test]
    fn weighted_choice_skips_zero_weights() {
        let mut rng = Rng::new(1);
        assert!((0..1000).all(|_| rng.weighted(&[0.0, 2.0, 0.0, 1.0]) % 2 == 1));
    }

    #[test]
    fn sample_is_distinct() {
        let mut picked = Rng::new(3).sample(50, 20);
        picked.sort_unstable();
        picked.dedup();
        assert_eq!(picked.len(), 20);
        assert!(picked.iter().all(|&i| i < 50));
    }
}
