use anyhow::Result;
use ratatui::widgets::ListState;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use tui_input::Input;

pub mod history;

#[derive(Debug)]
pub enum GitProgress {
    Update(String, f64),
    Success(String),
    Failure(String),
}

#[derive(Debug)]
pub enum UpdateStatus {
    UpToDate,
    UpdateAvailable(String),
    Error,
}

// --- ADDED: Enum for dependency check results ---
#[derive(Debug)]
pub enum DependencyStatus {
    AllOk,
    GitMissing,
    GitLfsMissing,
}

#[derive(Debug)]
pub enum RunMode {
    StartupSelection,
    FileBrowser,
}

pub enum AppState {
    // --- ADDED: New states for the initial dependency check ---
    CheckingDependencies,
    ConfirmDependencyInstall { missing: DependencyStatus },
    InstallingDependencies,
    Browsing,
    AwaitingInput,
    ConfirmReinit,
    ConfirmInvalidFolder { path: PathBuf },
    InsideInstanceFolderError,
    ConfirmUpdate { version: String },
    FetchingChangelog,
    ViewingChangelog { content: String, scroll: u16 },
    FetchingBranches,
    BranchSelection {
        branches: Vec<String>,
        list_state: ListState,
        selected_branch: Option<String>,
    },
    Processing { message: String, progress: f64 },
    Finished(String),
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum TutorialState {
    Welcome,
    StartupMenu,
    InsideInstanceFolderHint,
    FileBrowserNav,
    FileBrowserSelect,
    InvalidSelectionHint,
    FileBrowserConfirm,
}

pub struct App {
    pub mode: RunMode,
    pub history: Vec<PathBuf>,
    pub history_state: ListState,
    pub current_dir: PathBuf,
    pub initial_dir: PathBuf,
    pub items: Vec<PathBuf>,
    pub selected: usize,
    pub list_state: ListState,
    pub selected_path: Option<PathBuf>,
    pub confirmed_path: Option<PathBuf>,
    pub state: AppState,
    pub input: Input,
    pub input_error: Option<String>,
    pub progress_rx: Option<Receiver<GitProgress>>,
    pub update_rx: Option<Receiver<UpdateStatus>>,
    pub changelog_rx: Option<Receiver<Result<String>>>,
    pub branch_rx: Option<Receiver<Result<Vec<String>>>>,
    // --- ADDED: Channel for dependency check results ---
    pub dependency_rx: Option<Receiver<DependencyStatus>>,
    // --- ADDED: Channel for dependency installation results ---
    pub install_rx: Option<Receiver<Result<()>>>,
    pub pending_update: Option<String>,
    pub should_perform_update: bool,
    pub gosling_mode: bool,
    pub tutorial: Option<TutorialState>,
    pub tutorial_interactive: bool,
    pub tutorial_paused: bool,
    pub tutorial_step1_expanded: bool,
}

impl App {
    pub fn new(history: Vec<PathBuf>) -> Result<Self> {
        let mut history_state = ListState::default();
        if !history.is_empty() {
            history_state.select(Some(0));
        }

        let (tutorial, tutorial_interactive) = if history::should_start_tutorial() {
            (Some(TutorialState::Welcome), false)
        } else {
            (None, false)
        };

        Ok(Self {
            mode: RunMode::StartupSelection,
            history,
            history_state,
            current_dir: PathBuf::new(),
            initial_dir: PathBuf::new(),
            items: vec![],
            selected: 0,
            list_state: ListState::default(),
            selected_path: None,
            confirmed_path: None,
            // --- MODIFIED: App now starts in the CheckingDependencies state ---
            state: AppState::CheckingDependencies,
            input: Input::default(),
            input_error: None,
            progress_rx: None,
            update_rx: None,
            changelog_rx: None,
            branch_rx: None,
            dependency_rx: None,
            install_rx: None,
            pending_update: None,
            should_perform_update: false,
            gosling_mode: false,
            tutorial,
            tutorial_interactive,
            tutorial_paused: false,
            tutorial_step1_expanded: false,
        })
    }
    // ... rest of the file is unchanged ...
    pub fn init_file_browser(&mut self, path: PathBuf) -> Result<()> {
        let items = Self::read_dir(&path)?;
        let mut list_state = ListState::default();
        if !items.is_empty() {
            list_state.select(Some(0));
        }
        self.current_dir = path.clone();
        self.initial_dir = path;
        self.items = items;
        self.selected = 0;
        self.list_state = list_state;
        self.selected_path = None;
        self.confirmed_path = None;
        self.state = AppState::Browsing;
        self.mode = RunMode::FileBrowser;
        Ok(())
    }

    pub fn history_next(&mut self) {
        let i = self.history_state.selected().map_or(0, |i| {
            if i >= self.history.len() { 0 } else { i + 1 }
        });
        self.history_state.select(Some(i.min(self.history.len())));
    }

    pub fn history_previous(&mut self) {
        let i = self.history_state.selected().map_or(0, |i| {
            if i == 0 { self.history.len() } else { i - 1 }
        });
        self.history_state.select(Some(i));
    }

    pub fn read_dir(dir: &Path) -> Result<Vec<PathBuf>> {
        let mut folders: Vec<_> = fs::read_dir(dir)?
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        folders.sort();
        Ok(folders)
    }

    pub fn go_up(&mut self) -> Result<()> {
        if let Some(parent) = self.current_dir.parent() {
            let old_dir_name = self.current_dir.file_name().map(PathBuf::from);
            self.current_dir = parent.to_path_buf();
            self.items = Self::read_dir(&self.current_dir)?;
            self.selected = old_dir_name
                .and_then(|name| {
                    self.items.iter().position(|item| item.file_name() == Some(name.as_os_str()))
                })
                .unwrap_or(0);
            self.list_state.select(Some(self.selected));
            self.selected_path = None;
        }
        Ok(())
    }

    pub fn go_in(&mut self) -> Result<()> {
        if !self.items.is_empty() {
            let selected_path = &self.items[self.selected];
            if selected_path.is_dir() {
                self.current_dir = selected_path.clone();
                self.items = Self::read_dir(&self.current_dir)?;
                self.selected = 0;
                self.list_state.select(Some(0));
                self.selected_path = None;
            }
        }
        Ok(())
    }

    pub fn reset(&mut self) -> Result<()> {
        self.current_dir = self.initial_dir.clone();
        self.items = Self::read_dir(&self.current_dir)?;
        self.selected = 0;
        self.list_state.select(Some(0));
        self.selected_path = None;
        Ok(())
    }

    pub fn next(&mut self) {
        if !self.items.is_empty() {
            let i = self.list_state.selected().map_or(0, |i| {
                if i >= self.items.len() - 1 { 0 } else { i + 1 }
            });
            self.selected = i;
            self.list_state.select(Some(i));
        }
    }

    pub fn previous(&mut self) {
        if !self.items.is_empty() {
            let i = self.list_state.selected().map_or(0, |i| {
                if i == 0 { self.items.len() - 1 } else { i - 1 }
            });
            self.selected = i;
            self.list_state.select(Some(i));
        }
    }
}