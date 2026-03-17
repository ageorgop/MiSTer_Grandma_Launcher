// SPDX-License-Identifier: GPL-3.0-or-later
use crate::framebuf::{Color, Framebuffer};
use grandma_common::config::{GameEntry, State};

pub struct GridState {
    pub selected: usize,
    #[allow(dead_code)]
    pub scroll_offset: usize,
    pub games: Vec<GameEntry>,
    #[allow(dead_code)]
    pub recently_played: Vec<String>,
    pub columns: u32,
    pub title: String,
}

pub type ArtCache = std::collections::HashMap<String, (u32, u32, Vec<u8>)>;

pub const TILE_WIDTH: u32 = 280;
pub const TILE_HEIGHT: u32 = 340;
pub const ART_HEIGHT: u32 = 280;
const TILE_PAD: u32 = 20;
const TOP_MARGIN: u32 = 80;
const LEFT_MARGIN: u32 = 60;
const HIGHLIGHT_BORDER: u32 = 4;

impl GridState {
    pub fn new(games: Vec<GameEntry>, state: &State, columns: u32, title: String) -> Self {
        Self {
            selected: 0,
            scroll_offset: 0,
            games,
            recently_played: state.recently_played.clone(),
            columns,
            title,
        }
    }

    pub fn selected_game(&self) -> Option<&GameEntry> {
        self.games.get(self.selected)
    }

    pub fn move_up(&mut self) {
        let cols = self.columns as usize;
        if self.selected >= cols {
            self.selected -= cols;
        }
    }

    pub fn move_down(&mut self) {
        let cols = self.columns as usize;
        if self.selected + cols < self.games.len() {
            self.selected += cols;
        }
    }

    pub fn move_left(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn move_right(&mut self) {
        if self.selected + 1 < self.games.len() {
            self.selected += 1;
        }
    }

    pub fn render(&self, fb: &mut Framebuffer, art_cache: &ArtCache, font: &fontdue::Font) {
        fb.clear(Color::DARK_BG);

        render_text(fb, font, &self.title, LEFT_MARGIN, 20, 40.0, Color::WHITE);

        let cols = self.columns;
        for (i, game) in self.games.iter().enumerate() {
            let col = (i as u32) % cols;
            let row = (i as u32) / cols;

            let x = LEFT_MARGIN + col * (TILE_WIDTH + TILE_PAD);
            let y = TOP_MARGIN + row * (TILE_HEIGHT + TILE_PAD);

            if y > fb.height() { break; }

            let is_selected = i == self.selected;

            if is_selected {
                fb.draw_rect(
                    x.saturating_sub(HIGHLIGHT_BORDER),
                    y.saturating_sub(HIGHLIGHT_BORDER),
                    TILE_WIDTH + HIGHLIGHT_BORDER * 2,
                    TILE_HEIGHT + HIGHLIGHT_BORDER * 2,
                    HIGHLIGHT_BORDER,
                    Color::HIGHLIGHT,
                );
            }

            if let Some((w, h, rgba)) = art_cache.get(&game.id) {
                fb.blit_rgba(x, y, *w, *h, rgba);
            } else {
                fb.fill_rect(x, y, TILE_WIDTH, ART_HEIGHT, Color { r: 40, g: 40, b: 60, a: 255 });
                render_text(fb, font, &game.name, x + 10, y + ART_HEIGHT / 2 - 10, 24.0, Color::WHITE);
            }

            if !is_selected {
                fb.dim_rect(x, y, TILE_WIDTH, TILE_HEIGHT, 80);
            }
            let label_color = if is_selected { Color::WHITE } else { Color { r: 150, g: 150, b: 150, a: 255 } };
            render_text(fb, font, &game.name, x, y + ART_HEIGHT + 8, 20.0, label_color);
        }

        if let Some(game) = self.games.get(self.selected) {
            let banner_y = fb.height().saturating_sub(80);
            fb.fill_rect(0, banner_y, fb.width(), 80, Color { r: 10, g: 10, b: 20, a: 255 });
            render_text(fb, font, &game.name, LEFT_MARGIN, banner_y + 10, 36.0, Color::WHITE);
            render_text(fb, font, "Press button to play", LEFT_MARGIN, banner_y + 50, 20.0, Color { r: 120, g: 120, b: 160, a: 255 });
        }

        fb.present();
    }
}

pub fn render_text(fb: &mut Framebuffer, font: &fontdue::Font, text: &str, x: u32, y: u32, size: f32, color: Color) {
    use fontdue::layout::{Layout, CoordinateSystem, TextStyle};

    let mut layout = Layout::new(CoordinateSystem::PositiveYDown);
    layout.append(&[font], &TextStyle::new(text, size, 0));

    for glyph in layout.glyphs() {
        let (metrics, bitmap) = font.rasterize_config(glyph.key);
        for gy in 0..metrics.height {
            for gx in 0..metrics.width {
                let coverage = bitmap[gy * metrics.width + gx];
                if coverage > 128 {
                    let px = x + glyph.x as u32 + gx as u32;
                    let py = y + glyph.y as u32 + gy as u32;
                    fb.set_pixel(px, py, color);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use grandma_common::config::GameEntry;

    fn test_games() -> Vec<GameEntry> {
        vec![
            GameEntry { id: "galaga".into(), name: "Galaga".into(), system: "arcade".into(), launch: "a.mra".into(), art: "a.png".into() },
            GameEntry { id: "mspacman".into(), name: "Ms. Pac-Man".into(), system: "arcade".into(), launch: "b.mra".into(), art: "b.png".into() },
            GameEntry { id: "dkong".into(), name: "Donkey Kong".into(), system: "arcade".into(), launch: "c.mra".into(), art: "c.png".into() },
        ]
    }

    #[test]
    fn test_navigation_right() {
        let state_cfg = grandma_common::config::State::default();
        let mut grid = GridState::new(test_games(), &state_cfg, 3, "TEST".into());
        assert_eq!(grid.selected, 0);
        grid.move_right();
        assert_eq!(grid.selected, 1);
    }

    #[test]
    fn test_navigation_bounds() {
        let state_cfg = grandma_common::config::State::default();
        let mut grid = GridState::new(test_games(), &state_cfg, 3, "TEST".into());
        grid.move_left();
        assert_eq!(grid.selected, 0);
        grid.selected = 2;
        grid.move_right();
        assert_eq!(grid.selected, 2);
    }

    #[test]
    fn test_selected_game() {
        let state_cfg = grandma_common::config::State::default();
        let grid = GridState::new(test_games(), &state_cfg, 3, "TEST".into());
        assert_eq!(grid.selected_game().unwrap().id, "galaga");
    }
}
