// SPDX-License-Identifier: GPL-3.0-or-later
use grandma_common::config::GamesConfig;
use grandma_common::paths::GrandmaPaths;
use grandma_common::atomic::atomic_write_json_with_backup;
use log::{info, error};
use simplelog::*;
use tiny_http::{Server, Response, Header, Method};
use std::io::Read;
use std::process::ExitCode;

fn scan_mra_files() -> Vec<MraFile> {
    let arcade_dir = GrandmaPaths::arcade_dir();
    let mut results = Vec::new();

    if let Ok(entries) = std::fs::read_dir(&arcade_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map_or(false, |e| e == "mra") {
                let name = path.file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("Unknown")
                    .to_string();
                let id = name.to_lowercase()
                    .replace(|c: char| !c.is_alphanumeric(), "");
                let canonical = path.canonicalize().unwrap_or(path.clone());
                results.push(MraFile {
                    id,
                    name,
                    path: canonical.to_string_lossy().to_string(),
                });
            }
        }
    }

    results.sort_by(|a, b| a.name.cmp(&b.name));
    results
}

#[derive(serde::Serialize)]
struct MraFile {
    id: String,
    name: String,
    path: String,
}

fn handle_request(
    mut request: tiny_http::Request,
    paths: &GrandmaPaths,
) {
    let url = request.url().to_string();
    let method = request.method().clone();

    match (method, url.as_str()) {
        (Method::Get, "/") => {
            let html = include_str!("web/index.html");
            let header = Header::from_bytes("Content-Type", "text/html; charset=utf-8").unwrap();
            request.respond(Response::from_string(html).with_header(header)).ok();
        }

        (Method::Get, "/api/available-mra") => {
            let mras = scan_mra_files();
            let json = serde_json::to_string(&mras).unwrap_or_else(|_| "[]".into());
            let header = Header::from_bytes("Content-Type", "application/json").unwrap();
            request.respond(Response::from_string(json).with_header(header)).ok();
        }

        (Method::Get, "/api/games") => {
            let config = GamesConfig::load(&paths.games_json())
                .unwrap_or(GamesConfig { schema: 1, games: vec![] });
            let json = serde_json::to_string(&config).unwrap_or_else(|_| "{}".into());
            let header = Header::from_bytes("Content-Type", "application/json").unwrap();
            request.respond(Response::from_string(json).with_header(header)).ok();
        }

        (Method::Post, "/api/games") => {
            // Limit request body to 1MB to prevent memory exhaustion
            let content_length = request.body_length().unwrap_or(0);
            if content_length > 1_048_576 {
                request.respond(
                    Response::from_string(r#"{"error":"Request body too large"}"#)
                        .with_status_code(413)
                ).ok();
                return;
            }
            let mut body = String::new();
            if request.as_reader().take(1_048_576).read_to_string(&mut body).is_ok() {
                match serde_json::from_str::<GamesConfig>(&body) {
                    Ok(config) => {
                        if let Err(e) = grandma_common::atomic::validate_games(&config) {
                            request.respond(
                                Response::from_string(format!(r#"{{"error":"{}"}}"#, e))
                                    .with_status_code(400)
                            ).ok();
                            return;
                        }
                        match atomic_write_json_with_backup(&paths.games_json(), &config) {
                            Ok(_) => {
                                info!("Saved {} games", config.games.len());
                                let header = Header::from_bytes("Content-Type", "application/json").unwrap();
                                request.respond(
                                    Response::from_string(r#"{"ok":true}"#).with_header(header)
                                ).ok();
                            }
                            Err(e) => {
                                error!("Failed to save: {}", e);
                                request.respond(
                                    Response::from_string(format!(r#"{{"error":"{}"}}"#, e))
                                        .with_status_code(500)
                                ).ok();
                            }
                        }
                    }
                    Err(e) => {
                        request.respond(
                            Response::from_string(format!(r#"{{"error":"{}"}}"#, e))
                                .with_status_code(400)
                        ).ok();
                    }
                }
            }
        }

        (Method::Post, path) if path.starts_with("/api/art/") => {
            let game_id = &path["/api/art/".len()..];

            // Validate game_id: non-empty, lowercase alphanumeric only
            if game_id.is_empty() || game_id.len() > 64 || !game_id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()) {
                let header = Header::from_bytes("Content-Type", "application/json").unwrap();
                request.respond(
                    Response::from_string(r#"{"error":"Invalid game_id: must be non-empty lowercase alphanumeric"}"#)
                        .with_status_code(400)
                        .with_header(header)
                ).ok();
                return;
            }

            // Reject body over 2MB (when Content-Length is known).
            // For chunked requests, body_length() returns None and the
            // .take(MAX_ART_SIZE) on the reader provides a hard cap.
            const MAX_ART_SIZE: u64 = 2 * 1024 * 1024;
            if let Some(len) = request.body_length() {
                if len > MAX_ART_SIZE as usize {
                    let header = Header::from_bytes("Content-Type", "application/json").unwrap();
                    request.respond(
                        Response::from_string(r#"{"error":"Request body too large"}"#)
                            .with_status_code(413)
                            .with_header(header)
                    ).ok();
                    return;
                }
            }

            // Read body bytes
            let mut body = Vec::new();
            if let Err(e) = request.as_reader().take(MAX_ART_SIZE).read_to_end(&mut body) {
                error!("Failed to read art upload body: {}", e);
                let header = Header::from_bytes("Content-Type", "application/json").unwrap();
                request.respond(
                    Response::from_string(r#"{"error":"Failed to read request body"}"#)
                        .with_status_code(500)
                        .with_header(header)
                ).ok();
                return;
            }

            // Reject empty body (after read)
            if body.is_empty() {
                let header = Header::from_bytes("Content-Type", "application/json").unwrap();
                request.respond(
                    Response::from_string(r#"{"error":"Empty body"}"#)
                        .with_status_code(400)
                        .with_header(header)
                ).ok();
                return;
            }

            // Validate PNG magic bytes
            const PNG_MAGIC: [u8; 8] = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
            if body.len() < 8 || body[..8] != PNG_MAGIC {
                let header = Header::from_bytes("Content-Type", "application/json").unwrap();
                request.respond(
                    Response::from_string(r#"{"error":"Invalid PNG file"}"#)
                        .with_status_code(400)
                        .with_header(header)
                ).ok();
                return;
            }

            // Create boxart directory if needed
            let boxart_dir = paths.boxart_dir();
            if let Err(e) = std::fs::create_dir_all(&boxart_dir) {
                error!("Failed to create boxart directory: {}", e);
                let header = Header::from_bytes("Content-Type", "application/json").unwrap();
                request.respond(
                    Response::from_string(r#"{"error":"Failed to create boxart directory"}"#)
                        .with_status_code(500)
                        .with_header(header)
                ).ok();
                return;
            }

            // Save using atomic write
            let art_path = boxart_dir.join(format!("{}.png", game_id));
            match grandma_common::atomic::atomic_write(&art_path, &body) {
                Ok(_) => {
                    info!("Saved art for game '{}'", game_id);
                    let header = Header::from_bytes("Content-Type", "application/json").unwrap();
                    request.respond(
                        Response::from_string(r#"{"ok":true}"#).with_header(header)
                    ).ok();
                }
                Err(e) => {
                    error!("Failed to save art for '{}': {}", game_id, e);
                    let header = Header::from_bytes("Content-Type", "application/json").unwrap();
                    request.respond(
                        Response::from_string(r#"{"error":"Failed to save art"}"#)
                            .with_status_code(500)
                            .with_header(header)
                    ).ok();
                }
            }
        }

        _ => {
            request.respond(Response::from_string("404").with_status_code(404)).ok();
        }
    }
}

fn main() -> ExitCode {
    let base = std::env::args().nth(1)
        .unwrap_or_else(|| "/media/fat/grandma_launcher".to_string());
    let paths = GrandmaPaths::new(&base);

    TermLogger::init(LevelFilter::Info, Config::default(), TerminalMode::Stderr, ColorChoice::Auto).ok();

    let settings = grandma_common::config::Settings::load(&paths.settings_json())
        .unwrap_or_default();

    let addr = format!("0.0.0.0:{}", settings.admin_port);
    info!("Starting admin server at http://{}", addr);

    let server = match Server::http(&addr) {
        Ok(s) => s,
        Err(e) => {
            error!("Failed to start server: {}", e);
            return ExitCode::FAILURE;
        }
    };

    for request in server.incoming_requests() {
        handle_request(request, &paths);
    }

    ExitCode::SUCCESS
}
