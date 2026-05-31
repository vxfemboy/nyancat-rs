//! Frame rendering.
//!
//! [`Animation`] owns the immutable frame data and color map and is shared as an
//! `Arc<Animation>` across every transport (SSH, Telnet, local). It is stateless:
//! each connection tracks its own frame index and asks the animation to render a
//! given frame cropped to the connection's terminal size. This avoids a shared
//! mutable frame counter and any locking on the hot path.

use std::collections::HashMap;

use crate::art::{self, OUTPUT_CHAR};

/// Immutable, shareable animation: frames plus the character → color map.
pub struct Animation {
    frames: Vec<Vec<String>>,
    colors: HashMap<char, u8>,
}

impl Animation {
    /// Build the animation from the embedded [`crate::art::FRAMES`] data.
    pub fn new() -> Self {
        let frames = art::FRAMES
            .iter()
            .map(|frame| frame.iter().map(|line| line.to_string()).collect())
            .collect();
        Self {
            frames,
            colors: art::colors(),
        }
    }

    /// Render `frame_index` (taken modulo the frame count) as an ANSI string,
    /// center-cropped to fit a `width` × `height` terminal. The returned string
    /// clears the screen and positions the cursor, so it can be written directly
    /// to any terminal or SSH/Telnet channel.
    pub fn render(&self, frame_index: usize, width: usize, height: usize) -> String {
        let frame = &self.frames[frame_index % self.frames.len()];

        let mut min_row = 0;
        let mut max_row = frame.len();
        let mut min_col = 0;
        let mut max_col = frame[0].chars().count();

        // Each art cell renders as OUTPUT_CHAR (two columns), so the number of
        // cells that fit horizontally is the terminal width divided by its length.
        let cell_cols = (width / OUTPUT_CHAR.len()).max(1);

        if max_row > height {
            min_row = (max_row - height) / 2;
            max_row = min_row + height;
        }
        if max_col > cell_cols {
            min_col = (max_col - cell_cols) / 2;
            max_col = min_col + cell_cols;
        }

        let mut buf = String::with_capacity(width * height * 2);
        // Clear screen, cursor home.
        buf.push_str("\x1B[2J\x1B[1;1H");

        for (row_idx, line) in frame[min_row..max_row].iter().enumerate() {
            if row_idx > 0 {
                buf.push_str(&format!("\x1B[{};1H", row_idx + 1));
            }
            for ch in line.chars().skip(min_col).take(max_col - min_col) {
                let code = self.colors.get(&ch).copied().unwrap_or(0);
                buf.push_str(&format!("\x1B[48;5;{code}m{OUTPUT_CHAR}\x1B[0m"));
            }
        }

        buf
    }
}

impl Default for Animation {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 2x2 animation with one frame, for deterministic assertions.
    fn tiny() -> Animation {
        Animation {
            frames: vec![vec!["+,".to_string(), ".#".to_string()]],
            colors: art::colors(),
        }
    }

    #[test]
    fn renders_clear_and_home() {
        let out = tiny().render(0, 80, 24);
        assert!(out.starts_with("\x1B[2J\x1B[1;1H"));
    }

    #[test]
    fn known_chars_map_to_their_codes() {
        let out = tiny().render(0, 80, 24);
        // '+' -> 226, '.' -> 15, '#' -> 82
        assert!(out.contains("\x1B[48;5;226m"));
        assert!(out.contains("\x1B[48;5;15m"));
        assert!(out.contains("\x1B[48;5;82m"));
    }

    #[test]
    fn unknown_char_falls_back_to_zero() {
        let anim = Animation {
            frames: vec![vec!["?".to_string()]],
            colors: art::colors(),
        };
        let out = anim.render(0, 80, 24);
        assert!(out.contains("\x1B[48;5;0m"));
    }

    #[test]
    fn frame_index_wraps() {
        // Indexing past the end must not panic.
        let _ = tiny().render(99, 80, 24);
    }

    #[test]
    fn crops_when_frame_taller_than_viewport() {
        // 4 rows tall, viewport height 2 → exactly 2 rows rendered (one home + one move).
        let anim = Animation {
            frames: vec![vec![
                "+".to_string(),
                "+".to_string(),
                "+".to_string(),
                "+".to_string(),
            ]],
            colors: art::colors(),
        };
        let out = anim.render(0, 80, 2);
        // Only the second rendered row emits a reposition escape (`\x1B[2;1H`).
        let repositions = out.matches("\x1B[2;1H").count();
        assert_eq!(repositions, 1);
    }
}
