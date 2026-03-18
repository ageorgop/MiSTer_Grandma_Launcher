// SPDX-License-Identifier: GPL-3.0-or-later
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GamesConfig {
    pub schema: u32,
    pub games: Vec<GameEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameEntry {
    pub id: String,
    pub name: String,
    pub system: String,
    pub launch: String,
    pub art: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resolution {
    pub width: u32,
    pub height: u32,
}

fn default_resolution() -> Resolution {
    Resolution { width: 1920, height: 1080 }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub schema: u32,
    #[serde(default = "default_title")]
    pub title: String,
    #[serde(default = "default_boot_delay")]
    pub boot_delay_seconds: u32,
    #[serde(default)]
    pub admin_server: bool,
    #[serde(default = "default_admin_port")]
    pub admin_port: u16,
    #[serde(default = "default_columns")]
    pub columns: u32,
    #[serde(default = "default_resolution")]
    pub resolution: Resolution,
}

fn default_title() -> String { "GAME TIME!".to_string() }
fn default_boot_delay() -> u32 { 3 }
fn default_admin_port() -> u16 { 8080 }
fn default_columns() -> u32 { 3 }

impl Settings {
    pub fn load(path: &std::path::Path) -> Result<Self, String> {
        let data = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read settings: {}", e))?;
        let settings: Self = serde_json::from_str(&data)
            .map_err(|e| format!("Failed to parse settings: {}", e))?;
        if settings.schema != 1 {
            return Err(format!("Unsupported settings schema version: {}", settings.schema));
        }
        Ok(settings)
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            schema: 1,
            title: default_title(),
            boot_delay_seconds: default_boot_delay(),
            admin_server: false,
            admin_port: default_admin_port(),
            columns: default_columns(),
            resolution: default_resolution(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct State {
    #[serde(default)]
    pub schema: u32,
    #[serde(default)]
    pub recently_played: Vec<String>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            schema: 1,
            recently_played: Vec::new(),
        }
    }
}

impl GamesConfig {
    pub fn load(path: &std::path::Path) -> Result<Self, String> {
        let data = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read games config: {}", e))?;
        let config: Self = serde_json::from_str(&data)
            .map_err(|e| format!("Failed to parse games config: {}", e))?;
        if config.schema != 1 {
            return Err(format!("Unsupported games config schema version: {}", config.schema));
        }
        Ok(config)
    }
}

impl State {
    /// Load state from disk. Returns default if file is missing or corrupt.
    pub fn load(path: &std::path::Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|data| serde_json::from_str(&data).ok())
            .unwrap_or_default()
    }

    pub fn record_play(&mut self, game_id: &str) {
        self.recently_played.retain(|id| id != game_id);
        self.recently_played.insert(0, game_id.to_string());
        self.recently_played.truncate(5);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_games_config() {
        let json = r#"{
            "schema": 1,
            "games": [
                {
                    "id": "pacman",
                    "name": "Pac-Man",
                    "system": "arcade",
                    "launch": "/media/fat/_Arcade/Pac-Man.mra",
                    "art": "assets/boxart/pacman.png"
                }
            ]
        }"#;
        let config: GamesConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.schema, 1);
        assert_eq!(config.games.len(), 1);
        assert_eq!(config.games[0].name, "Pac-Man");
    }

    #[test]
    fn test_parse_empty_games() {
        let json = r#"{"schema": 1, "games": []}"#;
        let config: GamesConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.games.len(), 0);
    }

    #[test]
    fn test_corrupt_games_config_fails() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), "not json at all").unwrap();
        let result = GamesConfig::load(tmp.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_missing_games_config_fails() {
        let result = GamesConfig::load(std::path::Path::new("/nonexistent/games.json"));
        assert!(result.is_err());
    }

    #[test]
    fn test_state_load_missing_file_returns_default() {
        let state = State::load(std::path::Path::new("/nonexistent/state.json"));
        assert!(state.recently_played.is_empty());
    }

    #[test]
    fn test_state_load_corrupt_returns_default() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), "{{{{garbage").unwrap();
        let state = State::load(tmp.path());
        assert!(state.recently_played.is_empty());
    }

    #[test]
    fn test_record_play_adds_to_front() {
        let mut state = State::default();
        state.record_play("galaga");
        state.record_play("pacman");
        assert_eq!(state.recently_played, vec!["pacman", "galaga"]);
    }

    #[test]
    fn test_record_play_deduplicates() {
        let mut state = State::default();
        state.record_play("galaga");
        state.record_play("pacman");
        state.record_play("galaga");
        assert_eq!(state.recently_played, vec!["galaga", "pacman"]);
    }

    #[test]
    fn test_record_play_truncates_at_5() {
        let mut state = State::default();
        for i in 0..7 {
            state.record_play(&format!("game{}", i));
        }
        assert_eq!(state.recently_played.len(), 5);
        assert_eq!(state.recently_played[0], "game6");
    }

    #[test]
    fn test_settings_with_resolution() {
        let json = r#"{
            "schema": 1,
            "resolution": { "width": 1280, "height": 720 }
        }"#;
        let settings: Settings = serde_json::from_str(json).unwrap();
        assert_eq!(settings.resolution.width, 1280);
        assert_eq!(settings.resolution.height, 720);
    }

    #[test]
    fn test_settings_without_resolution_uses_default() {
        let json = r#"{"schema": 1}"#;
        let settings: Settings = serde_json::from_str(json).unwrap();
        assert_eq!(settings.resolution.width, 1920);
        assert_eq!(settings.resolution.height, 1080);
    }
}
