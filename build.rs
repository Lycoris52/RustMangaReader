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

    let pdfium_dll_name = "pdfium.dll"; // for reading pdf
    let dav1d_dll_name = "dav1d.dll"; // for reading avif
    let setting_name = "settings.json";
    let license = "LICENSE";
    // Copy various necessary file
    fs::copy(&manifest_dir.join(pdfium_dll_name), target_dir.join(pdfium_dll_name)).expect("Failed to copy pdfium.dll to target directory");
    fs::copy(&manifest_dir.join(dav1d_dll_name), target_dir.join(dav1d_dll_name)).expect("Failed to copy dav1d.dll to target directory");
    fs::copy(&manifest_dir.join(setting_name), target_dir.join(setting_name)).expect("Failed to copy setting.json to target directory");
    fs::copy(&manifest_dir.join(license), target_dir.join(license)).expect("Failed to copy setting.json to target directory");
}