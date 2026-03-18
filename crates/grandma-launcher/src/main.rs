// SPDX-License-Identifier: GPL-3.0-or-later
mod framebuf;
mod input;
mod render;

use grandma_common::config::{GamesConfig, State};
use grandma_common::paths::GrandmaPaths;
use grandma_common::atomic::atomic_write;
use input::{Action, InputState};
use render::{ArtCache, GridState};
use log::{info, error, warn};
use simplelog::*;
use std::process::ExitCode;

fn init_logging() {
    TermLogger::init(LevelFilter::Info, Config::default(), TerminalMode::Stderr, ColorChoice::Auto).ok();
}

fn load_art_cache(games: &[grandma_common::config::GameEntry], paths: &GrandmaPaths) -> ArtCache {
    let mut cache = ArtCache::new();

    for game in games {
        let art_path = if game.art.starts_with('/') {
            std::path::PathBuf::from(&game.art)
        } else {
            paths.base.join(&game.art)
        };

        if !art_path.exists() {
            warn!("Art not found for {}: {:?}", game.id, art_path);
            continue;
        }

        match image::open(&art_path) {
            Ok(img) => {
                let resized = img.resize(
                    render::TILE_WIDTH,
                    render::ART_HEIGHT,
                    image::imageops::FilterType::Triangle,
                );
                let rgba = resized.to_rgba8();
                cache.insert(
                    game.id.clone(),
                    (rgba.width(), rgba.height(), rgba.into_raw()),
                );
                info!("Loaded art for {}", game.id);
            }
            Err(e) => {
                warn!("Failed to load art for {}: {}", game.id, e);
            }
        }
    }

    cache
}

/// Enable the FPGA framebuffer display by sending F9 via a uinput virtual keyboard.
/// MiSTer's menu code handles F9 by calling video_fb_enable() which sends the SPI
/// commands to the FPGA. Synthetic keypresses on existing devices are blocked by
/// EVIOCGRAB, so we create a new uinput device that MiSTer discovers via inotify.
#[cfg(target_os = "linux")]
fn enable_fpga_framebuffer() -> Result<(), String> {
    const UI_SET_EVBIT: libc::Ioctl = 0x40045564;
    const UI_SET_KEYBIT: libc::Ioctl = 0x40045565;
    const UI_DEV_CREATE: libc::Ioctl = 0x5501;
    const UI_DEV_DESTROY: libc::Ioctl = 0x5502;
    const EV_KEY: u16 = 1;
    const EV_SYN: u16 = 0;
    const KEY_F9: u16 = 67;

    let uinput_path = std::ffi::CString::new("/dev/uinput")
        .map_err(|e| format!("CString error: {}", e))?;

    let fd = unsafe { libc::open(uinput_path.as_ptr(), libc::O_WRONLY | libc::O_NONBLOCK) };
    if fd < 0 {
        return Err(format!("Failed to open /dev/uinput: {}", std::io::Error::last_os_error()));
    }

    unsafe {
        libc::ioctl(fd, UI_SET_EVBIT, EV_KEY as libc::c_int);
        libc::ioctl(fd, UI_SET_KEYBIT, KEY_F9 as libc::c_int);
    }

    // uinput_user_dev struct: name[80] + id(4xu16) + ff_max(u32) + abs arrays
    let mut dev_buf = vec![0u8; 80 + 8 + 4 + 64 * 4 * 4];
    dev_buf[..11].copy_from_slice(b"grandma-kbd");

    let written = unsafe { libc::write(fd, dev_buf.as_ptr() as *const libc::c_void, dev_buf.len()) };
    if written < 0 {
        unsafe { libc::close(fd); }
        return Err(format!("Failed to write uinput dev: {}", std::io::Error::last_os_error()));
    }

    if unsafe { libc::ioctl(fd, UI_DEV_CREATE) } < 0 {
        unsafe { libc::close(fd); }
        return Err(format!("UI_DEV_CREATE failed: {}", std::io::Error::last_os_error()));
    }

    // Wait for MiSTer to discover the new device via inotify
    std::thread::sleep(std::time::Duration::from_secs(2));

    // Send F9 keypress
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let sec = now.as_secs() as i32;
    let usec = now.subsec_micros() as i32;

    #[repr(C)]
    struct InputEvent {
        sec: i32,
        usec: i32,
        type_: u16,
        code: u16,
        value: i32,
    }

    let events = [
        InputEvent { sec, usec, type_: EV_KEY, code: KEY_F9, value: 1 }, // press
        InputEvent { sec, usec, type_: EV_SYN, code: 0, value: 0 },
    ];

    for ev in &events {
        unsafe {
            libc::write(fd, ev as *const _ as *const libc::c_void, std::mem::size_of::<InputEvent>());
        }
    }

    std::thread::sleep(std::time::Duration::from_millis(100));

    let release_events = [
        InputEvent { sec, usec, type_: EV_KEY, code: KEY_F9, value: 0 }, // release
        InputEvent { sec, usec, type_: EV_SYN, code: 0, value: 0 },
    ];

    for ev in &release_events {
        unsafe {
            libc::write(fd, ev as *const _ as *const libc::c_void, std::mem::size_of::<InputEvent>());
        }
    }

    std::thread::sleep(std::time::Duration::from_millis(500));

    unsafe {
        libc::ioctl(fd, UI_DEV_DESTROY);
        libc::close(fd);
    }

    Ok(())
}

