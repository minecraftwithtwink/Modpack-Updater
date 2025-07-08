pub(crate) use crate::app::GitProgress;
use anyhow::{bail, Context, Result};
use git2::{build::CheckoutBuilder, AnnotatedCommit, Repository};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;

const GIT_REMOTE_URL: &str = "https://github.com/minecraftwithtwink/Twinkcraft-Modpack.git";
const GIT_BRANCH_NAME: &str = "main";

fn clean_managed_directories(repo: &Repository, progress_tx: &Sender<GitProgress>) -> Result<()> {
    const DIRS_TO_CLEAN: &[&str] = &[
        "mods",
        "kubejs",
        "configureddefaults",
        "resourcepacks",
        "patchouli_books",
        "datapacks",
    ];

    progress_tx.send(GitProgress::Update("Cleaning managed directories...".to_string(), 1.0)).ok();

    for dir_name in DIRS_TO_CLEAN {
        let mut builder = CheckoutBuilder::new();
        builder.force().remove_untracked(true).path(dir_name);
        repo.checkout_head(Some(&mut builder)).context(format!("Failed to clean the '{}' directory.", dir_name))?;
    }
    Ok(())
}

// --- REWRITTEN: This function now uses only the Rust standard library for maximum compatibility ---
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
            if let Some(parent) = dest_path.parent() {
                fs::create_dir_all(parent)?;
            }

            if *is_directory {
                // Remove the old directory first to ensure a clean copy
                if dest_path.exists() {
                    fs::remove_dir_all(&dest_path)?;
                }
                // Now, perform a recursive copy
                copy_dir_all(&source_path, &dest_path)?;
            } else {
                fs::copy(&source_path, &dest_path)?;
            }
        }
    }

    Ok(())
}

// --- ADDED: A helper function for recursive directory copy using std::fs ---
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
    #[cfg(windows)]
    { PathBuf::from(stripped.replace('/', "\\")) }
    #[cfg(not(windows))]
    { PathBuf::from(stripped.replace('\\', "/")) }
}

pub fn perform_git_operations_threaded(path: PathBuf, progress_tx: Sender<GitProgress>) {
    let result = (|| -> Result<String> {
        let mut callbacks = git2::RemoteCallbacks::new();
        let tx = progress_tx.clone();
        callbacks.transfer_progress(move |stats| {
            let received = stats.received_objects();
            let total = stats.total_objects();
            let ratio = if total > 0 { received as f64 / total as f64 } else { 0.0 };
            let mb = 1024 * 1024;
            let received_mb = stats.received_bytes() / mb;
            let message = format!("Downloading objects: {} / {} ({} MB)", received, total, received_mb);
            tx.send(GitProgress::Update(message, ratio)).is_ok()
        });

        progress_tx.send(GitProgress::Update("Setting up remote...".to_string(), 0.0)).ok();
        let mut fo = git2::FetchOptions::new();
        fo.remote_callbacks(callbacks);
        let mut proxy_opts = git2::ProxyOptions::new();
        proxy_opts.auto();
        fo.proxy_options(proxy_opts);

        let repo = match Repository::open(&path) {
            Ok(repo) => repo,
            Err(_) => Repository::init(&path)?,
        };
        repo.remote_set_url("origin", GIT_REMOTE_URL).context("Failed to set remote URL")?;
        let mut remote = repo.find_remote("origin").context("Failed to find remote 'origin'")?;

        progress_tx.send(GitProgress::Update("Fetching from remote...".to_string(), 0.0)).ok();
        let refspec = format!("+refs/heads/{}:refs/remotes/origin/{}", GIT_BRANCH_NAME, GIT_BRANCH_NAME);
        remote.fetch(&[refspec], Some(&mut fo), None).context("Failed to fetch. Check network/proxy/branch name ('main').")?;

        progress_tx.send(GitProgress::Update("Analyzing changes...".to_string(), 1.0)).ok();
        let remote_branch_ref_name = format!("refs/remotes/origin/{}", GIT_BRANCH_NAME);
        let fetch_commit = repo.find_reference(&remote_branch_ref_name)?.peel_to_commit().context("Failed to find the latest commit")?;
        let fetch_head: AnnotatedCommit = repo.find_annotated_commit(fetch_commit.id())?;
        let (analysis, _) = repo.merge_analysis(&[&fetch_head])?;

        // --- MODIFIED: Restructured logic to always run the final steps ---
        if analysis.is_up_to_date() {
            // It's up-to-date, but we still force the cleanup and config copy.
            progress_tx.send(GitProgress::Update("Repository up-to-date. Verifying files...".to_string(), 1.0)).ok();
            repo.checkout_head(Some(git2::build::CheckoutBuilder::default().force()))?;
        } else if analysis.is_fast_forward() || repo.head().is_err() {
            progress_tx.send(GitProgress::Update("Applying fast-forward update...".to_string(), 1.0)).ok();
            let local_branch_ref_name = format!("refs/heads/{}", GIT_BRANCH_NAME);
            let mut local_branch_ref = match repo.find_reference(&local_branch_ref_name) {
                Ok(r) => r,
                Err(_) => repo.reference(&local_branch_ref_name, fetch_commit.id(), true, "Create local branch")?,
            };
            local_branch_ref.set_target(fetch_commit.id(), "Fast-forward")?;
            repo.set_head(&local_branch_ref_name)?;
            repo.checkout_head(Some(git2::build::CheckoutBuilder::default().force()))?;
        } else {
            progress_tx.send(GitProgress::Update("Merging changes...".to_string(), 1.0)).ok();
            let our_commit = repo.head()?.peel_to_commit()?;
            let merge_base_oid = repo.merge_base(our_commit.id(), fetch_commit.id())?;
            let merge_base_commit = repo.find_commit(merge_base_oid)?;
            let mut index = repo.merge_trees(&merge_base_commit.tree()?, &our_commit.tree()?, &fetch_commit.tree()?, None)?;
            if index.has_conflicts() {
                bail!("Merge conflict detected! Please resolve manually.");
            }
            let result_tree_id = index.write_tree_to(&repo)?;
            let result_tree = repo.find_tree(result_tree_id)?;
            let signature = git2::Signature::now("Modpack Updater", "updater@example.com")?;
            repo.commit(Some("HEAD"), &signature, &signature, &format!("Merge remote-tracking branch 'origin/{}'", GIT_BRANCH_NAME), &result_tree, &[&our_commit, &fetch_commit])?;
            repo.checkout_head(Some(git2::build::CheckoutBuilder::default().force()))?;
        }

        // These crucial steps now run on EVERY successful path.
        clean_managed_directories(&repo, &progress_tx)?;
        force_copy_default_configs(&path, &progress_tx)?;

        Ok(format!("Successfully updated and verified repository at:\n\n{}\n\nPress Enter to close.", path.display()))
    })();

    match result {
        Ok(msg) => progress_tx.send(GitProgress::Success(msg)).ok(),
        Err(e) => progress_tx.send(GitProgress::Failure(format!("An error occurred:\n\n{:#}", e))).ok(),
    };
}