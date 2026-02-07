fn main() {
    if cfg!(target_os = "windows") {
        let mut res = winres::WindowsResource::new();
        res.set_icon("src/assets/icon128.ico"); // Path to your .ico file
        res.compile().unwrap();
    }
}