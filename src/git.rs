pub(crate) use crate::app::GitProgress;
use anyhow::{bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::Sender;

const GIT_REMOTE_URL: &str = "https://github.com/minecraftwithtwink/Twinkcraft-Modpack.git";

fn run_command(command: &mut Command, description: &str, progress_tx: &Sender<GitProgress>) -> Result<()> {
    progress_tx.send(GitProgress::Update(description.to_string(), 0.5)).ok();
    let status = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .status()
        .context(format!("Failed to execute command: {:?}", command))?;

    if !status.success() {
        bail!("Command failed: {:?}", command);
    }
    Ok(())
}

pub fn fetch_remote_branches_threaded(tx: Sender<Result<Vec<String>>>) {
    let result = (|| -> Result<Vec<String>> {
        let output = Command::new("git")
            .args(&["ls-remote", "--heads", GIT_REMOTE_URL])
            .output()
            .context("Failed to run `git ls-remote`")?;

        if !output.status.success() {
            bail!("`git ls-remote` failed: {}", String::from_utf8_lossy(&output.stderr));
        }

        let mut branches: Vec<String> = String::from_utf8(output.stdout)?
            .lines()
            .filter_map(|line| line.split('\t').nth(1))
            .filter_map(|ref_name| ref_name.strip_prefix("refs/heads/"))
            .map(String::from)
            .collect();

        branches.sort();
        Ok(branches)
    })();
    tx.send(result).ok();
}

fn clean_managed_directories(path: &Path, progress_tx: &Sender<GitProgress>) -> Result<()> {
    const DIRS_TO_CLEAN: &[&str] = &[
        "mods", "kubejs", "configureddefaults", "resourcepacks", "patchouli_books", "datapacks",
    ];
    progress_tx.send(GitProgress::Update("Cleaning managed directories...".to_string(), 1.0)).ok();
    for dir_name in DIRS_TO_CLEAN {
        run_command(
            Command::new("git").current_dir(path).args(&["clean", "-fdx", "--", dir_name]),
            &format!("Cleaning {}...", dir_name),
            progress_tx,
        )?;
    }
    Ok(())
}

fn force_copy_default_configs(instance_path: &Path, progress_tx: &Sender<GitProgress>) -> Result<()> {
    progress_tx.send(GitProgress::Update("Applying default configurations...".to_string(), 1.0)).ok();
    let source_base = instance_path.join("configureddefaults");
    const ITEMS_TO_COPY: &[(&str, &str, bool)] = &[
        ("config/fancymenu", "config/fancymenu", true),
        ("customsplashscreen", "customsplashscreen", true),
        ("config/fog", "config/fog", true),
        ("config/customsplashscreen.json", "config/customsplashscreen.json", false),
        ("config/raised.json", "config/raised.json", false),
        ("sodium-extra.properties", "sodium-extra.properties", false),
        ("sodiumextrainformation.json", "sodiumextrainformation.json", false),
        ("sodium-extra-options.json", "sodium-extra-options.json", false),
        ("sodium-fingerprint.json", "sodium-fingerprint.json", false),
        ("sodium-mixins.properties", "sodium-mixins.properties", false),
        ("sodium-options.json", "sodium-options.json", false),
        ("sodium-shadowy-path-blocks-options.json", "sodium-shadowy-path-blocks-options.json", false),
        ("tectonic.json", "tectonic.json", false),
        ("sparsestructures.json5", "sparsestructures.json5", false),
        ("parcool-client.toml", "parcool-client.toml", false),
    ];
    for (source_suffix, dest_suffix, is_directory) in ITEMS_TO_COPY {
        let source_path = source_base.join(source_suffix);
        let dest_path = instance_path.join(dest_suffix);
        if source_path.exists() {
            if let Some(parent) = dest_path.parent() { fs::create_dir_all(parent)?; }
            if *is_directory {
                if dest_path.exists() { fs::remove_dir_all(&dest_path)?; }
                copy_dir_all(&source_path, &dest_path)?;
            } else {
                fs::copy(&source_path, &dest_path)?;
            }
        }
    }
    Ok(())
}

fn copy_dir_all(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> Result<()> {
    fs::create_dir_all(&dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_dir() {
            copy_dir_all(entry.path(), dst.as_ref().join(entry.file_name()))?;
        } else {
            fs::copy(entry.path(), dst.as_ref().join(entry.file_name()))?;
        }
    }
    Ok(())
}

pub fn parse_input_path(input: &str) -> PathBuf {
    let trimmed = input.trim();
    let stripped = if trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() > 1 {
        &trimmed[1..trimmed.len() - 1]
    } else {
        trimmed
    };
    #[cfg(windows)] { PathBuf::from(stripped.replace('/', "\\")) }
    #[cfg(not(windows))] { PathBuf::from(stripped.replace('\\', "/")) }
}

pub fn perform_git_operations_threaded(path: PathBuf, branch_name: String, progress_tx: Sender<GitProgress>) {
    let result = (|| -> Result<String> {
        let git_dir = path.join(".git");
        if !git_dir.exists() {
            run_command(Command::new("git").current_dir(&path).arg("init"), "Initializing new repository...", &progress_tx)?;
            run_command(Command::new("git").current_dir(&path).args(&["remote", "add", "origin", GIT_REMOTE_URL]), "Setting remote URL...", &progress_tx)?;
        } else {
            run_command(Command::new("git").current_dir(&path).args(&["remote", "set-url", "origin", GIT_REMOTE_URL]), "Verifying remote URL...", &progress_tx)?;
        }

        run_command(Command::new("git").current_dir(&path).args(&["fetch", "origin"]), "Fetching from remote...", &progress_tx)?;
        run_command(Command::new("git").current_dir(&path).args(&["checkout", &branch_name]), &format!("Switching to branch '{}'...", branch_name), &progress_tx)?;
        run_command(Command::new("git").current_dir(&path).args(&["reset", "--hard", &format!("origin/{}", branch_name)]), "Resetting to latest version...", &progress_tx)?;
        run_command(Command::new("git").current_dir(&path).args(&["lfs", "pull"]), "Pulling LFS files...", &progress_tx)?;

        clean_managed_directories(&path, &progress_tx)?;
        force_copy_default_configs(&path, &progress_tx)?;

        Ok(format!("Successfully updated and verified repository at:\n\n{}\n\nPress Enter to close.", path.display()))
    })();

    match result {
        Ok(msg) => progress_tx.send(GitProgress::Success(msg)).ok(),
        Err(e) => progress_tx.send(GitProgress::Failure(format!("An error occurred:\n\n{:#}", e))).ok(),
    };
}