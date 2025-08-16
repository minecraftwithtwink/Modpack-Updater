use crate::app::{history, App, AppState, DependencyStatus, RunMode, TutorialState, UpdateStatus};
use crate::changelog;
use crate::dependency_check;
use crate::git;
use crate::music::MusicPlayer;
use crate::ui;
use anyhow::Result;
use arboard::Clipboard;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::backend::Backend;
use ratatui::widgets::ListState;
use ratatui::Terminal;
use std::env;
use std::io::Write;
use std::path::Path;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use tui_input::backend::crossterm::EventHandler;


pub fn run<B: Backend + Write>(
    terminal: &mut Terminal<B>,
    app: &mut App,
    music_player: &mut MusicPlayer,
) -> Result<()> {
    // Start the initial dependency check
    let (tx, rx) = mpsc::channel();
    app.dependency_rx = Some(rx);
    thread::spawn(move || {
        dependency_check::check_dependencies_background(tx);
    });

    loop {
        // --- Channel Checkers ---
        if let Some(rx) = &app.dependency_rx {
            if let Ok(status) = rx.try_recv() {
                match status {
                    DependencyStatus::AllOk => {
                        app.state = AppState::Browsing;
                    }
                    _ => {
                        app.state = AppState::ConfirmDependencyInstall { missing: status };
                    }
                }
                app.dependency_rx = None;
            }
        }

        if let Some(rx) = &app.install_rx {
            if let Ok(result) = rx.try_recv() {
                match result {
                    Ok(_) => {
                        app.state = AppState::Browsing;
                    }
                    Err(e) => {
                        app.state = AppState::Finished(format!("Dependency installation failed:\n\n{}\n\nPlease install Git and Git LFS manually.", e));
                    }
                }
                app.install_rx = None;
            }
        }

        if let Some(rx) = &app.update_rx {
            if let Ok(status) = rx.try_recv() {
                match status {
                    UpdateStatus::UpdateAvailable(version) => {
                        if app.tutorial.is_some() {
                            app.pending_update = Some(version);
                        } else {
                            app.state = AppState::ConfirmUpdate { version };
                        }
                    }
                    _ => {}
                }
                app.update_rx = None;
            }
        }

        if let Some(rx) = &app.changelog_rx {
            if let Ok(result) = rx.try_recv() {
                match result {
                    Ok(content) => {
                        app.state = AppState::ViewingChangelog { content, scroll: 0 };
                    }
                    Err(e) => {
                        app.state = AppState::Finished(format!("Failed to fetch changelog:\n\n{}", e));
                    }
                }
                app.changelog_rx = None;
            }
        }

        if let Some(rx) = &app.branch_rx {
            if let Ok(result) = rx.try_recv() {
                match result {
                    Ok(branches) => {
                        let mut list_state = ListState::default();
                        if !branches.is_empty() {
                            list_state.select(Some(0));
                        }
                        app.state = AppState::BranchSelection { branches, list_state, selected_branch: None };
                    }
                    Err(e) => {
                        app.state = AppState::Finished(format!("Failed to fetch branches:\n\n{}", e));
                    }
                }
                app.branch_rx = None;
            }
        }

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
                    // --- Top-level input handlers for popups ---
                    match &mut app.state {
                        AppState::ConfirmDependencyInstall { .. } => {
                            match key.code {
                                KeyCode::Char('y') | KeyCode::Char('Y') => {
                                    let (tx, rx) = mpsc::channel();
                                    app.install_rx = Some(rx);
                                    app.state = AppState::InstallingDependencies;
                                    thread::spawn(move || {
                                        dependency_check::install_dependencies_background(tx);
                                    });
                                }
                                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc | KeyCode::Char('q') => {
                                    return Ok(());
                                }
                                _ => {}
                            }
                            continue;
                        }
                        AppState::ConfirmUpdate { .. } => {
                            match key.code {
                                KeyCode::Char('y') | KeyCode::Char('Y') => {
                                    app.should_perform_update = true;
                                    return Ok(());
                                }
                                KeyCode::Esc => {
                                    app.state = AppState::Browsing;
                                }
                                _ => {}
                            }
                            continue;
                        }
                        AppState::ViewingChangelog { scroll, .. } => {
                            match key.code {
                                KeyCode::Up => *scroll = scroll.saturating_sub(1),
                                KeyCode::Down => *scroll = scroll.saturating_add(1),
                                KeyCode::Esc => app.state = AppState::Browsing,
                                _ => {}
                            }
                            continue;
                        }
                        _ => {}
                    }

                    // --- Main input routing ---
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
    if key.code == KeyCode::Char('s') {
        music_player.play_confirm_sfx();
        history::mark_tutorial_as_completed().ok();
        app.tutorial = None;
        app.tutorial_interactive = true;
        if let Some(version) = app.pending_update.clone() {
            app.state = AppState::ConfirmUpdate { version };
        }
        return Ok(());
    }

    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => {
            app.tutorial = None;
            music_player.play_cancel_sfx();
            if let Some(version) = app.pending_update.clone() {
                app.state = AppState::ConfirmUpdate { version };
            }
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
        KeyCode::Char('c') => {
            let (tx, rx) = mpsc::channel();
            app.changelog_rx = Some(rx);
            app.state = AppState::FetchingChangelog;
            std::thread::spawn(move || {
                changelog::fetch_changelog_background(tx);
            });
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

    let mut next_state: Option<AppState> = None;
    let mut branch_to_process: Option<String> = None;

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
                            next_state = Some(AppState::ConfirmReinit);
                            if app.tutorial.is_some() {
                                history::mark_tutorial_as_completed().ok();
                                app.tutorial = None;
                                if let Some(version) = app.pending_update.clone() {
                                    next_state = Some(AppState::ConfirmUpdate { version });
                                }
                            }
                        } else {
                            next_state = Some(AppState::ConfirmInvalidFolder {
                                path: current_path.clone(),
                            });
                        }
                    } else {
                        app.selected_path = Some(current_path.clone());
                    }
                }
            }
            KeyCode::Esc => {
                if app.selected_path.is_some() {
                    app.selected_path = None;
                } else {
                    app.history.retain(|path| path.exists() && path.is_dir());
                    if app.history.is_empty() {
                        app.history_state.select(None);
                    } else {
                        app.history_state.select(Some(app.history.len()));
                    }
                    app.mode = RunMode::StartupSelection;
                }
            }
            KeyCode::Char('f') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                next_state = Some(AppState::AwaitingInput);
                app.input.reset();
                app.input_error = None;
            }
            KeyCode::Char('q') => return Ok(false),
            KeyCode::Char('p') => music_player.toggle_pause(),
            _ => {}
        },
        AppState::ConfirmInvalidFolder { .. } => {
            if matches!(key.code, KeyCode::Enter | KeyCode::Esc) {
                next_state = Some(AppState::Browsing);
                app.selected_path = None;
            }
        }
        AppState::InsideInstanceFolderError => {
            if key.code == KeyCode::Left {
                app.go_up()?;
                next_state = Some(AppState::Browsing);
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
                            next_state = Some(AppState::Browsing);
                            app.input_error = None;
                        } else {
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
                                        next_state = Some(AppState::InsideInstanceFolderError);
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
                    }
                    KeyCode::Esc => {
                        next_state = Some(AppState::Browsing);
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
                app.branch_rx = Some(rx);
                next_state = Some(AppState::FetchingBranches);
                std::thread::spawn(move || {
                    git::fetch_remote_branches_threaded(tx);
                });
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                next_state = Some(AppState::Browsing);
                app.confirmed_path = None;
            }
            _ => {}
        },
        AppState::BranchSelection { branches, list_state, selected_branch } => {
            match key.code {
                KeyCode::Down => {
                    if !branches.is_empty() {
                        let i = list_state.selected().map_or(0, |i| (i + 1) % branches.len());
                        list_state.select(Some(i));
                    }
                }
                KeyCode::Up => {
                    if !branches.is_empty() {
                        let i = list_state.selected().map_or(0, |i| (i + branches.len() - 1) % branches.len());
                        list_state.select(Some(i));
                    }
                }
                KeyCode::Enter => {
                    if let Some(i) = list_state.selected() {
                        let highlighted_branch = &branches[i];
                        if Some(highlighted_branch) == selected_branch.as_ref() {
                            branch_to_process = Some(highlighted_branch.clone());
                        } else {
                            *selected_branch = Some(highlighted_branch.clone());
                        }
                    }
                }
                KeyCode::Esc => {
                    if selected_branch.is_some() {
                        *selected_branch = None;
                    } else {
                        next_state = Some(AppState::Browsing);
                    }
                }
                _ => {}
            }
        }
        AppState::Finished(_) => {
            if matches!(key.code, KeyCode::Enter | KeyCode::Char('q') | KeyCode::Esc) {
                return Ok(false);
            }
        }
        _ => {}
    }

    if let Some(state) = next_state {
        app.state = state;
    }

    if let Some(branch) = branch_to_process {
        let (tx, rx) = mpsc::channel();
        app.progress_rx = Some(rx);
        app.state = AppState::Processing { message: "Initializing...".to_string(), progress: 0.0, };
        let path = app.confirmed_path.clone().unwrap();
        std::thread::spawn(move || {
            git::perform_git_operations_threaded(path, branch, tx);
        });
    }

    Ok(true)
}