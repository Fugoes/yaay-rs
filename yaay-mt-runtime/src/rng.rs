use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) fn seed_from_system_time() -> u32 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_micros() as u32
}

#[inline]
pub(crate) fn next_seed(seed: u32) -> u32 {
    let seed = seed ^ (seed << 13);
    let seed = seed ^ (seed << 17);
    let seed = seed ^ (seed << 5);
    seed
}