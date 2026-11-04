//! Card motion. Every card has a resting position given by the layout; when an
//! action moves cards, each one that changed position glides there from where
//! it was last drawn.

use macroquad::prelude::*;
use soli_core::Suit;
use std::collections::HashMap;

/// A card's identity. Each rank/suit pair is unique within a deck.
pub type CardKey = (u8, Suit);

const DURATION: f64 = 0.16;

struct Flight {
    from: Vec2,
    start: f64,
}

#[derive(Default)]
pub struct Animator {
    flights: HashMap<CardKey, Flight>,
}

impl Animator {
    /// Starts a flight for every card whose position differs between the two
    /// snapshots.
    pub fn start(
        &mut self,
        before: &HashMap<CardKey, Vec2>,
        after: &HashMap<CardKey, Vec2>,
        now: f64,
    ) {
        for (key, &to) in after {
            if let Some(&from) = before.get(key)
                && from.distance_squared(to) > 0.25
            {
                self.flights.insert(*key, Flight { from, start: now });
            }
        }
    }

    /// Drops finished flights. Returns true while anything is still moving.
    pub fn update(&mut self, now: f64) -> bool {
        self.flights.retain(|_, f| now - f.start < DURATION);
        !self.flights.is_empty()
    }

    pub fn is_flying(&self, key: CardKey) -> bool {
        self.flights.contains_key(&key)
    }

    /// Where to draw a card whose resting position is `target`.
    pub fn position(&self, key: CardKey, target: Vec2, now: f64) -> Vec2 {
        match self.flights.get(&key) {
            Some(f) => {
                let t = ((now - f.start) / DURATION).clamp(0.0, 1.0) as f32;
                let eased = 1.0 - (1.0 - t).powi(3);
                f.from.lerp(target, eased)
            }
            None => target,
        }
    }
}
