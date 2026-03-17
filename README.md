# MiSTer Grandma Launcher

A curated retro game launcher for MiSTer FPGA — turn your MiSTer into a simple game kiosk.

<!-- TODO: add screenshot of game grid -->

**Status:** Early POC — tested with arcade (MAME) games only. Developed on a QMTECH MiSTer FPGA Cyclone V SoC.

## What It Does

- MiSTer boots, shows a brief splash screen (3-second escape window), then a grid of games with box art
- D-pad to browse, button to play — that's the entire interface
- Games launch via MiSTer's native `/dev/MiSTer_cmd` FIFO — no FPGA modifications, no custom cores
- Press B/ESC during the splash screen to escape to the normal MiSTer menu

## How It Works

**Fair warning:** this is a kludge. MiSTer wasn't designed for third-party launchers — there's no plugin API, no framebuffer abstraction, no input sharing protocol. Everything here is a workaround: we write pixels to a raw DDR3 address via `/dev/mem`, fake a keyboard F9 press through a virtual uinput device to trick MiSTer into enabling the display, and send commands through a named FIFO that was meant for internal use. It works, but understanding *why* each piece exists requires understanding which "proper" approach didn't work.

### Boot and Game Launch Flow

```
                       MiSTer powers on
                            │
                  ┌─────────▼──────────┐
                  │  user-startup.sh   │
                  │  (boot hook)       │
                  └─────────┬──────────┘
                            │ launches in background
                  ┌─────────▼──────────┐
                  │ grandma-supervisor │  ← tiny process manager, no rendering
                  └─────────┬──────────┘
                            │ spawns
                  ┌─────────▼──────────┐
                  │  grandma-splash    │  ← 3-second countdown + IP display
                  └────┬──────────┬────┘
                       │          │
                  ESC/B pressed   timeout
                       │          │
                  exit to      ┌──▼──────────────┐
                  normal       │ grandma-launcher │  ← game grid UI
                  MiSTer       └──┬──────────────┘
                  menu            │ user picks a game
                                  │
                       ┌──────────▼─────────────┐
                       │ write "load_core ..."  │
                       │ to /dev/MiSTer_cmd     │
                       └──────────┬─────────────┘
                                  │ launcher exits
                       ┌──────────▼──────────┐
                       │  MiSTer loads core   │  ← execl() replaces MiSTer process
                       │  (game plays)        │
                       └──────────┬──────────┘
                                  │ user resets / power cycles
                       ┌──────────▼──────────┐
                       │ MiSTer re-execs,    │
                       │ supervisor restarts │
                       │ the launcher        │
                       └─────────────────────┘
```

### The Four Binaries

| Binary | Role | What it does |
|--------|------|-------------|
| `grandma-supervisor` | Process manager | Launches splash, then launcher in a loop. Crash backoff: 3 fast failures and it gives up, letting normal MiSTer boot. No rendering, no I/O. |
| `grandma-splash` | Boot splash | Shows countdown and device IP. Exit code 0 = proceed to launcher, exit code 1 = escape to normal MiSTer menu. |
| `grandma-launcher` | Game UI | Reads config from disk, renders game grid to FPGA framebuffer via `/dev/mem`, waits for controller input, writes `load_core` to FIFO, exits. |
| `grandma-admin` | Web config | Optional HTTP server for managing the game list from a browser. Off by default. |

### Display Architecture

```
  ┌──────────────────────────────────────────────────┐
  │                    FPGA                          │
  │  ┌──────────────┐     ┌───────────────────┐     │
  │  │ OSD overlay   │     │  Scaler / Display │     │
  │  │ (MiSTer menu) │     │                   │     │
  │  └───────┬───────┘     └─────────▲─────────┘     │
  │          │ SPI                   │ SPI            │
  └──────────┼───────────────────────┼────────────────┘
             │                       │
  ┌──────────▼───────────────────────┼────────────────┐
  │                ARM CPU (HPS)                      │
  │                                                   │
  │  MiSTer binary ───── /dev/MiSTer_cmd (FIFO)      │
  │       │                       ▲                   │
  │       │ SPI commands          │ "fb_cmd0 ..."     │
  │       │                       │ "load_core ..."   │
  │       ▼                       │                   │
  │  DDR3 @ 0x22000000       grandma-launcher         │
  │  (FPGA framebuffer)       writes pixels here      │
  │                           via /dev/mem mmap        │
  └───────────────────────────────────────────────────┘
```

