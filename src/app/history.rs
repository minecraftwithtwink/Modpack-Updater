use anyhow::{Context, Result};
use directories::ProjectDirs;
use std::fs;
use std::path::PathBuf;

// Helper to get the base config directory, avoiding code duplication.
fn get_config_dir() -> Result<PathBuf> {
    let proj_dirs = ProjectDirs::from("com", "vodkapocalypse", "ModpackUpdater")
        .context("Could not find a valid configuration directory")?;
    let config_dir = proj_dirs.config_dir();
    fs::create_dir_all(config_dir)?;
    Ok(config_dir.to_path_buf())
}

// Gets the path to the history file.
fn get_history_path() -> Result<PathBuf> {
    Ok(get_config_dir()?.join("history.txt"))
}

fn get_tutorial_flag_path() -> Result<PathBuf> {
    Ok(get_config_dir()?.join("tutorial.flag"))
}

pub fn mark_tutorial_as_completed() -> Result<()> {
    let path = get_tutorial_flag_path()?;
    fs::write(path, "")?;
    Ok(())
}

pub fn should_start_tutorial() -> bool {
    let history_is_empty = match load() {
        Ok(h) => h.is_empty(),
        Err(_) => true,
    };

    let flag_exists = match get_tutorial_flag_path() {
        Ok(path) => path.exists(),
        Err(_) => false,
    };

    history_is_empty && !flag_exists
}

pub fn load() -> Result<Vec<PathBuf>> {
    let path = get_history_path()?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(path)?;
    // --- THE FIX: Filter out paths that no longer exist ---
    let valid_history = content
        .lines()
        .filter(|l| !l.is_empty())
        .map(PathBuf::from)
        .filter(|p| p.exists() && p.is_dir()) // Check that the path still exists and is a directory
        .collect();

    Ok(valid_history)
}

pub fn save(history: &[PathBuf]) -> Result<()> {
    let path = get_history_path()?;
    let content: String = history
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect::<Vec<String>>()
        .join("\n");
    fs::write(path, content)?;
    Ok(())
}