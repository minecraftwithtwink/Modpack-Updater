use crate::app::{history, App, AppState, RunMode, TutorialState};
use crate::git;
use crate::music::MusicPlayer;
use crate::ui;
use anyhow::Result;
use arboard::Clipboard;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::backend::Backend;
use ratatui::Terminal;
use std::env;
use std::io::Write;
use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;
use tui_input::backend::crossterm::EventHandler;


pub fn run<B: Backend + Write>(
    terminal: &mut Terminal<B>,
    app: &mut App,
    music_player: &mut MusicPlayer,
) -> Result<()> {
    loop {
        if let Some(rx) = &app.progress_rx {
            if let Ok(progress) = rx.try_recv() {
                match progress {
                    git::GitProgress::Update(message, ratio) => {
                        app.state = AppState::Processing { message, progress: ratio };
                    }
                    git::GitProgress::Success(message) => {
                        let path = app.confirmed_path.clone().unwrap();
                        if !app.history.contains(&path) {
                            app.history.push(path);
                            history::save(&app.history).ok();
                        }
                        app.state = AppState::Finished(message);
                        app.progress_rx = None;
                    }
                    git::GitProgress::Failure(message) => {
                        app.state = AppState::Finished(message);
                        app.progress_rx = None;
                    }
                }
            }
        }

        terminal.draw(|f| ui::draw(f, app, music_player))?;

        if event::poll(Duration::from_millis(10))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Release {
                    if app.tutorial.is_some() && !app.tutorial_paused {
                        handle_tutorial_input(app, key, music_player)?;
                    } else {
                        let mut should_quit = false;
                        match app.mode {
                            RunMode::StartupSelection => {
                                if matches!(key.code, KeyCode::Char('q') | KeyCode::Esc) {
                                    music_player.play_confirm_sfx();
                                    should_quit = true;
                                } else {
                                    handle_startup_input(app, key, music_player)?;
                                }
                            }
                            RunMode::FileBrowser => {
                                if !handle_file_browser_input(app, key, music_player)? {
                                    should_quit = true;
                                }
                            }
                        }

                        if should_quit {
                            return Ok(());
                        }
                    }
                }
            }
        }
    }
}

fn is_valid_instance_folder(path: &Path) -> bool {
    let has_mods = path.join("mods").is_dir();
    let has_config = path.join("config").is_dir();
    has_mods && has_config
}


