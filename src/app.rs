use crate::update::UpdateStatus;
use anyhow::{Context, Result};
use ratatui::widgets::ListState;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use tokio::runtime::Runtime;
use tui_input::Input;

pub mod history;

#[derive(Deserialize, Debug, PartialEq)]
pub struct ModpackInfo {
    pub branch: String,
    pub version: String,
}

#[derive(Deserialize, Debug, PartialEq)]
pub struct MinecraftInfo {
    pub version: String,
}

#[derive(Deserialize, Debug, PartialEq)]
pub struct LoaderInfo {
    #[serde(rename = "type")]
    pub loader_type: String,
    pub version: String,
}

#[derive(Deserialize, Debug, PartialEq)]
pub struct VersionCheck {
    pub modpack: ModpackInfo,
    pub minecraft: MinecraftInfo,
    pub loader: LoaderInfo,
}

#[derive(Debug, Clone)]
pub enum GitProgress {
    Update(String, f64),
    Success(String),
    Failure(String),
}

#[derive(Debug, Clone)]
pub enum InstanceUpdateStatus {
    UpToDate,
    UpdateAvailable(String),
    Error(String),
    Checking,
}

#[derive(Debug)]
pub enum RunMode {
    StartupSelection,
    FileBrowser,
}

pub enum AppState {
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
    pub instance_versions: HashMap<PathBuf, InstanceUpdateStatus>,
    pub version_check_rx: Option<Receiver<(PathBuf, InstanceUpdateStatus)>>,
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
    pub pending_update: Option<String>,
    pub should_perform_update: bool,
    pub gosling_mode: bool,
    pub tutorial: Option<TutorialState>,
    pub tutorial_interactive: bool,
    pub tutorial_paused: bool,
    pub tutorial_step1_expanded: bool
}

async fn check_instance_version(path: &Path) -> InstanceUpdateStatus {
    let result: Result<InstanceUpdateStatus> = (async {
        let local_path = path.join("modpack_version_check.json");
        if !local_path.exists() {
            return Ok(InstanceUpdateStatus::Error("File not found".to_string()));
        }

        let local_content = fs::read_to_string(local_path).context("Failed to read local version file")?;
        let local_version: VersionCheck = serde_json::from_str(&local_content).context("Failed to parse local version file")?;

        let remote_url = format!(
            "https://raw.githubusercontent.com/minecraftwithtwink/Twinkcraft-Modpack/{}/modpack_version_check.json",
            local_version.modpack.branch
        );

        let client = reqwest::Client::new();
        let response = client.get(&remote_url).send().await.context("Failed to fetch remote version file")?;

        if !response.status().is_success() {
            return Ok(InstanceUpdateStatus::Error(format!("Branch not found ({})", response.status())));
        }

        let remote_version: VersionCheck = response.json().await.context("Failed to parse remote version file")?;

        if local_version.modpack.version != remote_version.modpack.version
            || local_version.loader.version != remote_version.loader.version
        {
            Ok(InstanceUpdateStatus::UpdateAvailable(remote_version.modpack.version))
        } else {
            Ok(InstanceUpdateStatus::UpToDate)
        }
    }).await;

    result.unwrap_or_else(|e: anyhow::Error| InstanceUpdateStatus::Error(format!("{}", e)))
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

        let rt = Runtime::new()?;
        let mut instance_versions = HashMap::new();
        let (version_tx, version_rx) = std::sync::mpsc::channel();

        for path in &history {
            instance_versions.insert(path.clone(), InstanceUpdateStatus::Checking);
            let path_clone = path.clone();
            let version_tx_clone = version_tx.clone();
            rt.spawn(async move {
                let status = check_instance_version(&path_clone).await;
                let _ = version_tx_clone.send((path_clone, status));
            });
        }

        Ok(Self {
            mode: RunMode::StartupSelection,
            history: history.clone(),
            history_state,
            instance_versions,
            version_check_rx: Some(version_rx),
            current_dir: PathBuf::new(),
            initial_dir: PathBuf::new(),
            items: vec![],
            selected: 0,
            list_state: ListState::default(),
            selected_path: None,
            confirmed_path: None,
            state: AppState::Browsing,
            input: Input::default(),
            input_error: None,
            progress_rx: None,
            update_rx: None,
            changelog_rx: None,
            branch_rx: None,
            pending_update: None,
            should_perform_update: false,
            gosling_mode: false,
            tutorial,
            tutorial_interactive,
            tutorial_paused: false,
            tutorial_step1_expanded: false
        })
    }

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
