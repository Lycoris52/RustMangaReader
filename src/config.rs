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
    TopDown,  // vertical web manga view
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
        Self::Japanese
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
    SlideImageDown,
    SlideImageUp,
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
    pub const ALL: [Self; 17] = [
        Self::None,
        Self::SlideImageDown,
        Self::SlideImageUp,
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
#[serde(default)]
pub struct KeyConfig {
    #[serde(default)]
    pub slide_image_down: Option<Shortcut>,
    #[serde(default)]
    pub slide_image_up: Option<Shortcut>,
    pub next_page: Option<Shortcut>,
    pub prev_page: Option<Shortcut>,
    pub one_next_page: Option<Shortcut>,
    pub one_prev_page: Option<Shortcut>,
    pub first_page: Option<Shortcut>,
    pub last_page: Option<Shortcut>,
    pub next_file: Option<Shortcut>,
    pub prev_file: Option<Shortcut>,
    pub next_folder: Option<Shortcut>,
    pub prev_folder: Option<Shortcut>,
    pub fullscreen: Option<Shortcut>,
    pub view_mode: Option<Shortcut>,
    pub open_file: Option<Shortcut>,
    pub quit_app: Option<Shortcut>,
}

impl Default for KeyConfig {
    fn default() -> Self {
        Self {
            slide_image_down: None,
            slide_image_up: None,
            next_page: Some(Shortcut::new(egui::Key::ArrowLeft, false, false, false)),
            prev_page: Some(Shortcut::new(egui::Key::ArrowRight, false, false, false)),
            one_next_page: Some(Shortcut::new(egui::Key::ArrowLeft, false, false, true)),
            one_prev_page: Some(Shortcut::new(egui::Key::ArrowRight, false, false, true)),
            first_page: Some(Shortcut::new(egui::Key::Home, false, false, false)),
            last_page: Some(Shortcut::new(egui::Key::End, false, false, false)),
            next_file: Some(Shortcut::new(egui::Key::ArrowDown, false, false, false)),
            prev_file: Some(Shortcut::new(egui::Key::ArrowUp, false, false, false)),
            next_folder: Some(Shortcut::new(egui::Key::ArrowLeft, true, false, false)),
            prev_folder: Some(Shortcut::new(egui::Key::ArrowRight, true, false, false)),
            fullscreen: Some(Shortcut::new(egui::Key::Enter, true, false, false)),
            view_mode: Some(Shortcut::new(egui::Key::Enter, false, false, false)),
            open_file: Some(Shortcut::new(egui::Key::O, false, false, false)),
            quit_app: Some(Shortcut::new(egui::Key::Escape, false, false, false)),
        }
    }
}

impl KeyConfig {
    pub fn top_down_default() -> Self {
        Self {
            slide_image_down: Some(Shortcut::new(egui::Key::ArrowDown, false, false, false)),
            slide_image_up: Some(Shortcut::new(egui::Key::ArrowUp, false, false, false)),
            next_page: Some(Shortcut::new(egui::Key::ArrowLeft, false, false, false)),
            prev_page: Some(Shortcut::new(egui::Key::ArrowRight, false, false, false)),
            one_next_page: None,
            one_prev_page: None,
            first_page: Some(Shortcut::new(egui::Key::Home, false, false, false)),
            last_page: Some(Shortcut::new(egui::Key::End, false, false, false)),
            next_file: Some(Shortcut::new(egui::Key::ArrowLeft, false, false, false)),
            prev_file: Some(Shortcut::new(egui::Key::ArrowRight, false, false, false)),
            next_folder: Some(Shortcut::new(egui::Key::ArrowLeft, true, false, false)),
            prev_folder: Some(Shortcut::new(egui::Key::ArrowRight, true, false, false)),
            fullscreen: Some(Shortcut::new(egui::Key::Enter, true, false, false)),
            view_mode: Some(Shortcut::new(egui::Key::Enter, false, false, false)),
            open_file: Some(Shortcut::new(egui::Key::O, false, false, false)),
            quit_app: Some(Shortcut::new(egui::Key::Escape, false, false, false)),
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

impl MouseConfig {
    pub fn top_down_default() -> Self {
        Self {
            scroll_up: MangaAction::SlideImageUp,
            scroll_down: MangaAction::SlideImageDown,
            ..Self::default()
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

impl GamepadConfig {
    pub fn top_down_default() -> Self {
        Self {
            dpad_up: MangaAction::SlideImageUp,
            dpad_down: MangaAction::SlideImageDown,
            ..Self::default()
        }
    }
}

fn default_double_click_threshold_ms() -> u64 {
    160
}

fn default_image_panel_background() -> [u8; 4] {
    [40, 40, 40, 255]
}

fn default_top_down_keys() -> KeyConfig {
    KeyConfig::top_down_default()
}

fn default_top_down_mouse() -> MouseConfig {
    MouseConfig::top_down_default()
}

fn default_top_down_gamepad() -> GamepadConfig {
    GamepadConfig::top_down_default()
}

fn default_top_down_image_slide_speed() -> f32 {
    0.05
}

fn default_top_down_image_drag_speed() -> f32 {
    3.0
}

fn default_settings_button_y_offset() -> f32 {
    0.0
}

fn default_settings_button_width() -> f32 {
    20.0
}

fn default_settings_button_x_offset() -> f32 {
    0.0
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
    #[serde(default)]
    pub auto_hide_settings_button: bool,
    pub transparency_support: bool,
    pub enable_single_file_caching: bool,
    pub image_delay: u64,
    pub keys: KeyConfig,
    #[serde(default = "default_top_down_keys")]
    pub top_down_keys: KeyConfig,
    #[serde(default)]
    pub mouse: MouseConfig,
    #[serde(default = "default_top_down_mouse")]
    pub top_down_mouse: MouseConfig,
    #[serde(default)]
    pub gamepad: GamepadConfig,
    #[serde(default = "default_top_down_gamepad")]
    pub top_down_gamepad: GamepadConfig,
    #[serde(default = "default_double_click_threshold_ms")]
    pub double_click_threshold_ms: u64,
    #[serde(default = "default_double_click_threshold_ms")]
    pub top_down_double_click_threshold_ms: u64,
    #[serde(default = "default_top_down_image_slide_speed")]
    pub top_down_image_slide_speed: f32,
    #[serde(default = "default_top_down_image_drag_speed")]
    pub top_down_image_drag_speed: f32,
    #[serde(default = "default_settings_button_width")]
    pub settings_button_width: f32,
    #[serde(default = "default_settings_button_x_offset")]
    pub settings_button_x_offset: f32,
    #[serde(default = "default_settings_button_y_offset")]
    pub settings_button_y_offset: f32,
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
            language: UiLanguage::Japanese,
            settings_width: 300.0,
            show_settings: false,
            auto_hide_settings_button: true,
            transparency_support: false,
            enable_single_file_caching: true,
            image_delay: 0,
            keys: KeyConfig::default(),
            top_down_keys: KeyConfig::top_down_default(),
            mouse: MouseConfig::default(),
            top_down_mouse: MouseConfig::top_down_default(),
            gamepad: GamepadConfig::default(),
            top_down_gamepad: GamepadConfig::top_down_default(),
            double_click_threshold_ms: 160,
            top_down_double_click_threshold_ms: 160,
            top_down_image_slide_speed: default_top_down_image_slide_speed(),
            top_down_image_drag_speed: default_top_down_image_drag_speed(),
            settings_button_width: default_settings_button_width(),
            settings_button_x_offset: default_settings_button_x_offset(),
            settings_button_y_offset: default_settings_button_y_offset(),
            show_top_bar: true,
            enable_auto_image_byte_fix: true,
            last_page_action: LastPageAction::GotoNextFile,
        }
    }
}
