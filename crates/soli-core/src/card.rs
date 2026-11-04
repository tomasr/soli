#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Suit {
    Clubs,
    Diamonds,
    Hearts,
    Spades,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Color {
    Red,
    Black,
}

impl Suit {
    pub const ALL: [Suit; 4] = [Suit::Clubs, Suit::Diamonds, Suit::Hearts, Suit::Spades];

    pub fn color(self) -> Color {
        match self {
            Suit::Diamonds | Suit::Hearts => Color::Red,
            Suit::Clubs | Suit::Spades => Color::Black,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Card {
    /// 1 = Ace ... 13 = King
    pub rank: u8,
    pub suit: Suit,
    pub face_up: bool,
    /// Distinguishes identical cards in multi-deck games (Spider). Always 0
    /// for single-deck games.
    pub copy: u8,
}

impl Card {
    pub fn new(rank: u8, suit: Suit) -> Self {
        debug_assert!((1..=13).contains(&rank));
        Card {
            rank,
            suit,
            face_up: false,
            copy: 0,
        }
    }

    pub fn turned_up(mut self) -> Self {
        self.face_up = true;
        self
    }

    /// The card cannot be confused with any other card in play.
    pub fn id(self) -> (u8, Suit, u8) {
        (self.rank, self.suit, self.copy)
    }

    pub fn color(self) -> Color {
        self.suit.color()
    }

    pub fn rank_label(self) -> &'static str {
        const LABELS: [&str; 13] = [
            "A", "2", "3", "4", "5", "6", "7", "8", "9", "10", "J", "Q", "K",
        ];
        LABELS[(self.rank - 1) as usize]
    }

    /// Ordered full deck, all cards face down.
    pub fn deck() -> Vec<Card> {
        Suit::ALL
            .iter()
            .flat_map(|&s| (1..=13).map(move |r| Card::new(r, s)))
            .collect()
    }
}
