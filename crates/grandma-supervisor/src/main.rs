// SPDX-License-Identifier: GPL-3.0-or-later
use grandma_common::paths::GrandmaPaths;
use file_rotate::{FileRotate, ContentLimit, suffix::AppendCount, compression::Compression};
use log::{info, error, warn};
use simplelog::*;
use std::process::{Command, ExitCode};
use std::time::{Duration, Instant};

const MAX_FAST_FAILURES: u32 = 3;
const FAST_FAILURE_THRESHOLD: Duration = Duration::from_secs(2);
const BACKOFF_SLEEP: Duration = Duration::from_secs(5);

fn init_logging(paths: &GrandmaPaths) {
    let log_file = FileRotate::new(
        paths.log_file(),
        AppendCount::new(2),
        ContentLimit::Bytes(100_000),
        Compression::None,
        #[cfg(unix)]
        None,
    );

    CombinedLogger::init(vec![
        TermLogger::new(LevelFilter::Info, Config::default(), TerminalMode::Stderr, ColorChoice::Auto),
        WriteLogger::new(LevelFilter::Info, Config::default(), log_file),
    ]).ok();
}

fn bin_path(paths: &GrandmaPaths, name: &str) -> std::path::PathBuf {
    paths.base.join("bin").join(name)
}

fn run_child(paths: &GrandmaPaths, name: &str) -> Option<i32> {
    let path = bin_path(paths, name);
    info!("Starting {}", name);
    match Command::new(&path)
        .arg(paths.base.to_str().unwrap_or("/media/fat/grandma_launcher"))
        .status()
    {
        Ok(status) => {
            let code = status.code().unwrap_or(-1);
            info!("{} exited with code {}", name, code);
            Some(code)
        }
        Err(e) => {
            error!("Failed to start {}: {}", name, e);
            None
        }
    }
}

fn main() -> ExitCode {
    let base = std::env::args().nth(1)
        .unwrap_or_else(|| "/media/fat/grandma_launcher".to_string());
    let paths = GrandmaPaths::new(&base);

    init_logging(&paths);
    info!("grandma-supervisor starting (base: {})", base);

    // Kill switch check
    if GrandmaPaths::kill_switch().exists() {
        info!("Kill switch file exists, exiting");
        return ExitCode::SUCCESS;
    }

    // Run splash
    match run_child(&paths, "grandma-splash") {
        Some(0) => info!("Splash completed, proceeding to launcher"),
        Some(_) => {
            info!("Escape requested during splash, exiting to MiSTer menu");
            return ExitCode::SUCCESS;
        }
        None => {
            error!("Splash failed to start, exiting to MiSTer menu");
            return ExitCode::FAILURE;
        }
    }

    // Blank the Linux console to prevent cursor/text bleeding through
    let _ = std::fs::write("/sys/class/graphics/fbcon/cursor_blink", "0");
    let _ = Command::new("sh").args(["-c", "echo -e '\\033[?25l' > /dev/tty0"]).status();

    // Launcher loop with crash backoff
    let mut fast_failures: u32 = 0;

    loop {
        let start = Instant::now();

        match run_child(&paths, "grandma-launcher") {
            Some(_) | None => {}
        }

        let elapsed = start.elapsed();

        if elapsed < FAST_FAILURE_THRESHOLD {
            fast_failures += 1;
            warn!("Launcher exited fast ({:?}), failure {}/{}", elapsed, fast_failures, MAX_FAST_FAILURES);

            if fast_failures >= MAX_FAST_FAILURES {
                error!("Too many fast failures, falling back to MiSTer menu");
                return ExitCode::FAILURE;
            }

            std::thread::sleep(BACKOFF_SLEEP);
        } else {
            // Normal exit (game was launched), reset counter
            fast_failures = 0;
        }

        // Brief pause before relaunch
        std::thread::sleep(Duration::from_millis(500));
    }
}
