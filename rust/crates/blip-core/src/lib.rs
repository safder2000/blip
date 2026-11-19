//! Blip's model, shared by everything that isn't drawing: settings, the pet's needs and care,
//! timers and alarms, notes and reminders, the inbox. It reads and writes the same JSON files as
//! the Python Blip (`config.json`, `state.json`, `timers.json`, `notes.json`, `inbox.json`), so
//! switching keeps everyone's pet, timers and notes.

pub mod clock;
pub mod config;
pub mod cry;
pub mod notes;
pub mod pet;
pub mod store;
pub mod timers;

/// Seconds since the Unix epoch, like Python's time.time().
pub fn now() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}

/// A small random generator (xorshift64*), seeded from the system.
#[derive(Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new() -> Rng {
        let mut b = [0u8; 8];
        let _ = getrandom::getrandom(&mut b);
        Rng(u64::from_le_bytes(b) | 1)
    }
    pub fn seeded(seed: u64) -> Rng {
        Rng(seed | 1)
    }
    /// Uniform in [0, 1).
    pub fn f(&mut self) -> f64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        (self.0.wrapping_mul(0x2545F4914F6CDD1D) >> 11) as f64 / (1u64 << 53) as f64
    }
    pub fn range(&mut self, a: f64, b: f64) -> f64 {
        a + (b - a) * self.f()
    }
    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[((self.f() * items.len() as f64) as usize).min(items.len() - 1)]
    }
    pub fn hex(&mut self, n: usize) -> String {
        (0..n).map(|_| format!("{:x}", (self.f() * 16.0) as u32 & 15)).collect()
    }
}

impl Default for Rng {
    fn default() -> Self {
        Rng::new()
    }
}
