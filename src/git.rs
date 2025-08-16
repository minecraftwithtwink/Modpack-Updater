pub(crate) use crate::app::GitProgress;
use anyhow::{bail, Context, Result};
use git2::{build::CheckoutBuilder, AnnotatedCommit, Remote, Repository};
use octocrab::Octocrab;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use tokio::runtime::Runtime;

const GIT_REMOTE_URL: &str = "https://github.com/minecraftwithtwink/Twinkcraft-Modpack.git";

// LFS-related structures
#[derive(Serialize)]
struct LfsBatchRequest {
    operation: String,
    transfer: Vec<String>,
    objects: Vec<LfsObject>,
}

#[derive(Serialize, Deserialize)]
struct LfsObject {
    oid: String,
    size: u64,
}

#[derive(Deserialize)]
struct LfsBatchResponse {
    objects: Vec<LfsObjectResponse>,
}

#[derive(Deserialize)]
struct LfsObjectResponse {
    #[allow(dead_code)]
    oid: String,
    #[allow(dead_code)]
    size: u64,
    actions: Option<LfsActions>,
}

#[derive(Deserialize)]
struct LfsActions {
    download: Option<LfsAction>,
}

#[derive(Deserialize)]
struct LfsAction {
    href: String,
    #[allow(dead_code)]
    expires_at: Option<String>,
}

// --- ADDED: A new function to fetch the list of remote branches ---
pub fn fetch_remote_branches_threaded(tx: Sender<Result<Vec<String>>>) {
    let result = (|| -> Result<Vec<String>> {
        let mut remote = Remote::create_detached(GIT_REMOTE_URL)?;
        remote.connect(git2::Direction::Fetch)?;
        let list = remote.list()?;

        let mut branches: Vec<String> = list.iter()
            .filter_map(|head| {
                let name = head.name();
                if name.starts_with("refs/heads/") {
                    Some(name.trim_start_matches("refs/heads/").to_string())
                } else {
                    None
                }
            })
            .collect();

        branches.sort();
        Ok(branches)
    })();
    tx.send(result).ok();
}

// Function to check if a file is an LFS pointer file
fn is_lfs_pointer_file(content: &str) -> Option<(String, u64)> {
    let lines: Vec<&str> = content.lines().collect();
    if lines.len() >= 3
        && lines[0] == "version https://git-lfs.github.com/spec/v1"
        && lines[1].starts_with("oid sha256:")
        && lines[2].starts_with("size ") {

        let oid = lines[1].strip_prefix("oid sha256:").unwrap_or("").to_string();
        let size_str = lines[2].strip_prefix("size ").unwrap_or("0");
        if let Ok(size) = size_str.parse::<u64>() {
            return Some((oid, size));
        }
    }
    None
}

// Function to download LFS files using GitHub API
async fn download_lfs_files_async(repo_path: &Path, branch_name: &str, progress_tx: &Sender<GitProgress>) -> Result<()> {
    progress_tx.send(GitProgress::Update("Scanning for LFS files...".to_string(), 0.0)).ok();

    let octocrab = Octocrab::builder().build()?;
    let owner = "minecraftwithtwink";
    let repo_name = "Twinkcraft-Modpack";

    // Get repository contents recursively to find LFS files
    let mut lfs_files = Vec::new();
    scan_for_lfs_files_recursive(&octocrab, owner, repo_name, branch_name, "", repo_path, &mut lfs_files).await?;

    if lfs_files.is_empty() {
        progress_tx.send(GitProgress::Update("No LFS files found.".to_string(), 1.0)).ok();
        return Ok(());
    }

    progress_tx.send(GitProgress::Update(format!("Found {} LFS files, downloading...", lfs_files.len()), 0.1)).ok();

    // Download LFS files in batches
    for (i, (file_path, oid, size)) in lfs_files.iter().enumerate() {
        let progress = 0.1 + (i as f64 / lfs_files.len() as f64) * 0.9;
        progress_tx.send(GitProgress::Update(format!("Downloading LFS file: {}", file_path), progress)).ok();

        download_single_lfs_file(owner, repo_name, oid, *size, &repo_path.join(file_path)).await?;
    }

    progress_tx.send(GitProgress::Update("LFS files downloaded successfully.".to_string(), 1.0)).ok();
    Ok(())
}

// Recursive function to scan for LFS files in repository
fn scan_for_lfs_files_recursive<'a>(
    octocrab: &'a Octocrab,
    owner: &'a str,
    repo: &'a str,
    branch: &'a str,
    path: &'a str,
    local_repo_path: &'a Path,
    lfs_files: &'a mut Vec<(String, String, u64)>,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + 'a>> {
    Box::pin(async move {
    let contents = octocrab
        .repos(owner, repo)
        .get_content()
        .path(path)
        .r#ref(branch)
        .send()
        .await?;

    for item in contents.items {
        let item_path = if path.is_empty() { item.name.clone() } else { format!("{}/{}", path, item.name) };

        match item.r#type.as_str() {
            "file" => {
                // Check if this file exists locally and is an LFS pointer
                let local_file_path = local_repo_path.join(&item_path);
                if local_file_path.exists() {
                    if let Ok(content) = std::fs::read_to_string(&local_file_path) {
                        if let Some((oid, size)) = is_lfs_pointer_file(&content) {
                            lfs_files.push((item_path, oid, size));
                        }
                    }
                }
            }
            "dir" => {
                // Recursively scan subdirectories
                scan_for_lfs_files_recursive(octocrab, owner, repo, branch, &item_path, local_repo_path, lfs_files).await?;
            }
            _ => {} // Ignore other types
        }
    }

    Ok(())
    })
}

