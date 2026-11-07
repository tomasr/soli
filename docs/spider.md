# Spider

Two decks, ten columns, and the goal of sorting everything into full suits. The
code is in `crates/soli-core/src/spider.rs`.

## Rules

- 104 cards. The difficulty sets how many suits they come from: 1 suit (eight
  sets of spades), 2 suits (four sets each of spades and hearts) or 4 suits (two
  full decks).
- 54 cards are dealt into ten columns: six cards in the first four columns and
  five in the rest, with only the top card of each face up. The other 50 cards
  stay in the stock.
- Any card can go on a card one rank higher, regardless of suit, and any card
  can go into an empty column.
- Only a run of the same suit, descending, moves as a unit. A mixed-suit run
  can be built, but has to be taken apart one card at a time.
- When a column ends in a complete King-to-Ace run of one suit, it's removed to
  a foundation automatically.
- Clicking the stock deals one card face up onto every column. That's only
  allowed when no column is empty.
- You win when all eight suits are complete.

## Implementation notes

### Two decks

With two decks, the same rank and suit can appear up to eight times. `Card`
carries a `copy` number so every card stays unique, which the front end relies
on to animate cards by identity. The deck builder (`spider_deck`) assigns copy
numbers per suit set.

### Completed runs

After every move into a column and after every deal, `collect_completed` checks
whether the column now ends in thirteen cards forming a King-to-Ace run of one
suit. If so, it moves the run to the first empty foundation, King first, so the
King is the card shown on top. This happens inside the player's move, so undo
restores the run along with the move that completed it.

### Dealing

The stock is dealt in rows of ten, and the front end fans the stock slightly by
remaining deals so you can see how many are left. When a column is empty,
clicking the stock shows "Fill every column before dealing" instead of doing
nothing.

### Double-click

Double-clicking a run sends it to the best column it can go to: one where it
continues the same suit first, then any card one rank higher, then an empty
column.

### Hints

Hints prefer moves that build in suit, then moves that turn a face-down card up,
and give a small bonus to moves that empty a column. A run already sitting on a
card of the same suit one rank higher isn't suggested again, and an empty
column is only suggested when using it turns a card up. Dealing from the stock
comes last.
