// SPDX-License-Identifier: GPL-3.0-or-later
use std::process::ExitCode;
use std::time::{Duration, Instant};
use std::io::{self, Read};

fn get_local_ip() -> String {
    std::fs::read_to_string("/proc/net/fib_trie")
        .ok()
        .and_then(|content| {
            for line in content.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with("|-- 192.168.")
                    || trimmed.starts_with("|-- 10.")
                {
                    return Some(trimmed.trim_start_matches("|-- ").to_string());
                }
            }
            None
        })
        .unwrap_or_else(|| "unknown".to_string())
}

fn main() -> ExitCode {
    let base = std::env::args().nth(1)
        .unwrap_or_else(|| "/media/fat/grandma_launcher".to_string());
    let paths = grandma_common::paths::GrandmaPaths::new(&base);

    let ip = get_local_ip();
    let settings = grandma_common::config::Settings::load(&paths.settings_json())
        .unwrap_or_default();
    let delay = Duration::from_secs(settings.boot_delay_seconds as u64);

    eprintln!("========================================");
    eprintln!("  Starting games...");
    eprintln!("  Press any key to escape to MiSTer menu");
    eprintln!("  Admin: http://{}:{}", ip, settings.admin_port);
    eprintln!("========================================");

    // Only poll stdin for escape if it's a real terminal.
    // When launched as a background process from user-startup.sh,
    // stdin is /dev/null and read() returns Ok(0) immediately.
    #[cfg(unix)]
    let check_stdin = {
        use std::os::unix::io::AsRawFd;
        let fd = io::stdin().as_raw_fd();
        let is_tty = unsafe { libc::isatty(fd) } == 1;
        if is_tty {
            unsafe {
                let flags = libc::fcntl(fd, libc::F_GETFL);
                libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
            }
        }
        is_tty
    };

    #[cfg(not(unix))]
    let check_stdin = false;

    let start = Instant::now();
    while start.elapsed() < delay {
        if check_stdin {
            let mut buf = [0u8; 1];
            if let Ok(n) = io::stdin().read(&mut buf) {
                if n > 0 {
                    eprintln!("Escape requested");
                    return ExitCode::from(1);
                }
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    ExitCode::SUCCESS
}
