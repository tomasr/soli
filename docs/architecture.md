# Architecture

Soli is a Cargo workspace with two crates:

- `crates/soli-core`: the rules of every game. It has no dependencies at all,
  which keeps the rules easy to test and means a different front end could
  drive the same games.
- `crates/soli`: the desktop front end, built on macroquad. It owns layout,
  drawing, animation, input, settings and statistics.

The front end never knows the rules of a particular game. It talks to every
game through one trait, and only asks things like "can these cards move there?"
or "where should this card go if it's double-clicked?".

## The core crate

### Cards and piles

A `Card` is a rank (1 to 13), a suit, whether it's face up, and a `copy` number
that tells apart identical cards in two-deck games (Spider and Cairn). The
triple `(rank, suit, copy)` is unique within a game, and the front end uses it
as the key for animations.

Every place a card can be is a `PileId`:

| Pile | Used by |
|---|---|
| `Stock`, `Waste` | Klondike, Spider (stock only), Cairn |
| `Foundation(i)` | Klondike, Spider, FreeCell, and Cairn's matched pile |
| `Tableau(i)` | Klondike, Spider, FreeCell |
| `Cell(i)` | FreeCell's free cells, Cairn's reserve |
| `Slot(i)` | Cairn's layout, one card per slot |

A pile is always exposed as a slice of cards, bottom card first. That holds even
for Cairn's layout slots, which hold zero or one card.

### The `Game` trait

`Game` (in `game.rs`) is what every variant implements. The main methods are:

- `piles()` and `pile(id)`: what's on the table.
- `can_pick_up(from, count)` and `can_move(from, count, to)`: rules checks,
  which the front end uses for dragging and for highlighting drop targets.
- `move_cards(from, count, to)`: the player's move. It returns `false` and
  changes nothing if the move is illegal.
- `click_stock()` and `stock_blocked()`: drawing, recycling or dealing, and a
  short reason to show when clicking the stock does nothing.
- `undo()`, `hints()`, `auto_target()` for double-click, `auto_move_step()`
  for safe moves to the foundations, and `can_autocomplete()` with
  `autocomplete_step()` for finishing a game mechanically.

Most methods have defaults that do nothing, so a variant only implements what
applies to it. A few exist just for Cairn: `slots()` returns the layout
positions, `is_covered()` tells the front end which cards to shade, and
`refusal()` and `blocked_reason()` give a short explanation when a move that
looks possible isn't allowed.

### Variants and families

`Variant` names every playable combination (`Klondike(DrawMode)`,
`Spider(SpiderSuits)`, `FreeCell`, `Cairn(CairnLevel)`) and creates games with
`Variant::new_game(seed)`. Each variant has a stable `key()` used in the
settings and statistics files, so variants can be added without changing the
file format.

`Variant::FAMILIES` groups variants into the games shown in the chooser, with
a default variant for each.

### Undo

Every game keeps its changing state in a `Board` struct that is small enough to
clone, and `History<Board>` stores a snapshot before every change. Each
snapshot is marked as either a player action or an automatic move (auto-move or
auto-finish), and undo rolls back past the automatic moves to the last player
action. That way, a single undo never leaves the player looking at a board the
game is about to change again.

### Deals

Shuffling uses `SplitMix64` (`rng.rs`) rather than the `rand` crate, so a seed
always produces the same deal across versions. The status bar shows a deal
number, and restarting (`R`) replays the same seed. FreeCell is the exception:
it uses the Microsoft deal generator so its numbers match the classic Windows
game. Cairn builds its deals around a solution so every deal can be won (see
[cairn.md](cairn.md)).

### Hints

`hints()` returns useful moves, best first. Each game scores candidate moves
with its own heuristics: moves to a foundation first, then moves that turn a
card up or free one, then building moves. Moves that only shuffle cards around
are left out. Pressing `H` again cycles through the list.

## The front end crate

The front end is a single-threaded macroquad loop in `main.rs` that computes a
`Layout`, then calls `App::update` and `App::draw` every frame.

| Module | Does |
|---|---|
| `app.rs` | Game state, input (drag, click, double-click, keys), overlays, statistics bookkeeping, drawing the table |
| `layout.rs` | Screen geometry, recomputed every frame so the table follows window resizes |
| `render.rs` | Card faces, backs and suits drawn from primitives, the palette, text |
| `art.rs` | The line-art animals, stored as a subset of SVG path data and flattened once |
| `anim.rs` | Card motion |
| `ui.rs` | Panels, keycaps, toasts |
| `persist.rs` | Settings and statistics as TOML |

### Layout

Column-based games (Klondike, Spider, FreeCell) get their card size from the
number of columns and the window height, with tableau columns squeezed
vertically when they would run off the screen. Cairn's layout is different:
slots are positioned in half-card units, so `Layout` fits the layout's bounding
box to the window and places the stock, waste, reserve and matched pile in a
row underneath.

### Animation

Every card has a resting position given by the layout. Before an action, the
app records where each card is currently drawn; after it, where each card
should rest. Any card whose position changed gets a short flight (0.16 seconds,
eased) from the old position to the new one. Since cards are keyed by
`(rank, suit, copy)`, this works the same for moves, undo, dealing and
auto-finish, with no per-action animation code.

### Input

Dragging picks up a card or run where it is currently drawn, even mid-flight,
and the drop target is the legal pile the dragged cards overlap the most.
Double-click sends a card to `auto_target()`. In Cairn, a press that doesn't
move counts as a click, which selects a card or pairs it with the selected one.

Hit testing walks the piles in reverse drawing order, so the topmost card wins
when cards overlap.

### Frame pacing

VSync is off, because a vsynced OpenGL swap chain queues frames and makes a
dragged card trail behind the cursor. Instead, the loop caps the frame rate
itself: 165 fps while anything is moving or the mouse button is down, and 60
fps when the table is still. The loop sleeps before presenting, so input is
read as late as possible.

### Settings and statistics

Settings (current variant, table colour, card back, auto-move, window size) and
statistics (played, won, streaks, best time, fewest moves per variant) are
TOML files written atomically (write to a temporary file, then rename). A
missing or broken file falls back to defaults rather than stopping the game
from starting. Abandoning a game you've made a move in counts as a loss.