The display path works like this:

1. The FPGA framebuffer lives at a fixed DDR3 address (`0x22000000 + 4096`), not at `/dev/fb0`
2. The launcher `mmap`s this address via `/dev/mem` and writes BGRA pixels directly
3. MiSTer must stay alive — it owns the SPI bus to the FPGA. We tell it to enable the framebuffer display by:
   - Creating a virtual keyboard via uinput and sending F9 (triggers MiSTer's `video_fb_enable()`)
   - Writing `fb_cmd0 8888 1 1` to `/dev/MiSTer_cmd` to configure display mode
4. Game launching is just `load_core /path/to/game.mra\n` written to the same FIFO

### Why Not Just...

We tried the obvious approaches first. Here's why they don't work:

| Approach | Why it doesn't work |
|----------|-------------------|
| Write to `/dev/fb0` | MiSTer doesn't relay fb0 content to the FPGA display. Pixels go nowhere. |
| Kill MiSTer, take over the display | The OSD freezes on screen (it's FPGA-rendered, not Linux). The `/dev/MiSTer_cmd` FIFO disappears, so you can't launch games. |
| Send SPI commands directly | Partially works for display, but OSD management requires full FPGA core state. You still need MiSTer alive for game launching. |
| Use GroovyMiSTer (like MiSTer_Games_GUI) | Requires loading a custom FPGA core first via the OSD menu. Chicken-and-egg problem. |

So instead, we keep MiSTer alive, write pixels behind its back to raw DDR3, and poke it via FIFO and fake keypresses to cooperate. It's ugly but it works.

### The Fresh Boot Rule

Every launcher start rebuilds all state from scratch: reopen the framebuffer mmap, re-enumerate input devices, reload config, reload art, render the first frame from zero. No warm resume, no cached runtime state. This prevents a whole class of "works once, breaks on the second game" bugs caused by stale framebuffer state, vanished input devices, or corrupt mmap regions after a core switch.

## Quick Start

### Scripts Overview

There are two kinds of scripts, kept in separate places to avoid confusion:

| Script | Runs on | Location | Purpose |
|--------|---------|----------|---------|
| `deploy.sh` | Your PC | repo root | Build, package, ship to MiSTer, and run the installer remotely |
| `install.sh` | MiSTer | `mister_scripts/` | Set up directories, copy binaries, hook into boot, configure MiSTer.ini |
| `uninstall.sh` | MiSTer | `mister_scripts/` | Remove launcher, unhook from boot, optionally back up configs |
| `admin-start.sh` | MiSTer | `mister_scripts/` | Start the admin web server in the background |
| `admin-stop.sh` | MiSTer | `mister_scripts/` | Stop the admin web server |

`deploy.sh` calls `install.sh` — deploy is the developer workflow (build + ship), install is the on-device setup. If you download a prebuilt tarball (planned for v0.2), you'd only run `install.sh` on the MiSTer.

> **Note:** Only tested with arcade (MAME) games so far. The examples below use `.mra` files from the stock `_Arcade/` folder.

### Prerequisites

**On your build machine:**
- [Rust toolchain](https://rustup.rs/)
- [`cross`](https://github.com/cross-rs/cross): `cargo install cross` (requires Docker or Podman)
- SSH access to your MiSTer

**On your MiSTer:**
- Stock MiSTer setup with SSH enabled (it is by default)
- Some arcade ROMs installed (run [Update All](https://github.com/theypsilon/Update_All_MiSTer) to get `.mra` files and cores)

### Steps

**1. Clone the repo:**

```bash
git clone <repo-url>
cd <repo>/MiSTer_Grandma_Launcher
```

**2. Configure SSH access to your MiSTer:**

Add to `~/.ssh/config`:
```
Host mister
    HostName <your-mister-ip>
    User root
```

Verify with `ssh mister` — you should get a root shell on the MiSTer.

**3. Build and deploy:**

```bash
./deploy.sh
```

This cross-compiles all four binaries for ARM, copies them to the MiSTer, runs the installer, and starts the supervisor. Takes a few minutes on first build.

**4. Add some games:**

SSH into the MiSTer and edit the game list:

```bash
ssh mister
vi /media/fat/grandma_launcher/games.json
```

Copy the included example file as a starting point, or edit `/media/fat/grandma_launcher/games.json` directly. It has Galaga, Ms. Pac-Man, and Donkey Kong — these `.mra` files exist on stock MiSTer if you've run Update All (your exact filenames may differ — check `ls /media/fat/_Arcade/` on your device):

```bash
scp games.json.example mister:/media/fat/grandma_launcher/games.json
```

See [`games.json.example`](games.json.example), which looks like this:

```json
{
  "schema": 1,
  "games": [
    {
      "id": "galagamidwayset1",
      "name": "Galaga (Midway, Set 1)",
      "system": "arcade",
      "launch": "/media/fat/_Arcade/Galaga (Midway, Set 1).mra",
      "art": "assets/boxart/galagamidwayset1.png"
    },
    {
      "id": "mspacman",
      "name": "Ms. Pac-Man",
      "system": "arcade",
      "launch": "/media/fat/_Arcade/Ms. Pac-Man.mra",
      "art": "assets/boxart/mspacman.png"
    },
    {
      "id": "donkeykongusset1",
      "name": "Donkey Kong (US, Set 1)",
      "system": "arcade",
      "launch": "/media/fat/_Arcade/Donkey Kong (US, Set 1).mra",
      "art": "assets/boxart/donkeykongusset1.png"
    }
  ]
}
```

**5. Restart the launcher:**

```bash
ssh mister 'killall grandma-supervisor 2>/dev/null; setsid /media/fat/grandma_launcher/bin/grandma-supervisor /media/fat/grandma_launcher </dev/null >/dev/null 2>&1 & disown'
```

Or just reboot the MiSTer — the launcher starts automatically on boot.

**6. You should see the game grid.** Games appear without box art initially — art is optional. Navigate with D-pad, press a button to launch.

## Adding Box Art

Place PNG files in `/media/fat/grandma_launcher/assets/boxart/` on the MiSTer. The `art` field in `games.json` is relative to the `grandma_launcher` directory, so `"art": "assets/boxart/dkong.png"` maps to `/media/fat/grandma_launcher/assets/boxart/dkong.png`.

Any resolution works — images are resized at startup to fit the grid tiles.

## Admin Web Server

The admin server lets you manage the game list from a browser instead of editing JSON by hand. It scans your MiSTer's `_Arcade/` folder for available `.mra` files and lets you add/remove/reorder games.

> **Security warning:** The admin server has **no authentication** — anyone on your network can access it. Only run it when you need it, and stop it when you're done. Password protection is planned for a future version.

### Starting the admin server

SSH into your MiSTer and run:

```bash
/media/fat/grandma_launcher/admin-start.sh
```

This prints the URL to open in your browser (e.g. `http://10.73.7.226:8080`). The server runs in the background.

### Stopping the admin server

```bash
/media/fat/grandma_launcher/admin-stop.sh
```

### What it can do

- Browse available `.mra` files on the MiSTer
- Add games to your curated list
- Remove games
- Save changes (atomic write with backup)

### What it can't do yet

- Upload or scrape box art
- Generate MGL files for console games
- Any kind of authentication

The default port is 8080, configurable via `admin_port` in `settings.json`.

## Controls

| Input | Action |
|-------|--------|
| D-pad / Arrow keys | Navigate the game grid |
| South face button / Enter | Launch selected game |
| B / ESC (during boot splash only) | Escape to normal MiSTer menu |

## Configuration

All config files live in `/media/fat/grandma_launcher/` on the MiSTer.

### games.json (required)

The curated game list. You must create this with at least one entry.

| Field | Description |
|-------|-------------|
| `id` | Unique identifier (used for state tracking and art filename) |
| `name` | Display name shown in the grid |
| `system` | System label (`"arcade"`, `"nes"`, etc.) — for future use |
| `launch` | Absolute path to `.mra` or `.mgl` file on the MiSTer |
| `art` | Path to box art PNG, relative to `grandma_launcher/` directory |

### settings.json (auto-created)

UI settings. Created with defaults on first install — edit to customize.

| Field | Default | Description |
|-------|---------|-------------|
| `title` | `"GAME TIME!"` | Text displayed at the top of the grid |
| `columns` | `3` | Number of columns in the game grid |
| `boot_delay_seconds` | `3` | Splash screen countdown duration |
| `admin_server` | `false` | Enable the web config server |
| `admin_port` | `8080` | Port for the web config server |

### state.json (auto-managed)

Tracks recently played games. Don't edit this — it's auto-managed by the launcher. If it gets corrupted or deleted, it's silently recreated on the next game launch.

## Directory Layout on MiSTer

```
/media/fat/grandma_launcher/
├── bin/
│   ├── grandma-supervisor
│   ├── grandma-splash
│   ├── grandma-launcher
│   └── grandma-admin
├── assets/
│   ├── font.ttf
│   └── boxart/            ← your PNG files go here
├── mgls/                  ← for future MGL support
├── admin-start.sh         ← start the admin web server
├── admin-stop.sh          ← stop the admin web server
├── games.json             ← you edit this
├── settings.json          ← auto-created, optionally customize
├── state.json             ← auto-managed, don't touch
└── grandma.log            ← rotating log, ~100KB max
```

The installer also modifies:
- `/media/fat/linux/user-startup.sh` — adds the supervisor launch line
- `/media/fat/MiSTer.ini` — ensures `fb_terminal=1` is set

## Uninstalling

```bash
ssh mister '/media/fat/grandma_launcher/uninstall.sh'
```

This kills running processes, removes the startup hook, and prompts to back up your configs and art before deleting. Use `-f` to skip prompts and remove everything.

## Development

### Project Structure

The project is a Cargo workspace with five crates:

| Crate | Purpose |
|-------|---------|
| `grandma-common` | Shared types, config parsing, atomic file writes |
| `grandma-supervisor` | Process manager with crash backoff logic |
| `grandma-splash` | Boot splash screen |
| `grandma-launcher` | Framebuffer rendering + evdev input handling |
| `grandma-admin` | HTTP server for game list management |

### Building Without `cross`

If you don't want Docker, you can build with plain cargo — but you'll need an ARM cross-linker installed:

```bash
rustup target add armv7-unknown-linux-musleabihf
cargo build --release --target armv7-unknown-linux-musleabihf
```

See `.cargo/config.toml` for linker configuration.

### Running Tests

```bash
cargo test
```

Tests run on the host (not ARM). 25 tests cover config parsing, atomic writes, input normalization, and crash backoff.

## Roadmap to v0.2

- [ ] GitHub Actions CI for cross-compilation (build on every push)
- [ ] GitHub Release workflow — attach prebuilt tarball so users can download and install without Rust/cross
- [ ] Screenshot of the game grid for the README
- [ ] Responsive UI layout — scale tiles, margins, and fonts based on detected resolution and orientation (CRT, 720p, vertical/tate monitors, non-16:9 LCDs)
- [ ] Admin server authentication (password protection)

## Known Limitations

- **Only tested with arcade (MAME) games.** Console games via MGL are untested. Stick to `.mra` files in `_Arcade/` for now.
- **No automatic box art scraping.** Place PNGs manually in `assets/boxart/`.
- **Splash screen escape is stdin-only.** It reads from stdin, not evdev — escape detection requires the process to have a TTY.
- **No EVIOCGRAB.** If MiSTer grabs exclusive access to your controller, the launcher won't see its input.
- **No controller hotplug.** Input devices are enumerated once at launcher startup.
- **1080p HDMI only.** The UI layout (tile sizes, margins, fonts) is hardcoded for 1920x1080. The framebuffer adapts to other resolutions, but the grid won't fit on CRT (240p), 720p, vertical/tate monitors, or non-16:9 LCDs.
- **Admin web server has no authentication.** Anyone on your local network can access it. Only run it when actively managing games.
- **Admin web server is minimal.** No art scraping or MGL generation yet.

## License

[GPL-3.0-or-later](LICENSE)
