use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum SourceMode {
    Zip,
    Folder,
    Pdf,
    Rar,
}

#[derive(Debug, PartialEq, Copy, Clone, Serialize, Deserialize)]
pub enum PageViewOptions {
    Single,   // single page
    DoubleRL, // double page from right to left
    DoubleLR, // double page from left to right
}

#[derive(Debug, PartialEq, Copy, Clone, Serialize, Deserialize)]
pub enum LastPageAction {
    GotoNextFile,
    ToFirstPage,
    Nothing,
}

#[derive(Debug, PartialEq, Eq, Copy, Clone, Serialize, Deserialize)]
pub enum UiLanguage {
    English,
    Japanese,
}

impl Default for UiLanguage {
    fn default() -> Self {
        Self::English
    }
}

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

#[derive(Debug, PartialEq, Copy, Clone, Serialize, Deserialize)]
pub enum ImageSizingMode {
    FitHeight,
    FitWidth,
    OriginalSize,
}

impl Default for ImageSizingMode {
    fn default() -> Self {
        Self::FitHeight
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
        Self {
            key,
            ctrl,
            alt,
            shift,
        }
    }

    // Helper to format the name for the UI (e.g., "Ctrl + O")
    pub fn format(&self) -> String {
        let mut parts = vec![];
        if self.ctrl {
            parts.push("Ctrl");
        }
        if self.alt {
            parts.push("Alt");
        }
        if self.shift {
            parts.push("Shift");
        }
        let temp = format!("{:?}", self.key);
        parts.push(&temp);
        parts.join(" + ")
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Deserialize)]
pub enum MangaAction {
    None,
    NextPage,
    PrevPage,
    OneNextPage,
    OnePrevPage,
    FirstPage,
    LastPage,
    NextFile,
    PrevFile,
    NextFolder,
    PrevFolder,
    FullScreen,
    ViewMode,
    OpenFile,
    QuitApp,
}

impl MangaAction {
    pub const ALL: [Self; 15] = [
        Self::None,
        Self::NextPage,
        Self::PrevPage,
        Self::OneNextPage,
        Self::OnePrevPage,
        Self::FirstPage,
        Self::LastPage,
        Self::NextFile,
        Self::PrevFile,
        Self::NextFolder,
        Self::PrevFolder,
        Self::FullScreen,
        Self::ViewMode,
        Self::OpenFile,
        Self::QuitApp,
    ];
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Deserialize)]
pub enum MouseButton {
    Button1,
    Button2,
    Button3,
    Button4,
    Button5,
}

impl MouseButton {
    pub const ALL: [Self; 5] = [
        Self::Button1,
        Self::Button2,
        Self::Button3,
        Self::Button4,
        Self::Button5,
    ];
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Deserialize)]
pub enum MouseGesture {
    Unassigned,
    Click(MouseButton),
    DoubleClick(MouseButton),
    LongClick(MouseButton),
    ScrollUp,
    ScrollDown,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Deserialize)]
pub enum GamepadButton {
    South,
    East,
    North,
    West,
    LeftTrigger,
    LeftTrigger2,
    RightTrigger,
    RightTrigger2,
    Select,
    Start,
    LeftThumb,
    RightThumb,
    DPadUp,
    DPadDown,
    DPadLeft,
    DPadRight,
}

impl GamepadButton {
    pub const ALL: [Self; 16] = [
        Self::South,
        Self::East,
        Self::North,
        Self::West,
        Self::LeftTrigger,
        Self::LeftTrigger2,
        Self::RightTrigger,
        Self::RightTrigger2,
        Self::Select,
        Self::Start,
        Self::LeftThumb,
        Self::RightThumb,
        Self::DPadUp,
        Self::DPadDown,
        Self::DPadLeft,
        Self::DPadRight,
    ];
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq)]
pub struct KeyConfig {
    pub next_page: Shortcut,
    pub prev_page: Shortcut,
    pub one_next_page: Shortcut,
    pub one_prev_page: Shortcut,
    pub first_page: Shortcut,
    pub last_page: Shortcut,
    pub next_file: Shortcut,
    pub prev_file: Shortcut,
    pub next_folder: Shortcut,
    pub prev_folder: Shortcut,
    pub fullscreen: Shortcut,
    pub view_mode: Shortcut,
    pub open_file: Shortcut,
    pub quit_app: Shortcut,
}

