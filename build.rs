use std::{env, fs};
use std::path::PathBuf;

fn main() {
    if cfg!(target_os = "windows") {
        let mut res = winres::WindowsResource::new();
        res.set_icon("src/assets/icon128.ico"); // Path to your .ico file
        res.compile().unwrap();
    }

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let mut target_dir = out_dir.clone();
    for _ in 0..3 {
        target_dir.pop();
    }

    let dll_name = "pdfium.dll";
    let setting_name = "settings.json";
    // 4. Copy the file if it exists
    fs::copy(&manifest_dir.join(dll_name), target_dir.join(dll_name)).expect("Failed to copy pdfium.dll to target directory");
    fs::copy(&manifest_dir.join(setting_name), target_dir.join(setting_name)).expect("Failed to copy setting.json to target directory");
}