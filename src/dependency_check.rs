use crate::app::DependencyStatus;
use anyhow::{bail, Context, Result};
use std::process::{Command, Stdio};
use std::sync::mpsc::Sender;
use which::which;

fn is_installed(cmd: &str) -> bool {
    which(cmd).is_ok()
}

fn run_install_command(command: &mut Command) -> Result<()> {
    let status = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .status()
        .context(format!("Failed to execute command: {:?}", command))?;

    if !status.success() {
        bail!("Command failed with non-zero status: {:?}", command);
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn install_git_internal() -> Result<()> {
    if which("winget").is_ok() {
        run_install_command(Command::new("winget").args(&["install", "--id", "Git.Git", "-e", "--source", "winget"]))
    } else if which("choco").is_ok() {
        run_install_command(Command::new("choco").args(&["install", "git", "-y"]))
    } else {
        bail!("No package manager (winget, choco) found. Please install Git manually from https://git-scm.com/downloads")
    }
}

#[cfg(target_os = "macos")]
fn install_git_internal() -> Result<()> {
    if which("brew").is_ok() {
        run_install_command(Command::new("brew").args(&["install", "git"]))
    } else {
        bail!("Homebrew not found. Please install Git manually.")
    }
}

#[cfg(target_os = "linux")]
fn install_git_internal() -> Result<()> {
    if which("apt-get").is_ok() {
        run_install_command(Command::new("sudo").args(&["apt-get", "update"]))?;
        run_install_command(Command::new("sudo").args(&["apt-get", "install", "-y", "git"]))
    } else {
        bail!("No supported package manager (apt-get) found. Please install Git manually.")
    }
}

fn install_git_lfs_internal() -> Result<()> {
    run_install_command(Command::new("git").args(&["lfs", "install"]))
}

pub fn check_dependencies_background(tx: Sender<DependencyStatus>) {
    if !is_installed("git") {
        tx.send(DependencyStatus::GitMissing).ok();
        return;
    }
    if !is_installed("git-lfs") {
        tx.send(DependencyStatus::GitLfsMissing).ok();
        return;
    }
    tx.send(DependencyStatus::AllOk).ok();
}

pub fn install_dependencies_background(tx: Sender<Result<()>>) {
    let result = (|| -> Result<()> {
        if !is_installed("git") {
            install_git_internal()?;
        }
        if !is_installed("git-lfs") {
            install_git_lfs_internal()?;
        }
        Ok(())
    })();
    tx.send(result).ok();
}