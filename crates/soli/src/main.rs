// Release builds are GUI apps with no console window. Debug builds keep the
// console so panics and debug output stay visible.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod anim;
mod app;
mod art;
mod idle;
mod layout;
mod persist;
mod render;
mod ui;

use app::App;
use layout::Layout;
use macroquad::miniquad::conf::Platform;
use macroquad::prelude::*;
use persist::{Settings, Stats};
use soli_core::{Card, DrawMode, Suit, Variant};
use std::time::{Duration, Instant};

/// Frame pacing. VSync is off (DWM composites windowed apps, so there is no
/// tearing) because a vsynced OpenGL swap chain queues frames and makes dragged
/// cards trail the cursor. Instead we cap the frame rate ourselves: high while
/// something moves, lower when the table is static.
const ACTIVE_FRAME: Duration = Duration::from_micros(1_000_000 / 165);
const IDLE_FRAME: Duration = Duration::from_micros(1_000_000 / 60);

/// Initial window size in logical pixels, used until a size has been saved.
/// The window is created in physical pixels before the display scale is
/// known, so main() grows it to match.
const WINDOW_SIZE: (f32, f32) = (1100.0, 760.0);

enum Mode {
    Play,
    /// Render a fixed deal to a PNG and exit. `--variant <key>` picks the
    /// game, `--seed <n>` the deal; `--show help|stats|games|hint` adds an
    /// overlay.
    Screenshot(String),
    /// Render every card face and back to a PNG and exit.
    Gallery(String),
}

fn mode_from_args() -> Mode {
    let args: Vec<String> = std::env::args().collect();
    let value_of = |flag: &str| {
        args.iter()
            .position(|a| a == flag)
            .and_then(|i| args.get(i + 1).cloned())
    };
    if let Some(path) = value_of("--screenshot") {
        Mode::Screenshot(path)
    } else if let Some(path) = value_of("--gallery") {
        Mode::Gallery(path)
    } else {
        Mode::Play
    }
}

fn saved_window() -> Option<(u32, u32)> {
    match mode_from_args() {
        Mode::Play => Settings::load()
            .window
            .filter(|&(w, h)| w >= 400 && h >= 300),
        _ => None,
    }
}

fn window_conf() -> Conf {
    let (w, h) = saved_window().unwrap_or((WINDOW_SIZE.0 as u32, WINDOW_SIZE.1 as u32));
    Conf {
        window_title: "Soli".to_owned(),
        window_width: w as i32,
        window_height: h as i32,
        high_dpi: true,
        sample_count: 4,
        platform: Platform {
            swap_interval: Some(0),
            ..Default::default()
        },
        ..Default::default()
    }
}

/// All 52 faces plus every card back, for checking card art.
fn draw_gallery() {
    let (sw, sh) = (screen_width(), screen_height());
    let cols = 13.0;
    let gap = 8.0;
    let w = ((sw - gap * (cols + 1.0)) / cols).min((sh - gap * 6.0) / 5.0 / 1.4);
    let h = w * 1.4;
    render::prepare_cards(vec2(w, h), false);
    clear_background(render::TABLES[0].felt);
    for (row, suit) in Suit::ALL.iter().enumerate() {
        for rank in 1..=13u8 {
            let card = Card::new(rank, *suit).turned_up();
            let x = gap + (rank - 1) as f32 * (w + gap);
            let y = gap + row as f32 * (h + gap);
            render::draw_card(card, Rect::new(x, y, w, h), &render::BACKS[0]);
        }
    }
    for (i, back) in render::BACKS.iter().enumerate() {
        let card = Card::new(1, Suit::Spades);
        let x = gap + i as f32 * (w + gap);
        let y = gap + 4.0 * (h + gap);
        render::draw_card(card, Rect::new(x, y, w, h), back);
    }
}

