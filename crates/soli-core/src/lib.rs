//! Rules engine for soli. Has no UI dependencies so it can be tested on its own
//! and reused by any front end.

pub mod card;
pub mod freecell;
pub mod game;
pub mod klondike;
pub mod rng;
pub mod spider;

pub use card::{Card, Color, Suit};
pub use game::{DrawMode, Game, Hint, PileId, SpiderSuits, Variant};
