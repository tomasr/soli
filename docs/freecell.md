# FreeCell

The game where every card is visible from the start and almost every deal can
be won. The code is in `crates/soli-core/src/freecell.rs`.

## Rules

- One 52-card deck, dealt face up into eight columns (seven cards in the first
  four columns, six in the rest).
- Four free cells, each holding a single card.
- Four foundations that build up by suit from Ace to King.
- Columns build down in alternating colours, and any card can go into an empty
  column.
- Officially, only one card moves at a time. In practice, a run can move as a
  unit as long as there is enough free space to do it one card at a time (see
  below).
- You win when all 52 cards are on the foundations.

## Implementation notes

### Microsoft deal numbers

FreeCell deals use the generator from the Microsoft FreeCell that shipped with
Windows, rather than `SplitMix64`, so deal #617 in Soli is the same as deal #617
everywhere else. It's a linear congruential generator
(`state * 214013 + 2531011`, keeping bits 16 to 30) that picks cards out of a
reversed deck. Any seed maps onto deals 1 to 1,000,000, and the tests check deals
1 and 617 against their known layouts.

### Moving runs

The number of cards that can move as a unit depends on the free cells and empty
columns available as temporary space:

```
(free cells + 1) × 2^(empty columns)
```

Moving into an empty column means that column can't also be used as temporary
space, so it counts one fewer empty column. `max_run` implements this.

### Auto-move and auto-finish

Auto-move uses the same safety rule as Klondike: a card goes up to its
foundation only once nothing could ever be built on it.

Auto-finish is offered when every column is already in descending order, since
from there the cards can always go up lowest first.

### Hints

Hints are scored, best first: moves to a foundation, then moves that free a card
for a foundation, then moves that clear a column or take a card out of a free
cell, and finally ordinary building moves. An empty column is treated as
precious: it's only suggested for a run that doesn't already fill a column on
its own. Parking a card in a free cell is suggested when it uncovers a card
that can go straight up.