/// Write a command to /dev/MiSTer_cmd using non-blocking open.
#[cfg(target_os = "linux")]
fn write_mister_cmd(command: &str) -> Result<(), String> {
    let cmd_path = GrandmaPaths::mister_cmd();
    if !cmd_path.exists() {
        return Err("MiSTer_cmd FIFO not found".to_string());
    }

    let c_path = std::ffi::CString::new(
        cmd_path.to_str().unwrap_or("/dev/MiSTer_cmd")
    ).map_err(|e| format!("Invalid path: {}", e))?;

    let fd = unsafe { libc::open(c_path.as_ptr(), libc::O_WRONLY | libc::O_NONBLOCK) };
    if fd < 0 {
        return Err(format!("Failed to open MiSTer_cmd: {}", std::io::Error::last_os_error()));
    }

    let bytes = command.as_bytes();
    let written = unsafe { libc::write(fd, bytes.as_ptr() as *const libc::c_void, bytes.len()) };
    unsafe { libc::close(fd); }

    if written < 0 {
        Err(format!("Failed to write to MiSTer_cmd: {}", std::io::Error::last_os_error()))
    } else if (written as usize) != bytes.len() {
        Err(format!("Partial write to MiSTer_cmd: {} of {} bytes", written, bytes.len()))
    } else {
        Ok(())
    }
}

fn launch_game(game: &grandma_common::config::GameEntry) -> Result<(), String> {
    let cmd = format!("load_core {}\n", game.launch);
    info!("Launching: {}", cmd.trim());
    #[cfg(target_os = "linux")]
    { write_mister_cmd(&cmd) }
    #[cfg(not(target_os = "linux"))]
    { let _ = cmd; Ok(()) }
}

