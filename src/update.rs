use crate::app::UpdateStatus;
use anyhow::Result;
use self_update::backends::github::Update;
use std::sync::mpsc::Sender;

const REPO_OWNER: &str = "minecraftwithtwink";
const REPO_NAME: &str = "Modpack-Updater";

/// Checks for updates in the background and sends the result over a channel.
pub fn check_for_updates_background(tx: Sender<UpdateStatus>) {
    let result = (|| -> Result<UpdateStatus> {
        let latest_release = Update::configure()
            .repo_owner(REPO_OWNER)
            .repo_name(REPO_NAME)
            .bin_name("modpack-updater")
            .current_version(env!("CARGO_PKG_VERSION"))
            .build()?
            .get_latest_release()?;

        // This now correctly checks if the LATEST release version is greater than our CURRENT version.
        if self_update::version::bump_is_greater(env!("CARGO_PKG_VERSION"), &latest_release.version)? {
            Ok(UpdateStatus::UpdateAvailable(latest_release.version))
        } else {
            Ok(UpdateStatus::UpToDate)
        }
    })();

    match result {
        Ok(status) => tx.send(status).ok(),
        Err(_err) => tx.send(UpdateStatus::Error(std::string::String::from("Failed to check for updates"))).ok(),
    };
}

/// Performs the self-update, showing progress to the console.
pub fn perform_update() -> Result<()> {
    Update::configure()
        .repo_owner(REPO_OWNER)
        .repo_name(REPO_NAME)
        .bin_name("modpack-updater")
        .current_version(env!("CARGO_PKG_VERSION"))
        .show_download_progress(true)
        .show_output(true)
        .no_confirm(true)
        .build()?
        .update()?;
    Ok(())
}