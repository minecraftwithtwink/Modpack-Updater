mod app;
mod event;
mod git;
mod music;
mod ui;

use crate::app::App;
use anyhow::Result;
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::io;

fn main() -> Result<()> {
    // 1. Setup
    let mut music_player = music::MusicPlayer::new()?;
    music_player.play();
    let history = app::history::load().unwrap_or_else(|_| {
        println!("Warning: Could not load history file.");
        Vec::new()
    });
    let mut app = App::new(history)?;
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    // Bracketed Paste is no longer needed
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // 2. Run the main event loop
    let res = event::run(&mut terminal, &mut app, &mut music_player);

    // 3. Restore terminal and cleanup
    music_player.stop();
    disable_raw_mode()?;
    // Bracketed Paste is no longer needed
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    // 4. Handle exit conditions
    if let Err(err) = res {
        println!("Error: {:#}", err);
    } else if let Some(path) = app.confirmed_path {
        println!("Operation finished for: {}", path.display());
    }

    Ok(())
}