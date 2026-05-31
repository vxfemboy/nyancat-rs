//! Raw local-terminal playback (`--raw`).
//!
//! Renders the animation straight to the controlling terminal using crossterm's
//! alternate screen + raw mode. Used only when no servers are running, so it can
//! own the screen without log output interfering.

use std::io::{stdout, Write};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use crossterm::{
    cursor::{Hide, Show},
    event::{poll, read, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};

use crate::art::INTERVAL;
use crate::render::Animation;

/// Play the animation locally until the user presses `q`, `Esc`, or `Ctrl+C`.
pub fn run(animation: Arc<Animation>) -> Result<()> {
    let mut out = stdout();
    terminal::enable_raw_mode()?;
    execute!(out, EnterAlternateScreen, Hide)?;

    let result = play(&mut out, &animation);

    // Always restore the terminal, even if playback errored.
    let _ = execute!(out, LeaveAlternateScreen, Show);
    let _ = terminal::disable_raw_mode();
    result
}

fn play(out: &mut impl Write, animation: &Animation) -> Result<()> {
    let mut frame_index = 0usize;
    loop {
        if poll(Duration::from_millis(10))? {
            if let Event::Key(KeyEvent {
                code, modifiers, ..
            }) = read()?
            {
                let ctrl_c =
                    code == KeyCode::Char('c') && modifiers.contains(KeyModifiers::CONTROL);
                if ctrl_c || code == KeyCode::Char('q') || code == KeyCode::Esc {
                    break;
                }
            }
        }

        let (width, height) = terminal::size()?;
        let frame = animation.render(frame_index, width as usize, height as usize);
        out.write_all(frame.as_bytes())?;
        out.flush()?;

        frame_index = frame_index.wrapping_add(1);
        std::thread::sleep(INTERVAL);
    }
    Ok(())
}
