// in build.rs
use std::path::Path;

fn main() {
    if cfg!(target_os = "windows") {
        let mut res = winres::WindowsResource::new();
        if Path::new("./assets/icon.ico").exists() {
            res.set_icon("./assets/icon.ico");
        }
        res.compile().unwrap();
    }
}