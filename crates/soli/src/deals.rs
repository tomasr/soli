//! Picks the seed for each new deal. With winnable deals on, Spider deals come
//! from the solver (`soli_core::spider::winnable_seed`), which can take a
//! second or more for 4 suits. So the next winnable seed for each Spider
//! variant is found ahead of time on a background thread and kept in the
//! settings, which also covers the deal behind the chooser at startup.

use std::sync::mpsc::{self, Receiver};
use std::time::{SystemTime, UNIX_EPOCH};

use soli_core::{SpiderSuits, Variant};

use crate::persist::Settings;

pub fn random_seed() -> u64 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    nanos ^ (nanos >> 29)
}

fn solver_suits(variant: Variant) -> Option<SpiderSuits> {
    match variant {
        Variant::Spider(suits) => Some(suits),
        _ => None,
    }
}

#[derive(Default)]
pub struct Dealer {
    /// Searches in progress, at most one per variant.
    jobs: Vec<(Variant, Receiver<u64>)>,
}

impl Dealer {
    /// Starts finding the next winnable deal for `variant` if it uses the
    /// solver and one isn't already found or being found.
    pub fn prepare(&mut self, variant: Variant, settings: &Settings) {
        let Some(suits) = solver_suits(variant) else {
            return;
        };
        if !settings.winnable_deals
            || settings.winnable_seed(variant).is_some()
            || self.jobs.iter().any(|(v, _)| *v == variant)
        {
            return;
        }
        let (tx, rx) = mpsc::channel();
        let seed = random_seed();
        std::thread::spawn(move || {
            // The receiver may be gone if the app is closing; that's fine.
            let _ = tx.send(soli_core::spider::winnable_seed(seed, suits));
        });
        self.jobs.push((variant, rx));
    }

    /// Stores the seeds of finished searches in `settings`. Returns true if
    /// any were stored, so the settings need saving.
    pub fn collect(&mut self, settings: &mut Settings) -> bool {
        let mut stored = false;
        self.jobs.retain(|(variant, rx)| match rx.try_recv() {
            Ok(seed) => {
                settings.set_winnable_seed(*variant, Some(seed));
                stored = true;
                false
            }
            Err(mpsc::TryRecvError::Empty) => true,
            Err(mpsc::TryRecvError::Disconnected) => false,
        });
        stored
    }

    /// The seed for a new deal of `variant`, then starts finding the one after
    /// it. Waits for a search in progress if there's no seed ready, so the
    /// caller should save `settings` afterwards.
    pub fn next_seed(&mut self, variant: Variant, settings: &mut Settings) -> u64 {
        let Some(suits) = solver_suits(variant).filter(|_| settings.winnable_deals) else {
            return random_seed();
        };
        self.collect(settings);
        let seed = match settings.winnable_seed(variant) {
            Some(seed) => seed,
            None => match self.jobs.iter().position(|(v, _)| *v == variant) {
                Some(i) => {
                    let (_, rx) = self.jobs.remove(i);
                    rx.recv().unwrap_or_else(|_| random_seed())
                }
                None => soli_core::spider::winnable_seed(random_seed(), suits),
            },
        };
        settings.set_winnable_seed(variant, None);
        self.prepare(variant, settings);
        seed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soli_core::DrawMode;

    #[test]
    fn spider_seeds_are_used_once_and_the_next_is_prepared() {
        let mut settings = Settings::default();
        let mut dealer = Dealer::default();
        let one = Variant::Spider(SpiderSuits::One);
        settings.set_winnable_seed(one, Some(42));
        assert_eq!(dealer.next_seed(one, &mut settings), 42);
        assert_eq!(
            settings.winnable_seed(one),
            None,
            "a seed is only dealt once"
        );
        assert_eq!(dealer.jobs.len(), 1, "the next one is being found");
        let next = dealer.next_seed(one, &mut settings);
        assert_ne!(next, 42);
    }

    #[test]
    fn other_games_and_winnable_off_skip_the_solver() {
        let mut settings = Settings::default();
        let mut dealer = Dealer::default();
        dealer.next_seed(Variant::Klondike(DrawMode::One), &mut settings);
        settings.winnable_deals = false;
        dealer.next_seed(Variant::Spider(SpiderSuits::Four), &mut settings);
        assert!(dealer.jobs.is_empty());
        assert!(settings.winnable_seeds.is_empty());
    }
}
