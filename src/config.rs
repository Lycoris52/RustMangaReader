use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Copy, Clone, Serialize, Deserialize)]
pub enum ResizeMethod {
    None,       // Use original resolution
    Nearest,    // Nearest Neighbor (fastest)
    Triangle,   // Bilinear (Moderate)
    CatmullRom, // Bicubic
    Lanczos3,   // High Quality
}

impl ResizeMethod {
    pub fn to_filter(self) -> Option<image::imageops::FilterType> {
        match self {
            ResizeMethod::None => None,
            ResizeMethod::Nearest => Some(image::imageops::FilterType::Nearest),
            ResizeMethod::Triangle => Some(image::imageops::FilterType::Triangle),
            ResizeMethod::CatmullRom => Some(image::imageops::FilterType::CatmullRom),
            ResizeMethod::Lanczos3 => Some(image::imageops::FilterType::Lanczos3),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq)]
pub struct Shortcut {
    pub key: egui::Key,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

impl Shortcut {
    // Helper to create a new shortcut easily
    fn new(key: egui::Key, ctrl: bool, alt: bool, shift: bool) -> Self {
        Self { key, ctrl, alt, shift }
    }

    // Helper to format the name for the UI (e.g., "Ctrl + O")
    pub fn format(&self) -> String {
        let mut parts = vec![];
        if self.ctrl { parts.push("Ctrl"); }
        if self.alt { parts.push("Alt"); }
        if self.shift { parts.push("Shift"); }
        let temp = format!("{:?}", self.key);
        parts.push(&temp);
        parts.join(" + ")
    }
}

#[derive(PartialEq)]
pub enum MangaAction {
    None,
    NextPage,
    PrevPage,
    FirstPage,
    LastPage,
    NextFile,
    PrevFile,
    FullScreen,
    ViewMode,
    OpenFile,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq)]
pub struct KeyConfig {
    pub next_page: Shortcut,
    pub prev_page: Shortcut,
    pub first_page: Shortcut,
    pub last_page: Shortcut,
    pub next_file: Shortcut,
    pub prev_file: Shortcut,
    pub fullscreen: Shortcut,
    pub view_mode: Shortcut,
    pub open_file: Shortcut,
}

impl Default for KeyConfig {
    fn default() -> Self {
        Self {
            next_page: Shortcut::new(egui::Key::ArrowLeft, false, false, false),
            prev_page: Shortcut::new(egui::Key::ArrowRight, false, false, false),
            first_page: Shortcut::new(egui::Key::Home, false, false, false),
            last_page: Shortcut::new(egui::Key::End, false, false, false),
            next_file: Shortcut::new(egui::Key::ArrowLeft, true, false, false),
            prev_file: Shortcut::new(egui::Key::ArrowRight, true, false, false),
            fullscreen: Shortcut::new(egui::Key::Enter, true, false, false),
            view_mode: Shortcut::new(egui::Key::Enter, false, false, false),
            open_file: Shortcut::new(egui::Key::O, false, false, false),
        }
    }
}


#[derive(Serialize, Deserialize)]
pub struct AppSettings {
    pub resize_method: ResizeMethod,
    pub settings_width: f32,
    pub show_settings: bool,
    pub skip_folder: bool,
    pub transparency_support: bool,
    pub enable_single_file_caching: bool,
    pub keys: KeyConfig,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            resize_method: ResizeMethod::Triangle,
            settings_width: 250.0,
            show_settings: false,
            skip_folder: true,
            transparency_support: false,
            enable_single_file_caching: true,
            keys: KeyConfig::default(),
        }
    }
}