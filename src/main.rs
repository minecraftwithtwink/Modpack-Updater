mod app;
mod changelog;
mod event;
mod git;
mod music;
mod ui;
mod update;

use crate::app::App;
use anyhow::Result;
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::env; // --- ADDED: To get the current executable's path ---
use std::io;
use std::process::Command; // --- ADDED: To spawn the new process ---
use std::sync::mpsc;
use std::thread;

fn main() -> Result<()> {
    let (update_tx, update_rx) = mpsc::channel();
    thread::spawn(move || {
        update::check_for_updates_background(update_tx);
    });

    // 1. Setup
    let mut music_player = music::MusicPlayer::new()?;
    music_player.play();
    let history = app::history::load().unwrap_or_else(|_| {
        println!("Warning: Could not load history file.");
        Vec::new()
    });
    let mut app = App::new(history)?;
    app.update_rx = Some(update_rx);

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // 2. Run the main event loop
    let res = event::run(&mut terminal, &mut app, &mut music_player);

    // 3. Restore terminal and cleanup
    music_player.stop();
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    if app.should_perform_update {
        println!("Starting update...");
        // --- MODIFIED: The update and relaunch logic is now here ---
        match update::perform_update() {
            Ok(_) => {
                println!("Update successful! Relaunching...");
                // Try to get the path to the new executable
                if let Ok(updated_exe_path) = env::current_exe() {
                    // Spawn the new process. `.spawn()` detaches it, allowing our old process to exit cleanly.
                    Command::new(updated_exe_path).spawn()?;
                }
            }
            Err(e) => {
                eprintln!("Update failed: {}", e);
                println!("Press Enter to close.");
                let _ = io::stdin().read_line(&mut String::new());
            }
        }
        // Exit immediately after attempting the update, whether it succeeded or failed.
        return Ok(());
    }

    // 4. Handle normal exit conditions
    if let Err(err) = res {
        println!("Error: {:#}", err);
    } else if let Some(path) = app.confirmed_path {
        println!("Operation finished for: {}", path.display());
    }

    Ok(())
}