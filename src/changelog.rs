use anyhow::Result;
use std::sync::mpsc::Sender;

const CHANGELOG_URL: &str = "https://raw.githubusercontent.com/minecraftwithtwink/Modpack-Updater/main/CHANGELOG.md";

/// Fetches the changelog content from GitHub in a background thread.
pub fn fetch_changelog_background(tx: Sender<Result<String>>) {
    let result = (|| -> Result<String> {
        let response = reqwest::blocking::get(CHANGELOG_URL)?;
        let content = response.text()?;
        Ok(content)
    })();
    tx.send(result).ok();
}