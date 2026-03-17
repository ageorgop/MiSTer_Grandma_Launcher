// SPDX-License-Identifier: GPL-3.0-or-later
use std::path::PathBuf;

pub struct GrandmaPaths {
    pub base: PathBuf,
}

impl GrandmaPaths {
    pub fn new(base: impl Into<PathBuf>) -> Self {
        Self { base: base.into() }
    }

    /// Standard MiSTer deployment paths
    pub fn mister() -> Self {
        Self::new("/media/fat/grandma_launcher")
    }

    pub fn games_json(&self) -> PathBuf { self.base.join("games.json") }
    pub fn games_json_bak(&self) -> PathBuf { self.base.join("games.json.bak") }
    pub fn settings_json(&self) -> PathBuf { self.base.join("settings.json") }
    pub fn state_json(&self) -> PathBuf { self.base.join("state.json") }
    pub fn log_file(&self) -> PathBuf { self.base.join("grandma.log") }
    pub fn boxart_dir(&self) -> PathBuf { self.base.join("assets/boxart") }
    pub fn placeholder_art(&self) -> PathBuf { self.base.join("assets/placeholder.png") }
    pub fn font_file(&self) -> PathBuf { self.base.join("assets/font.ttf") }
    pub fn kill_switch() -> PathBuf { PathBuf::from("/media/fat/grandma_launcher.disabled") }
    pub fn mister_cmd() -> PathBuf { PathBuf::from("/dev/MiSTer_cmd") }
    pub fn arcade_dir() -> PathBuf { PathBuf::from("/media/fat/_Arcade") }
}
