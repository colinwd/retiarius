use rand::prelude::*;

use crate::Datagram;

use super::Filter;

#[derive(Copy, Clone)]
pub struct DropChance {
    pub drop_percent: f64
}

impl Filter for DropChance {
    /// Roll between 0-1 via PRNG. If the roll comes back less than our configured drop percent,
    /// return None. Otherwise pass the packet through as normal.
    fn apply(&self, input: Datagram) -> Option<Datagram> {
        let mut rng = rand::thread_rng();
        let roll: f64 = rng.gen();

        if roll < self.drop_percent {
            None
        } else {
            Some(input)
        }
    }
}
