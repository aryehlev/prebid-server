//! Random utilities — mirrors Go `util/randomutil` package.

use rand::Rng;

/// RandomGenerator trait for testability — mirrors Go `randomutil.RandomGenerator`.
pub trait RandomGenerator: Send + Sync {
    fn generate_int63(&self) -> i64;
    fn intn(&self, n: i32) -> i32;
}

/// Default random number generator using thread_rng.
pub struct RandomNumberGenerator;

impl RandomGenerator for RandomNumberGenerator {
    fn generate_int63(&self) -> i64 {
        let mut rng = rand::thread_rng();
        rng.gen::<i64>().abs()
    }

    fn intn(&self, n: i32) -> i32 {
        let mut rng = rand::thread_rng();
        rng.gen_range(0..n)
    }
}