// Function to download a single LFS file
async fn download_single_lfs_file(owner: &str, repo: &str, oid: &str, size: u64, local_path: &Path) -> Result<()> {
    let client = reqwest::Client::new();

    // Create the batch request
    let batch_request = LfsBatchRequest {
        operation: "download".to_string(),
        transfer: vec!["basic".to_string()],
        objects: vec![LfsObject {
            oid: oid.to_string(),
            size,
        }],
    };

    // Make request to LFS batch API
    let lfs_url = format!("https://github.com/{}/{}.git/info/lfs/objects/batch", owner, repo);
    let response = client
        .post(&lfs_url)
        .header("Accept", "application/vnd.git-lfs+json")
        .header("Content-Type", "application/json")
        .json(&batch_request)
        .send()
        .await?;

    if !response.status().is_success() {
        bail!("LFS batch request failed: {}", response.status());
    }

    let batch_response: LfsBatchResponse = response.json().await?;

    if let Some(object) = batch_response.objects.first() {
        if let Some(actions) = &object.actions {
            if let Some(download_action) = &actions.download {
                // Download the actual file
                let file_response = client.get(&download_action.href).send().await?;

                if !file_response.status().is_success() {
                    bail!("Failed to download LFS file: {}", file_response.status());
                }

                let file_content = file_response.bytes().await?;

                // Ensure parent directory exists
                if let Some(parent) = local_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }

                // Write the file
                std::fs::write(local_path, file_content)?;
                return Ok(());
            }
        }
    }

    bail!("No download URL found for LFS file with OID: {}", oid);
}

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
                if dest_path.exists() {
                    fs::remove_dir_all(&dest_path)?;
                }
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
    #[cfg(windows)]
    { PathBuf::from(stripped.replace('/', "\\")) }
    #[cfg(not(windows))]
    { PathBuf::from(stripped.replace('\\', "/")) }
}

// --- MODIFIED: Now accepts a branch_name parameter ---
pub fn perform_git_operations_threaded(path: PathBuf, branch_name: String, progress_tx: Sender<GitProgress>) {
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
        let refspec = format!("+refs/heads/{0}:refs/remotes/origin/{0}", branch_name);
        remote.fetch(&[&refspec], Some(&mut fo), None).context(format!("Failed to fetch. Check network/proxy/branch name ('{}').", branch_name))?;

        progress_tx.send(GitProgress::Update("Analyzing changes...".to_string(), 1.0)).ok();
        let remote_branch_ref_name = format!("refs/remotes/origin/{}", branch_name);
        let fetch_commit = repo.find_reference(&remote_branch_ref_name)?.peel_to_commit().context("Failed to find the latest commit")?;
        let fetch_head: AnnotatedCommit = repo.find_annotated_commit(fetch_commit.id())?;
        let (analysis, _) = repo.merge_analysis(&[&fetch_head])?;

        if analysis.is_up_to_date() {
            progress_tx.send(GitProgress::Update("Repository up-to-date. Verifying files...".to_string(), 1.0)).ok();
            repo.checkout_head(Some(CheckoutBuilder::default().force()))?;
        } else if analysis.is_fast_forward() || repo.head().is_err() {
            progress_tx.send(GitProgress::Update("Applying fast-forward update...".to_string(), 1.0)).ok();
            let local_branch_ref_name = format!("refs/heads/{}", branch_name);
            let mut local_branch_ref = match repo.find_reference(&local_branch_ref_name) {
                Ok(r) => r,
                Err(_) => repo.reference(&local_branch_ref_name, fetch_commit.id(), true, "Create local branch")?,
            };
            local_branch_ref.set_target(fetch_commit.id(), "Fast-forward")?;
            repo.set_head(&local_branch_ref_name)?;
            repo.checkout_head(Some(CheckoutBuilder::default().force()))?;
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
            repo.commit(Some("HEAD"), &signature, &signature, &format!("Merge remote-tracking branch 'origin/{}'", branch_name), &result_tree, &[&our_commit, &fetch_commit])?;
            repo.checkout_head(Some(CheckoutBuilder::default().force()))?;
        }

        clean_managed_directories(&repo, &progress_tx)?;
        force_copy_default_configs(&path, &progress_tx)?;

        // Download LFS files
        let rt = Runtime::new()?;
        rt.block_on(download_lfs_files_async(&path, &branch_name, &progress_tx))?;

        Ok(format!("Successfully updated and verified repository at:\n\n{}\n\nPress Enter to close.", path.display()))
    })();

    match result {
        Ok(msg) => progress_tx.send(GitProgress::Success(msg)).ok(),
        Err(e) => progress_tx.send(GitProgress::Failure(format!("An error occurred:\n\n{:#}", e))).ok(),
    };
}