fn main() -> ExitCode {
    let base = std::env::args().nth(1)
        .unwrap_or_else(|| "/media/fat/grandma_launcher".to_string());
    let paths = GrandmaPaths::new(&base);
    init_logging();
    info!("grandma-launcher starting (base: {})", base);

    // Load config (required)
    let config = match GamesConfig::load(&paths.games_json()) {
        Ok(c) => c,
        Err(e) => {
            error!("Failed to load games config: {}", e);
            return ExitCode::FAILURE;
        }
    };

    // Filter out games with missing launch files
    let valid_games: Vec<_> = config.games.into_iter().filter(|g| {
        let exists = std::path::Path::new(&g.launch).exists();
        if !exists {
            warn!("Excluding {}: launch file missing ({})", g.id, g.launch);
        }
        exists
    }).collect();

    if valid_games.is_empty() {
        error!("No games with valid launch files");
        return ExitCode::FAILURE;
    }

    info!("{} games with valid launch files", valid_games.len());

    // Load state (optional)
    let state = State::load(&paths.state_json());

    // Load font
    let font_data = include_bytes!("../../../assets/fonts/DejaVuSans.ttf");
    let font = fontdue::Font::from_bytes(font_data as &[u8], fontdue::FontSettings::default())
        .expect("Failed to parse font");

    // Load art
    let art_cache = load_art_cache(&valid_games, &paths);

    // Open framebuffer (falls back to mock on non-MiSTer hardware)
    let default_resolution = grandma_common::config::Resolution { width: 1920, height: 1080 };
    let mut fb = match framebuf::Framebuffer::open(&default_resolution) {
        Ok(fb) => fb,
        Err(e) => {
            error!("Failed to open framebuffer: {}", e);
            return ExitCode::FAILURE;
        }
    };

    // Enable the FPGA framebuffer display. This requires two steps:
    // 1. Send F9 via uinput to trigger MiSTer's video_fb_enable() (SPI setup)
    // 2. Send fb_cmd to configure resolution/format
    #[cfg(target_os = "linux")]
    {
        info!("Enabling FPGA framebuffer via uinput F9");
        match enable_fpga_framebuffer() {
            Ok(_) => info!("FPGA framebuffer enabled"),
            Err(e) => warn!("Failed to enable FPGA framebuffer: {}", e),
        }

        info!("Sending fb_cmd to configure framebuffer mode");
        match write_mister_cmd("fb_cmd0 8888 1 1\n") {
            Ok(_) => info!("MiSTer framebuffer mode configured"),
            Err(e) => warn!("Failed to send fb_cmd: {}", e),
        }
    }

    // Load settings
    let settings = grandma_common::config::Settings::load(&paths.settings_json())
        .unwrap_or_default();

    // Build grid state
    let mut grid = GridState::new(
        valid_games,
        &state,
        settings.columns,
        settings.title.clone(),
    );

    // Initial render
    grid.render(&mut fb, &art_cache, &font);

    // Input setup
    let mut input_state = InputState::new();

    #[cfg(target_os = "linux")]
    {
        use std::os::unix::io::AsRawFd;

        let mut devices: Vec<evdev::Device> = Vec::new();
        let mut device_fds: Vec<i32> = Vec::new();
        if let Ok(entries) = std::fs::read_dir("/dev/input/") {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.to_str().map_or(false, |s| s.contains("event")) {
                    if let Ok(dev) = evdev::Device::open(&path) {
                        if !input::is_usable_device(&dev) {
                            info!("Skipping non-gamepad/keyboard device: {} ({:?})",
                                dev.name().unwrap_or("unknown"), path);
                            continue;
                        }
                        info!("Input device: {} ({:?})", dev.name().unwrap_or("unknown"), path);
                        unsafe {
                            let fd = dev.as_raw_fd();
                            let flags = libc::fcntl(fd, libc::F_GETFL);
                            libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
                            device_fds.push(fd);
                        }
                        devices.push(dev);
                    }
                }
            }
        }

        info!("Found {} input devices", devices.len());

        let mut prev_axis_y: Option<Action> = None;
        let mut prev_axis_x: Option<Action> = None;

        loop {
            for dev in &mut devices {
                if let Ok(events) = dev.fetch_events() {
                    for event in events {
                        match event.destructure() {
                            evdev::EventSummary::Key(_, code, 1) => {
                                if let Some(action) = input::normalize_key(code) {
                                    if let Some(action) = input_state.on_press(action) {
                                        match action {
                                            Action::Escape => return ExitCode::SUCCESS,
                                            Action::Confirm => {
                                                if let Some(game) = grid.selected_game() {
                                                    let name = game.name.clone();
                                                    let id = game.id.clone();

                                                    let mut new_state = State::load(&paths.state_json());
                                                    new_state.record_play(&id);
                                                    let json = serde_json::to_string_pretty(&new_state).unwrap_or_default();
                                                    let _ = atomic_write(&paths.state_json(), json.as_bytes());

                                                    fb.clear(framebuf::Color::DARK_BG);
                                                    render::render_text(&mut fb, &font, &format!("Loading {}...", name), 100, 400, 48.0, framebuf::Color::WHITE);
                                                    fb.present();

                                                    // Release input devices before launching core.
                                                    // Use raw fd to avoid borrow conflict with the event loop.
                                                    const EVIOCGRAB: libc::Ioctl = 0x40044590;
                                                    for i in 0..device_fds.len() {
                                                        unsafe {
                                                            libc::ioctl(device_fds[i], EVIOCGRAB, 0);
                                                        }
                                                    }

                                                    if let Err(e) = launch_game(grid.selected_game().unwrap()) {
                                                        error!("Launch failed: {}", e);
                                                    }
                                                    return ExitCode::SUCCESS;
                                                }
                                            }
                                            Action::Up => { grid.move_up(); grid.render(&mut fb, &art_cache, &font); }
                                            Action::Down => { grid.move_down(); grid.render(&mut fb, &art_cache, &font); }
                                            Action::Left => { grid.move_left(); grid.render(&mut fb, &art_cache, &font); }
                                            Action::Right => { grid.move_right(); grid.render(&mut fb, &art_cache, &font); }
                                            Action::Back => {}
                                        }
                                    }
                                }
                            }
                            evdev::EventSummary::Key(_, code, 0) => {
                                if let Some(action) = input::normalize_key(code) {
                                    input_state.on_release(action);
                                }
                            }
                            evdev::EventSummary::AbsoluteAxis(_, code, value) => {
                                if let Some(axis_event) = input::normalize_axis(code, value, &mut prev_axis_y, &mut prev_axis_x) {
                                    match axis_event {
                                        input::AxisEvent::Pressed(action) => {
                                            if input_state.on_press(action).is_some() {
                                                match action {
                                                    Action::Up => { grid.move_up(); grid.render(&mut fb, &art_cache, &font); }
                                                    Action::Down => { grid.move_down(); grid.render(&mut fb, &art_cache, &font); }
                                                    Action::Left => { grid.move_left(); grid.render(&mut fb, &art_cache, &font); }
                                                    Action::Right => { grid.move_right(); grid.render(&mut fb, &art_cache, &font); }
                                                    _ => {}
                                                }
                                            }
                                        }
                                        input::AxisEvent::Released(action) => {
                                            input_state.on_release(action);
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }

            if let Some(action) = input_state.poll_repeat() {
                match action {
                    Action::Up => { grid.move_up(); grid.render(&mut fb, &art_cache, &font); }
                    Action::Down => { grid.move_down(); grid.render(&mut fb, &art_cache, &font); }
                    Action::Left => { grid.move_left(); grid.render(&mut fb, &art_cache, &font); }
                    Action::Right => { grid.move_right(); grid.render(&mut fb, &art_cache, &font); }
                    _ => {}
                }
            }

            std::thread::sleep(std::time::Duration::from_millis(16));
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        eprintln!("Launcher requires Linux. Rendering grid to mock framebuffer for testing.");
        grid.render(&mut fb, &art_cache, &font);
        ExitCode::SUCCESS
    }
}
