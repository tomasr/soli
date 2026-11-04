//! Rules engine for soli. Has no UI dependencies so it can be tested on its own
//! and reused by any front end.

pub mod card;
pub mod klondike;
pub mod rng;

pub use card::{Card, Color, Suit};
pub use klondike::{DrawMode, Hint, Klondike, PileId};