fn handle_tutorial_input(app: &mut App, key: event::KeyEvent, music_player: &mut MusicPlayer) -> Result<()> {
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => {
            app.tutorial = None;
            music_player.play_cancel_sfx();
            return Ok(());
        }
        KeyCode::Char('p') => {
            music_player.toggle_pause();
            return Ok(());
        }
        _ => {}
    }

    if let Some(tutorial_state) = app.tutorial {
        match tutorial_state {
            TutorialState::Welcome => {
                music_player.play_confirm_sfx();
                app.tutorial = Some(TutorialState::StartupMenu);
            }
            TutorialState::StartupMenu => {
                match key.code {
                    KeyCode::Char('h') => {
                        app.tutorial_step1_expanded = !app.tutorial_step1_expanded;
                    }
                    KeyCode::Down | KeyCode::Up => {
                        app.history_next();
                        app.tutorial_interactive = true;
                    }
                    KeyCode::Enter if app.tutorial_interactive => {
                        music_player.play_confirm_sfx();
                        let start_dir = env::current_dir()?;
                        app.init_file_browser(start_dir)?;
                        app.tutorial = Some(TutorialState::FileBrowserNav);
                        app.tutorial_interactive = false;
                    }
                    _ => {}
                }
            }
            TutorialState::FileBrowserNav => {
                match key.code {
                    KeyCode::Up | KeyCode::Down | KeyCode::Left | KeyCode::Right => {
                        handle_file_browser_input(app, key, music_player)?;
                        app.tutorial_interactive = true;
                    }
                    KeyCode::Enter if app.tutorial_interactive => {
                        music_player.play_confirm_sfx();
                        if is_valid_instance_folder(&app.current_dir) {
                            app.tutorial = Some(TutorialState::InsideInstanceFolderHint);
                            app.tutorial_interactive = false;
                        } else {
                            app.tutorial = Some(TutorialState::FileBrowserSelect);
                        }
                    }
                    KeyCode::Char('f') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        handle_file_browser_input(app, key, music_player)?;
                        app.tutorial_paused = true;
                    }
                    _ => {}
                }
            }
            TutorialState::InsideInstanceFolderHint => {
                if key.code == KeyCode::Left {
                    music_player.play_scroll_sfx();
                    app.go_up()?;
                    app.tutorial = Some(TutorialState::FileBrowserNav);
                    app.tutorial_interactive = true;
                }
            }
            TutorialState::FileBrowserSelect => {
                match key.code {
                    KeyCode::Up | KeyCode::Down => {
                        handle_file_browser_input(app, key, music_player)?;
                    }
                    KeyCode::Enter if app.tutorial_interactive => {
                        handle_file_browser_input(app, key, music_player)?;
                        if let Some(selected) = &app.selected_path {
                            if is_valid_instance_folder(selected) {
                                app.tutorial = Some(TutorialState::FileBrowserConfirm);
                            } else {
                                app.tutorial = Some(TutorialState::InvalidSelectionHint);
                                app.tutorial_interactive = false;
                            }
                        }
                    }
                    _ => {}
                }
            }
            TutorialState::InvalidSelectionHint => {
                if matches!(key.code, KeyCode::Enter | KeyCode::Esc) {
                    music_player.play_cancel_sfx();
                    app.selected_path = None;
                    app.tutorial = Some(TutorialState::FileBrowserSelect);
                    app.tutorial_interactive = true;
                }
            }
            TutorialState::FileBrowserConfirm => {
                match key.code {
                    KeyCode::Enter if app.selected_path.is_some() => {
                        handle_file_browser_input(app, key, music_player)?;
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(())
}


fn handle_startup_input(app: &mut App, key: event::KeyEvent, music_player: &mut MusicPlayer) -> Result<()> {
    if app.gosling_mode {
        if key.code != KeyCode::Char('p') {
            music_player.play_sfx();
        }
    } else {
        match key.code {
            KeyCode::Enter => {
                music_player.play_confirm_sfx();
            }
            _ => {}
        }
    }

    match key.code {
        KeyCode::Up => app.history_previous(),
        KeyCode::Down => app.history_next(),
        KeyCode::Enter => {
            if let Some(selected_index) = app.history_state.selected() {
                if selected_index < app.history.len() {
                    let path = app.history[selected_index].clone();
                    if is_valid_instance_folder(&path) {
                        app.confirmed_path = Some(path);
                        app.state = AppState::ConfirmReinit;
                        app.mode = RunMode::FileBrowser;
                    } else {
                        app.state = AppState::ConfirmInvalidFolder { path };
                    }
                } else {
                    let start_dir = env::current_dir()?;
                    app.init_file_browser(start_dir)?;
                }
            }
        }
        KeyCode::Char('p') => music_player.toggle_pause(),
        _ => {}
    }
    Ok(())
}

fn handle_file_browser_input(app: &mut App, key: event::KeyEvent, music_player: &mut MusicPlayer) -> Result<bool> {
    match &app.state {
        AppState::Browsing => match key.code {
            KeyCode::Up | KeyCode::Down | KeyCode::Left | KeyCode::Right => {}
            KeyCode::Enter => {
                if !app.items.is_empty() {
                    let current_path = &app.items[app.selected];
                    if Some(current_path) == app.selected_path.as_ref() {
                        music_player.play_confirm_sfx();
                    } else {
                        music_player.play_scroll_sfx();
                    }
                }
            }
            KeyCode::Char('f') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                music_player.play_scroll_sfx();
            }
            KeyCode::Esc | KeyCode::Home => {
                music_player.play_cancel_sfx();
            }
            KeyCode::Char('q') => {
                music_player.play_confirm_sfx();
            }
            _ => {}
        },
        _ => {}
    }

    match &mut app.state {
        AppState::Browsing => match key.code {
            KeyCode::Down => app.next(),
            KeyCode::Up => app.previous(),
            KeyCode::Right => app.go_in()?,
            KeyCode::Left => app.go_up()?,
            KeyCode::Home => app.reset()?,
            KeyCode::Enter => {
                if !app.items.is_empty() {
                    let current_path = &app.items[app.selected];
                    if Some(current_path) == app.selected_path.as_ref() {
                        if is_valid_instance_folder(current_path) {
                            app.confirmed_path = Some(current_path.clone());
                            app.state = AppState::ConfirmReinit;
                            if app.tutorial.is_some() {
                                history::mark_tutorial_as_completed().ok();
                                app.tutorial = None;
                            }
                        } else {
                            app.state = AppState::ConfirmInvalidFolder {
                                path: current_path.clone(),
                            };
                        }
                    } else {
                        app.selected_path = Some(current_path.clone());
                    }
                }
            }
            KeyCode::Esc => {
                // --- THE FIX ---
                if app.selected_path.is_some() {
                    app.selected_path = None;
                } else {
                    // Before returning to the startup screen, validate the history.
                    app.history.retain(|path| path.exists() && path.is_dir());

                    // Also, reset the selection to a valid state.
                    if app.history.is_empty() {
                        app.history_state.select(None);
                    } else {
                        // Select the last item, which is the "Specify new..." option.
                        app.history_state.select(Some(app.history.len()));
                    }

                    app.mode = RunMode::StartupSelection;
                }
            }
            KeyCode::Char('f') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                app.state = AppState::AwaitingInput;
                app.input.reset();
                app.input_error = None;
            }
            KeyCode::Char('q') => return Ok(false),
            KeyCode::Char('p') => music_player.toggle_pause(),
            _ => {}
        },
        AppState::ConfirmInvalidFolder { .. } => {
            if matches!(key.code, KeyCode::Enter | KeyCode::Esc) {
                app.state = AppState::Browsing;
                app.selected_path = None;
            }
        }
        AppState::InsideInstanceFolderError => {
            if key.code == KeyCode::Left {
                app.go_up()?;
                app.state = AppState::Browsing;
            }
        }
        AppState::AwaitingInput => {
            if key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char('v') {
                if let Ok(mut clipboard) = Clipboard::new() {
                    if let Ok(text) = clipboard.get_text() {
                        app.input.handle_event(&Event::Paste(text));
                    }
                }
            } else {
                match key.code {
                    KeyCode::Enter => {
                        let path_str = app.input.value();
                        if path_str == "literally me" {
                            app.gosling_mode = true;
                            music_player.play_secret_track();
                            app.state = AppState::Browsing;
                            app.input_error = None;
                            return Ok(true);
                        }
                        let path = git::parse_input_path(path_str);
                        if path.exists() && path.is_dir() {
                            app.init_file_browser(path.clone())?;
                            app.input_error = None;

                            if is_valid_instance_folder(&path) {
                                if app.tutorial_paused {
                                    app.tutorial = Some(TutorialState::InsideInstanceFolderHint);
                                    app.tutorial_interactive = false;
                                    app.tutorial_paused = false;
                                } else {
                                    app.state = AppState::InsideInstanceFolderError;
                                }
                            } else if app.tutorial_paused {
                                app.tutorial = Some(TutorialState::FileBrowserSelect);
                                app.tutorial_interactive = true;
                                app.tutorial_paused = false;
                            }

                        } else {
                            app.input_error = Some("Error: Path not found or is not a directory.".to_string());
                        }
                    }
                    KeyCode::Esc => {
                        app.state = AppState::Browsing;
                        app.input_error = None;
                        if app.tutorial_paused {
                            app.tutorial_paused = false;
                        }
                    }
                    _ => {
                        app.input.handle_event(&Event::Key(key));
                    }
                }
            }
        },
        AppState::ConfirmReinit => match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                let (tx, rx) = mpsc::channel();
                app.progress_rx = Some(rx);
                app.state = AppState::Processing {
                    message: "Initializing...".to_string(),
                    progress: 0.0,
                };
                let path = app.confirmed_path.clone().unwrap();
                std::thread::spawn(move || {
                    git::perform_git_operations_threaded(path, tx);
                });
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                app.state = AppState::Browsing;
                app.confirmed_path = None;
            }
            _ => {}
        },
        AppState::Finished(_) => {
            if matches!(key.code, KeyCode::Enter | KeyCode::Char('q') | KeyCode::Esc) {
                return Ok(false);
            }
        }
        AppState::Processing { .. } => {}
    }
    Ok(true)
}