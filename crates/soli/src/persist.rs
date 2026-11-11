//! Settings and statistics, stored as TOML in the per-user config directory
//! (`%APPDATA%\soli` on Windows, `~/Library/Application Support/soli` on macOS,
//! `$XDG_CONFIG_HOME/soli` or `~/.config/soli` elsewhere).

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use soli_core::{DrawMode, Variant};
use std::collections::BTreeMap;
use std::path::PathBuf;

fn config_dir() -> Option<PathBuf> {
    let base = if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support"))
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
    };
    base.map(|b| b.join("soli"))
}

/// Missing or unreadable files fall back to defaults; a bad settings file
/// should never stop the game from starting.
fn load<T: DeserializeOwned + Default>(name: &str) -> T {
    let Some(path) = config_dir().map(|d| d.join(name)) else {
        return T::default();
    };
    match std::fs::read_to_string(&path) {
        Ok(text) => toml::from_str(&text).unwrap_or_else(|e| {
            eprintln!("ignoring {}: {e}", path.display());
            T::default()
        }),
        Err(_) => T::default(),
    }
}

fn save<T: Serialize>(name: &str, value: &T) {
    let Some(dir) = config_dir() else { return };
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        std::fs::create_dir_all(&dir)?;
        let text = toml::to_string_pretty(value)?;
        // Write then rename, so a crash mid-write can't leave a truncated file.
        let tmp = dir.join(format!("{name}.tmp"));
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, dir.join(name))?;
        Ok(())
    })();
    if let Err(e) = result {
        eprintln!("could not save {name}: {e}");
    }
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Settings {
    /// Variant key (see [`Variant::key`]).
    pub variant: Option<String>,
    /// Pre-variant setting, still read so older settings files keep their
    /// draw mode.
    #[serde(skip_serializing)]
    pub draw_three: bool,
    pub auto_move: bool,
    pub card_back: usize,
    pub table: usize,
    /// Last window size in physical pixels.
    pub window: Option<(u32, u32)>,
    /// Spider deals only come from seeds the solver can win.
    pub winnable_deals: bool,
    /// The next winnable seed for each variant that has one ready, by variant
    /// key. Strings, because TOML integers stop at `i64::MAX`.
    pub winnable_seeds: BTreeMap<String, String>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            variant: None,
            draw_three: false,
            auto_move: true,
            card_back: 0,
            table: 0,
            window: None,
            winnable_deals: true,
            winnable_seeds: BTreeMap::new(),
        }
    }
}

impl Settings {
    const FILE: &str = "settings.toml";

    pub fn load() -> Self {
        load(Self::FILE)
    }

    pub fn save(&self) {
        save(Self::FILE, self)
    }

    pub fn variant(&self) -> Variant {
        self.variant
            .as_deref()
            .and_then(Variant::from_key)
            .unwrap_or(if self.draw_three {
                Variant::Klondike(DrawMode::Three)
            } else {
                Variant::Klondike(DrawMode::One)
            })
    }

    pub fn set_variant(&mut self, variant: Variant) {
        self.variant = Some(variant.key().to_owned());
    }

    pub fn winnable_seed(&self, variant: Variant) -> Option<u64> {
        self.winnable_seeds.get(variant.key())?.parse().ok()
    }

    pub fn set_winnable_seed(&mut self, variant: Variant, seed: Option<u64>) {
        let key = variant.key().to_owned();
        match seed {
            Some(seed) => self.winnable_seeds.insert(key, seed.to_string()),
            None => self.winnable_seeds.remove(&key),
        };
    }
}

#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct ModeStats {
    pub played: u32,
    pub won: u32,
    pub current_streak: u32,
    pub best_streak: u32,
    pub best_time_secs: Option<u32>,
    pub fewest_moves: Option<u32>,
}

impl ModeStats {
    pub fn record_win(&mut self, secs: u32, moves: u32) {
        self.played += 1;
        self.won += 1;
        self.current_streak += 1;
        self.best_streak = self.best_streak.max(self.current_streak);
        self.best_time_secs = Some(self.best_time_secs.map_or(secs, |b| b.min(secs)));
        self.fewest_moves = Some(self.fewest_moves.map_or(moves, |b| b.min(moves)));
    }

    pub fn record_loss(&mut self) {
        self.played += 1;
        self.current_streak = 0;
    }

    pub fn win_rate(&self) -> Option<f32> {
        (self.played > 0).then(|| self.won as f32 / self.played as f32)
    }
}

/// Statistics per variant, keyed by [`Variant::key`] so new variants can be
/// added without changing the file format.
#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Stats {
    pub modes: BTreeMap<String, ModeStats>,
}

impl Stats {
    const FILE: &str = "stats.toml";

    pub fn load() -> Self {
        load(Self::FILE)
    }

    pub fn save(&self) {
        save(Self::FILE, self)
    }

    pub fn mode(&self, variant: Variant) -> ModeStats {
        self.modes.get(variant.key()).cloned().unwrap_or_default()
    }

    pub fn mode_mut(&mut self, variant: Variant) -> &mut ModeStats {
        self.modes.entry(variant.key().to_owned()).or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streaks_and_bests() {
        let mut s = ModeStats::default();
        s.record_win(300, 120);
        s.record_win(250, 140);
        s.record_loss();
        s.record_win(400, 100);
        assert_eq!(s.played, 4);
        assert_eq!(s.won, 3);
        assert_eq!(s.current_streak, 1);
        assert_eq!(s.best_streak, 2);
        assert_eq!(s.best_time_secs, Some(250));
        assert_eq!(s.fewest_moves, Some(100));
    }

    #[test]
    fn stats_round_trip_through_toml() {
        let mut stats = Stats::default();
        let three = Variant::Klondike(DrawMode::Three);
        stats.mode_mut(three).record_win(90, 80);
        let text = toml::to_string_pretty(&stats).unwrap();
        let back: Stats = toml::from_str(&text).unwrap();
        assert_eq!(back.mode(three), stats.mode(three));
        assert_eq!(back.mode(Variant::FreeCell), ModeStats::default());
    }

    #[test]
    fn legacy_draw_three_setting_picks_variant() {
        let s: Settings = toml::from_str(
            "draw_three = true
",
        )
        .unwrap();
        assert_eq!(s.variant(), Variant::Klondike(DrawMode::Three));
        let s: Settings = toml::from_str(
            "draw_three = true
variant = \"freecell\"
",
        )
        .unwrap();
        assert_eq!(s.variant(), Variant::FreeCell);
        assert!(!toml::to_string(&s).unwrap().contains("draw_three"));
    }

    #[test]
    fn unknown_and_missing_settings_fields_are_tolerated() {
        let s: Settings = toml::from_str("table = 2\nfuture_option = true\n").unwrap();
        assert_eq!(s.table, 2);
        assert!(s.auto_move);
    }
}
