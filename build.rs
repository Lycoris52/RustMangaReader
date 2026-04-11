use std::path::PathBuf;
use std::{env, fs};

fn main() {
    if cfg!(target_os = "windows") {
        let mut res = winres::WindowsResource::new();
        res.set_icon("src/assets/icon256.ico"); // Path to your .ico file
        res.compile().unwrap();
    }

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let mut target_dir = out_dir.clone();
    for _ in 0..3 {
        target_dir.pop();
    }

    let required_files = [
        ("pdfium.dll", "pdfium.dll"),
        ("dav1d.dll", "dav1d.dll"),
        ("settings.json", "settings.json"),
        ("LICENSE", "LICENSE"),
    ];

    for (file_name, label) in required_files {
        fs::copy(manifest_dir.join(file_name), target_dir.join(file_name))
            .unwrap_or_else(|_| panic!("Failed to copy {label} to target directory"));
    }
}
