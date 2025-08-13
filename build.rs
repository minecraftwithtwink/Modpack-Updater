use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    if cfg!(target_os = "windows") {
        let mut res = winres::WindowsResource::new();
        if Path::new("./assets/icon.ico").exists() {
            res.set_icon("./assets/icon.ico");
        }
        res.compile().expect("Failed to compile Windows resource");
    }

    let target = env::var("TARGET").expect("TARGET environment variable not set");
    let profile = env::var("PROFILE").expect("PROFILE environment variable not set");
    let target_dir = env::var("CARGO_TARGET_DIR").unwrap_or_else(|_| "target".to_string());

    let output_name = match target.as_str() {
        "x86_64-pc-windows-msvc" => Some("modpack-updater-x86_64-pc-windows-msvc.exe"),
        "x86_64-pc-windows-gnu" => Some("modpack-updater-x86_64-pc-windows-gnu.exe"),
        "x86_64-unknown-linux-gnu" => Some("modpack-updater-linux-x86_64"),
        _ => None,
    };

    if let Some(name) = output_name {
        let mut path = PathBuf::from(&target_dir);
        path.push(&profile);
        path.push(target.as_str());

        fs::create_dir_all(&path).expect("Failed to create output directory");

        path.push(name);

        let output_path = path.to_str().expect("Path is not valid UTF-8");

        if target.contains("msvc") {
            println!("cargo:rustc-link-arg=/OUT:{}", output_path);
        } else {
            println!("cargo:rustc-link-arg=-o");
            println!("cargo:rustc-link-arg={}", output_path);
        }
    }
}