#[macroquad::main(window_conf)]
async fn main() {
    render::init_font();
    let mode = mode_from_args();

    if saved_window().is_none() {
        let dpi = screen_dpi_scale();
        if dpi > 1.0 {
            request_new_screen_size(WINDOW_SIZE.0 * dpi, WINDOW_SIZE.1 * dpi);
        }
    }

    let mut app = match mode {
        Mode::Play => {
            let mut app = App::new(Settings::load(), Stats::load(), app::random_seed(), true);
            app.open_game_chooser();
            app
        }
        _ => {
            let args: Vec<String> = std::env::args().collect();
            let value_of = |flag: &str| {
                let i = args.iter().position(|a| a == flag)?;
                args.get(i + 1).map(|s| s.as_str())
            };
            let variant = value_of("--variant")
                .and_then(Variant::from_key)
                .unwrap_or(Variant::Klondike(DrawMode::Three));
            let mut settings = Settings {
                auto_move: false,
                ..Default::default()
            };
            settings.set_variant(variant);
            let seed = value_of("--seed")
                .and_then(|s| s.parse().ok())
                .unwrap_or(12345);
            let mut app = App::new(settings, Stats::default(), seed, false);
            // A few stock clicks so the waste (or a Spider deal) shows.
            let clicks = match variant {
                Variant::Klondike(_) | Variant::Cairn(_) => 3,
                _ => 0,
            };
            for _ in 0..clicks {
                app.game.click_stock();
            }
            if let Some(i) = args.iter().position(|a| a == "--show") {
                app.show_overlay(args.get(i + 1).map_or("", |s| s.as_str()));
            }
            app
        }
    };

    // Closing the window goes through our loop so the round and window size
    // can be saved first.
    prevent_quit();

    // `--bench <frames>` (with --screenshot or --gallery): instead of saving
    // a PNG, draw that many frames flat out and print the average frame time,
    // for comparing rendering changes.
    let bench: Option<u32> = {
        let args: Vec<String> = std::env::args().collect();
        args.iter()
            .position(|a| a == "--bench")
            .and_then(|i| args.get(i + 1))
            .and_then(|s| s.parse().ok())
    };
    let bench_start = Instant::now();

    let mut frames = 0;
    // Whether the frame just presented showed the table at rest, so the next
    // one can wait for input instead of drawing the same picture again.
    let mut at_rest = false;
    loop {
        // An idle table is only redrawn when something could have changed it:
        // input or another window message, or the clock ticking over. Waiting
        // here, after the last frame was presented, rather than before
        // presenting it, keeps that frame on screen while we wait. Input that
        // wakes us is read by the next frame, so this one draws the same
        // picture again and skips its frame budget.
        let woke_for_input = at_rest && idle::wait_for_input(app.next_tick());
        let frame_start = Instant::now();
        let layout = Layout::compute(screen_width(), screen_height(), app.game.as_ref());
        match &mode {
            Mode::Play => {
                app.update(&layout);
                app.draw(&layout);
            }
            Mode::Screenshot(_) => app.draw(&layout),
            Mode::Gallery(_) => draw_gallery(),
        }

        if let Some(n) = bench {
            frames += 1;
            if frames == n {
                let ms = bench_start.elapsed().as_secs_f64() * 1000.0 / n as f64;
                println!("{n} frames, {ms:.3} ms/frame");
                break;
            }
            next_frame().await;
            continue;
        }

        if let Mode::Screenshot(path) | Mode::Gallery(path) = &mode {
            frames += 1;
            if frames == 3 {
                // The framebuffer's alpha channel isn't shown on screen, but
                // translucent drawing lowers it; make the PNG opaque to match.
                let mut image = get_screen_data();
                for px in image.bytes.chunks_exact_mut(4) {
                    px[3] = 255;
                }
                image.export_png(path);
                break;
            }
        }

        if is_quit_requested() {
            if let Mode::Play = mode {
                app.finish_round();
                let dpi = screen_dpi_scale();
                let size = (screen_width() * dpi, screen_height() * dpi);
                app.settings.window = Some((size.0.round() as u32, size.1.round() as u32));
                app.settings.save();
            }
            break;
        }

        // Sleep before presenting, so input is read as late as possible after.
        let budget = if woke_for_input {
            Duration::ZERO
        } else if app.busy() {
            ACTIVE_FRAME
        } else {
            IDLE_FRAME
        };
        if let Some(rest) = budget.checked_sub(frame_start.elapsed()) {
            std::thread::sleep(rest);
        }
        // After waking for input, the next frame has to draw what it did.
        at_rest = matches!(mode, Mode::Play) && !woke_for_input && !app.busy();
        next_frame().await;
    }
}