impl Default for KeyConfig {
    fn default() -> Self {
        Self {
            next_page: Shortcut::new(egui::Key::ArrowLeft, false, false, false),
            prev_page: Shortcut::new(egui::Key::ArrowRight, false, false, false),
            one_next_page: Shortcut::new(egui::Key::ArrowLeft, false, false, true),
            one_prev_page: Shortcut::new(egui::Key::ArrowRight, false, false, true),
            first_page: Shortcut::new(egui::Key::Home, false, false, false),
            last_page: Shortcut::new(egui::Key::End, false, false, false),
            next_file: Shortcut::new(egui::Key::ArrowDown, false, false, false),
            prev_file: Shortcut::new(egui::Key::ArrowUp, false, false, false),
            next_folder: Shortcut::new(egui::Key::ArrowLeft, true, false, false),
            prev_folder: Shortcut::new(egui::Key::ArrowRight, true, false, false),
            fullscreen: Shortcut::new(egui::Key::Enter, true, false, false),
            view_mode: Shortcut::new(egui::Key::Enter, false, false, false),
            open_file: Shortcut::new(egui::Key::O, false, false, false),
            quit_app: Shortcut::new(egui::Key::Escape, false, false, false),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq)]
#[serde(default)]
pub struct MouseConfig {
    pub scroll_up: MangaAction,
    pub scroll_down: MangaAction,
    pub button1_click: MangaAction,
    pub button2_click: MangaAction,
    pub button3_click: MangaAction,
    pub button4_click: MangaAction,
    pub button5_click: MangaAction,
    pub button1_double_click: MangaAction,
    pub button2_double_click: MangaAction,
    pub button3_double_click: MangaAction,
    pub button4_double_click: MangaAction,
    pub button5_double_click: MangaAction,
    pub button1_long_click: MangaAction,
    pub button2_long_click: MangaAction,
    pub button3_long_click: MangaAction,
    pub button4_long_click: MangaAction,
    pub button5_long_click: MangaAction,
}

impl Default for MouseConfig {
    fn default() -> Self {
        Self {
            scroll_up: MangaAction::PrevPage,
            scroll_down: MangaAction::NextPage,
            button1_click: MangaAction::NextPage,
            button2_click: MangaAction::PrevPage,
            button3_click: MangaAction::ViewMode,
            button4_click: MangaAction::None,
            button5_click: MangaAction::None,
            button1_double_click: MangaAction::FullScreen,
            button2_double_click: MangaAction::None,
            button3_double_click: MangaAction::None,
            button4_double_click: MangaAction::None,
            button5_double_click: MangaAction::None,
            button1_long_click: MangaAction::OpenFile,
            button2_long_click: MangaAction::None,
            button3_long_click: MangaAction::None,
            button4_long_click: MangaAction::None,
            button5_long_click: MangaAction::None,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq)]
#[serde(default)]
pub struct GamepadConfig {
    pub south: MangaAction,
    pub east: MangaAction,
    pub north: MangaAction,
    pub west: MangaAction,
    pub left_trigger: MangaAction,
    pub left_trigger2: MangaAction,
    pub right_trigger: MangaAction,
    pub right_trigger2: MangaAction,
    pub select: MangaAction,
    pub start: MangaAction,
    pub left_thumb: MangaAction,
    pub right_thumb: MangaAction,
    pub dpad_up: MangaAction,
    pub dpad_down: MangaAction,
    pub dpad_left: MangaAction,
    pub dpad_right: MangaAction,
}

impl Default for GamepadConfig {
    fn default() -> Self {
        Self {
            south: MangaAction::ViewMode,
            east: MangaAction::PrevPage,
            north: MangaAction::None,
            west: MangaAction::NextPage,
            left_trigger: MangaAction::PrevFile,
            left_trigger2: MangaAction::PrevFolder,
            right_trigger: MangaAction::NextFile,
            right_trigger2: MangaAction::NextFolder,
            select: MangaAction::QuitApp,
            start: MangaAction::OpenFile,
            left_thumb: MangaAction::FirstPage,
            right_thumb: MangaAction::LastPage,
            dpad_up: MangaAction::PrevFile,
            dpad_down: MangaAction::NextFile,
            dpad_left: MangaAction::NextPage,
            dpad_right: MangaAction::PrevPage,
        }
    }
}

fn default_double_click_threshold_ms() -> u64 {
    160
}

fn default_image_panel_background() -> [u8; 4] {
    [40, 40, 40, 255]
}

#[derive(Serialize, Deserialize)]
pub struct AppSettings {
    pub resize_method: ResizeMethod,
    pub page_view_options: PageViewOptions,
    #[serde(default)]
    pub image_sizing_mode: ImageSizingMode,
    #[serde(default)]
    pub spread_center_offset: f32,
    #[serde(default = "default_image_panel_background")]
    pub image_panel_background: [u8; 4],
    #[serde(default)]
    pub language: UiLanguage,
    pub settings_width: f32,
    pub show_settings: bool,
    pub transparency_support: bool,
    pub enable_single_file_caching: bool,
    pub image_delay: u64,
    pub keys: KeyConfig,
    #[serde(default)]
    pub mouse: MouseConfig,
    #[serde(default)]
    pub gamepad: GamepadConfig,
    #[serde(default = "default_double_click_threshold_ms")]
    pub double_click_threshold_ms: u64,
    pub show_top_bar: bool,
    pub enable_auto_image_byte_fix: bool,
    pub last_page_action: LastPageAction,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            resize_method: ResizeMethod::Triangle,
            page_view_options: PageViewOptions::DoubleRL,
            image_sizing_mode: ImageSizingMode::FitHeight,
            spread_center_offset: 0.0,
            image_panel_background: default_image_panel_background(),
            language: UiLanguage::English,
            settings_width: 300.0,
            show_settings: false,
            transparency_support: false,
            enable_single_file_caching: true,
            image_delay: 0,
            keys: KeyConfig::default(),
            mouse: MouseConfig::default(),
            gamepad: GamepadConfig::default(),
            double_click_threshold_ms: 160,
            show_top_bar: true,
            enable_auto_image_byte_fix: true,
            last_page_action: LastPageAction::GotoNextFile,
        }
    }
}
