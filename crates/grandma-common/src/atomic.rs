// SPDX-License-Identifier: GPL-3.0-or-later
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

/// Atomic file write: write to .tmp, fsync, rename over target.
pub fn atomic_write(path: &Path, data: &[u8]) -> Result<(), String> {
    let tmp_path = path.with_file_name(format!(
        "{}.tmp",
        path.file_name().unwrap_or_default().to_string_lossy()
    ));

    let mut file = File::create(&tmp_path)
        .map_err(|e| format!("Failed to create temp file {:?}: {}", tmp_path, e))?;

    file.write_all(data)
        .map_err(|e| format!("Failed to write temp file {:?}: {}", tmp_path, e))?;

    file.sync_all()
        .map_err(|e| format!("Failed to fsync temp file {:?}: {}", tmp_path, e))?;

    fs::rename(&tmp_path, path)
        .map_err(|e| format!("Failed to rename {:?} -> {:?}: {}", tmp_path, path, e))?;

    Ok(())
}

/// Validate a games config before saving.
/// Checks launch paths exist, extensions are valid, and IDs are unique.
pub fn validate_games(config: &crate::config::GamesConfig) -> Result<(), String> {
    let mut seen_ids = std::collections::HashSet::new();
    for game in &config.games {
        if !seen_ids.insert(&game.id) {
            return Err(format!("Duplicate game ID: {}", game.id));
        }
        let path = std::path::Path::new(&game.launch);
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if ext != "mra" && ext != "mgl" {
            return Err(format!("Invalid launch file extension for {}: {}", game.id, game.launch));
        }
        if !path.exists() {
            return Err(format!("Launch file not found for {}: {}", game.id, game.launch));
        }
    }
    Ok(())
}

/// Atomic JSON write with backup of previous file.
pub fn atomic_write_json_with_backup<T: serde::Serialize>(
    path: &Path,
    data: &T,
) -> Result<(), String> {
    if path.exists() {
        let bak = path.with_extension("json.bak");
        fs::copy(path, &bak)
            .map_err(|e| format!("Failed to backup {:?}: {}", path, e))?;
    }

    let json = serde_json::to_string_pretty(data)
        .map_err(|e| format!("Failed to serialize: {}", e))?;

    atomic_write(path, json.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_atomic_write_creates_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.json");
        atomic_write(&path, b"hello").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "hello");
    }

    #[test]
    fn test_atomic_write_no_tmp_left_behind() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.json");
        atomic_write(&path, b"hello").unwrap();
        assert!(!dir.path().join("test.json.tmp").exists());
    }

    #[test]
    fn test_atomic_write_json_with_backup() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("games.json");
        fs::write(&path, "old").unwrap();

        atomic_write_json_with_backup(&path, &serde_json::json!({"new": true})).unwrap();

        assert!(path.with_extension("json.bak").exists());
        assert_eq!(fs::read_to_string(path.with_extension("json.bak")).unwrap(), "old");
        let new_content = fs::read_to_string(&path).unwrap();
        assert!(new_content.contains("\"new\""));
    }
}
