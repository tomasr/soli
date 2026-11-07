# Klondike

The classic patience game, and the first one Soli had. The code is in
`crates/soli-core/src/klondike.rs`.

## Rules

- One 52-card deck. Seven tableau columns are dealt with 1 to 7 cards, the top
  card of each face up. The remaining 24 cards form the stock.
- Tableau columns build down in alternating colours. Any face-up run that
  follows that rule moves as a unit, and only a King (or a run starting with
  one) can go into an empty column.
- The four foundations build up by suit, from Ace to King. A card can also come
  back down from a foundation onto the tableau.
- Clicking the stock turns over one card (Draw 1) or three (Draw 3) onto the
  waste. Only the top waste card plays. When the stock runs out, clicking it
  turns the waste back over, as many times as you like.
- When a move leaves a face-down card on top of a column, it turns face up.
- You win when all 52 cards are on the foundations.

## Implementation notes

### Draw 3

In Draw 3, the last three cards drawn are fanned out on the waste. The board
tracks how many cards of the latest draw are still showing (`waste_visible`),
and playing the top waste card shrinks the fan by one, but never below one card.
The front end reads this through `Game::waste_fan()` to lay out the fan.

### Auto-move

With auto-move on (`M`), cards go up to the foundations by themselves, but only
when it's safe: when the card can never be needed on the tableau again. A card
is safe if it's an Ace or a 2, or if both foundations of the opposite colour
are already up to one rank below it (nothing could ever be built on it). This
check lives in `game::rules::is_safe_for_foundation`, shared with FreeCell.

Automatic moves are recorded in the undo history as automatic, so undo takes
back the player's move together with whatever auto-move did after it.

### Auto-finish

Once the stock and waste are empty and every tableau card is face up, the game
can be finished mechanically and the status bar offers `A`. Each step moves the
lowest-ranked card that fits a foundation, so the animation looks like cards
being put away in order rather than in column order.

### Hints

Hints are scored, best first:

1. A move to a foundation (more if it turns a card up).
2. A tableau move that turns a face-down card up.
3. Splitting a run so the card left behind can go to a foundation.
4. The waste card onto the tableau.
5. A run that empties its column, but only onto a real card.

Moves that just shift a run from one column to an equivalent spot, revealing
nothing, are left out. Clicking the stock is always last, while there's
anything to draw or recycle.

### Deals

Deals come from shuffling with `SplitMix64` and the game's seed. The status bar
shows the seed as a six-digit deal number, and `R` restarts the same deal.
