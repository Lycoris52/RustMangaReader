use crate::config::{GamepadButton, GamepadConfig, MangaAction, MouseButton, UiLanguage};
use crate::localize::tr;
use egui::PointerButton;
use gilrs::Button as GilrsButton;
use std::fs;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use windows::Win32::UI::Shell::StrCmpLogicalW;
use windows::core::PCWSTR;

/// Performs Windows-native natural alphanumeric sorting
pub fn windows_natural_sort(paths: &mut [PathBuf]) {
    paths.sort_by(|a, b| {
        // Convert OsStr to null-terminated Wide Strings (UTF-16) for Windows API
        let a_name: Vec<u16> = a
            .file_name()
            .unwrap_or_default()
            .encode_wide()
            .chain(Some(0))
            .collect();
        let b_name: Vec<u16> = b
            .file_name()
            .unwrap_or_default()
            .encode_wide()
            .chain(Some(0))
            .collect();

        let result = unsafe { StrCmpLogicalW(PCWSTR(a_name.as_ptr()), PCWSTR(b_name.as_ptr())) };

        match result {
            r if r < 0 => std::cmp::Ordering::Less,
            r if r > 0 => std::cmp::Ordering::Greater,
            _ => std::cmp::Ordering::Equal,
        }
    });
}

/// Natural alphanumeric sorting specifically for String vectors
pub fn windows_natural_sort_strings(strings: &mut [String]) {
    strings.sort_by(|a, b| {
        // Convert Strings to null-terminated UTF-16 for the Windows API
        let a_name: Vec<u16> = Path::new(a)
            .file_name()
            .unwrap_or_default()
            .encode_wide()
            .chain(Some(0))
            .collect();
        let b_name: Vec<u16> = Path::new(b)
            .file_name()
            .unwrap_or_default()
            .encode_wide()
            .chain(Some(0))
            .collect();

        let result = unsafe { StrCmpLogicalW(PCWSTR(a_name.as_ptr()), PCWSTR(b_name.as_ptr())) };

        match result {
            r if r < 0 => std::cmp::Ordering::Less,
            r if r > 0 => std::cmp::Ordering::Greater,
            _ => std::cmp::Ordering::Equal,
        }
    });
}

pub fn scan_folder(current_parent: &Path) -> Vec<PathBuf> {
    let mut items = Vec::new();
    if let Ok(entries) = fs::read_dir(current_parent) {
        for entry in entries.flatten() {
            let path = entry.path();
            let is_zip = path.extension().map_or(false, |ext| {
                ext == "zip" || ext == "pdf" || ext == "cbz" || ext == "cbr" || ext == "rar"
            });
            // Treat non-hidden directories as readable manga sources
            let is_dir = path.is_dir()
                && !path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .starts_with('.');

            if is_zip || is_dir {
                items.push(path);
            }
        }
    }
    windows_natural_sort(&mut items);
    items
}

pub fn get_adjacent_directories(
    path_with_file_name: Option<PathBuf>,
) -> (Option<PathBuf>, Option<PathBuf>) {
    // Unwrap the Option to get the actual PathBuf
    let path = match path_with_file_name {
        Some(p) => p,
        None => return (None, None),
    };

    let path = match path.parent() {
        Some(p) => p,
        None => return (None, None),
    };

    let root_dir = match path.parent() {
        Some(p) => p,
        None => return (None, None),
    };

    // Collect all valid sibling directories
    let mut dirs: Vec<PathBuf> = fs::read_dir(root_dir)
        .ok()
        .map(|read_dir| {
            read_dir
                .filter_map(|entry| {
                    let p = entry.ok()?.path();
                    // Ensure it's a directory and not a hidden file
                    if p.is_dir() && !p.file_name()?.to_str()?.starts_with('.') {
                        Some(p)
                    } else {
                        None
                    }
                })
                .collect()
        })
        .unwrap_or_default();

    // Sort using Windows natural alphanumeric order (test2 before test10)
    windows_natural_sort(&mut dirs);

    // Find where we are
    let current_pos = dirs.iter().position(|p| *p == path);

    match current_pos {
        Some(pos) => {
            let prev = if pos > 0 {
                Some(dirs[pos - 1].clone())
            } else {
                None
            };
            let next = if pos + 1 < dirs.len() {
                Some(dirs[pos + 1].clone())
            } else {
                None
            };
            (prev, next)
        }
        None => (None, None),
    }
}

pub fn separator_pct(ui: &mut egui::Ui) {
    let pct = 0.9;

    let spacing = ui.spacing();
    let stroke = ui.visuals().widgets.noninteractive.bg_stroke;

    // Allocate a thin horizontal area (like separator does)
    let desired_h = spacing.scroll.bar_width; // this is usually ~1.0
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), desired_h),
        egui::Sense::hover(),
    );

    // Compute a centered line that's pct of the available width
    let full_w = rect.width();
    let line_w = full_w * pct;

    let indent = 0.0;
    let x0 = rect.left() + indent;
    let x1 = (x0 + line_w).min(rect.right());
    let y = rect.center().y;

    ui.painter()
        .line_segment([egui::pos2(x0, y), egui::pos2(x1, y)], stroke);
}

pub fn render_mouse_action_dropdown(
    ui: &mut egui::Ui,
    id: &str,
    action: &mut MangaAction,
    action_options: &[(MangaAction, String)],
    unassigned_label: &str,
) -> bool {
    let previous = *action;
    egui::ComboBox::from_id_salt(id)
        .selected_text(
            action_options
                .iter()
                .find(|(option, _)| option == action)
                .map(|(_, label)| label.as_str())
                .unwrap_or(unassigned_label),
        )
        .width(150.0)
        .show_ui(ui, |ui| {
            for (option, label) in action_options {
                ui.selectable_value(action, *option, label);
            }
        });

    if *action != previous {
        ui.ctx().request_repaint();
        true
    } else {
        false
    }
}

pub fn language_label(language: UiLanguage) -> &'static str {
    match language {
        UiLanguage::English => tr("language.english"),
        UiLanguage::Japanese => tr("language.japanese"),
    }
}

pub fn action_label(action: MangaAction) -> &'static str {
    match action {
        MangaAction::None => tr("action.none"),
        MangaAction::SlideImageDown => tr("action.slide_image_down"),
        MangaAction::SlideImageUp => tr("action.slide_image_up"),
        MangaAction::ToggleAutoScroll => tr("action.toggle_auto_scroll"),
        MangaAction::ReloadCurrentImage => tr("action.reload_current_image"),
        MangaAction::ZoomIn => tr("action.zoom_in"),
        MangaAction::ZoomOut => tr("action.zoom_out"),
        MangaAction::NextPage => tr("action.next_page"),
        MangaAction::PrevPage => tr("action.prev_page"),
        MangaAction::OneNextPage => tr("action.one_next_page"),
        MangaAction::OnePrevPage => tr("action.one_prev_page"),
        MangaAction::FirstPage => tr("action.first_page"),
        MangaAction::LastPage => tr("action.last_page"),
        MangaAction::NextFile => tr("action.next_file"),
        MangaAction::PrevFile => tr("action.prev_file"),
        MangaAction::NextFolder => tr("action.next_folder"),
        MangaAction::PrevFolder => tr("action.prev_folder"),
        MangaAction::FullScreen => tr("action.fullscreen"),
        MangaAction::ViewMode => tr("action.view_mode"),
        MangaAction::OpenFile => tr("action.open_file"),
        MangaAction::QuitApp => tr("action.quit_app"),
    }
}

pub fn gamepad_button_label(button: GamepadButton) -> &'static str {
    match button {
        GamepadButton::South => tr("settings.gamepad.south"),
        GamepadButton::East => tr("settings.gamepad.east"),
        GamepadButton::North => tr("settings.gamepad.north"),
        GamepadButton::West => tr("settings.gamepad.west"),
        GamepadButton::LeftTrigger => tr("settings.gamepad.left_trigger"),
        GamepadButton::LeftTrigger2 => tr("settings.gamepad.left_trigger2"),
        GamepadButton::RightTrigger => tr("settings.gamepad.right_trigger"),
        GamepadButton::RightTrigger2 => tr("settings.gamepad.right_trigger2"),
        GamepadButton::Select => tr("settings.gamepad.select"),
        GamepadButton::Start => tr("settings.gamepad.start"),
        GamepadButton::LeftThumb => tr("settings.gamepad.left_thumb"),
        GamepadButton::RightThumb => tr("settings.gamepad.right_thumb"),
        GamepadButton::DPadUp => tr("settings.gamepad.dpad_up"),
        GamepadButton::DPadDown => tr("settings.gamepad.dpad_down"),
        GamepadButton::DPadLeft => tr("settings.gamepad.dpad_left"),
        GamepadButton::DPadRight => tr("settings.gamepad.dpad_right"),
    }
}

pub fn strip_adobe_app14_if_invalid(bytes: &[u8]) -> Vec<u8> {
    let mut i = 2;
    let mut out = Vec::with_capacity(bytes.len());

    // Copy SOI first (must be first two bytes)
    if bytes.len() < 2 || bytes[0] != 0xFF || bytes[1] != 0xD8 {
        return bytes.to_vec(); // not a valid jpeg
    }

    out.extend_from_slice(&bytes[0..2]);

    while i < bytes.len() {
        if i + 1 >= bytes.len() {
            break;
        }

        // Every marker must start with FF
        if bytes[i] != 0xFF {
            // Start of entropy data (after SOS)
            out.extend_from_slice(&bytes[i..]);
            break;
        }

        let marker = bytes[i + 1];

        // Standalone markers (no length)
        if marker == 0xD9 || (0xD0..=0xD7).contains(&marker) {
            out.push(0xFF);
            out.push(marker);
            i += 2;
            continue;
        }

        // SOS marker copy rest of file and stop parsing
        if marker == 0xDA {
            out.extend_from_slice(&bytes[i..]);
            break;
        }

        if i + 4 > bytes.len() {
            break;
        }

        let length = u16::from_be_bytes([bytes[i + 2], bytes[i + 3]]) as usize;
        let segment_end = i + 2 + length;

        if segment_end > bytes.len() {
            break;
        }

        // If APP14 (FF EE)
        if marker == 0xEE && length >= 14 && &bytes[i + 4..i + 9] == b"Adobe" {
            println!("Stripping Adobe APP14 segment");
            // Skip this segment entirely
            i = segment_end;
            continue;
        }

        // Otherwise copy full segment
        out.extend_from_slice(&bytes[i..segment_end]);
        i = segment_end;
    }

    out
}

pub fn mouse_button_to_pointer(button: MouseButton) -> PointerButton {
    match button {
        MouseButton::Button1 => PointerButton::Primary,
        MouseButton::Button2 => PointerButton::Secondary,
        MouseButton::Button3 => PointerButton::Middle,
        MouseButton::Button4 => PointerButton::Extra1,
        MouseButton::Button5 => PointerButton::Extra2,
    }
}

pub fn mouse_button_index(button: MouseButton) -> usize {
    match button {
        MouseButton::Button1 => 0,
        MouseButton::Button2 => 1,
        MouseButton::Button3 => 2,
        MouseButton::Button4 => 3,
        MouseButton::Button5 => 4,
    }
}

pub fn map_gamepad_button(button: GilrsButton) -> Option<GamepadButton> {
    match button {
        GilrsButton::South => Some(GamepadButton::South),
        GilrsButton::East => Some(GamepadButton::East),
        GilrsButton::North => Some(GamepadButton::North),
        GilrsButton::West => Some(GamepadButton::West),
        GilrsButton::LeftTrigger => Some(GamepadButton::LeftTrigger),
        GilrsButton::LeftTrigger2 => Some(GamepadButton::LeftTrigger2),
        GilrsButton::RightTrigger => Some(GamepadButton::RightTrigger),
        GilrsButton::RightTrigger2 => Some(GamepadButton::RightTrigger2),
        GilrsButton::Select => Some(GamepadButton::Select),
        GilrsButton::Start => Some(GamepadButton::Start),
        GilrsButton::LeftThumb => Some(GamepadButton::LeftThumb),
        GilrsButton::RightThumb => Some(GamepadButton::RightThumb),
        GilrsButton::DPadUp => Some(GamepadButton::DPadUp),
        GilrsButton::DPadDown => Some(GamepadButton::DPadDown),
        GilrsButton::DPadLeft => Some(GamepadButton::DPadLeft),
        GilrsButton::DPadRight => Some(GamepadButton::DPadRight),
        _ => None,
    }
}

pub fn gamepad_button_index(button: GamepadButton) -> usize {
    match button {
        GamepadButton::South => 0,
        GamepadButton::East => 1,
        GamepadButton::North => 2,
        GamepadButton::West => 3,
        GamepadButton::LeftTrigger => 4,
        GamepadButton::LeftTrigger2 => 5,
        GamepadButton::RightTrigger => 6,
        GamepadButton::RightTrigger2 => 7,
        GamepadButton::Select => 8,
        GamepadButton::Start => 9,
        GamepadButton::LeftThumb => 10,
        GamepadButton::RightThumb => 11,
        GamepadButton::DPadUp => 12,
        GamepadButton::DPadDown => 13,
        GamepadButton::DPadLeft => 14,
        GamepadButton::DPadRight => 15,
    }
}

pub fn gamepad_action_for_button(config: GamepadConfig, button: GamepadButton) -> MangaAction {
    match button {
        GamepadButton::South => config.south,
        GamepadButton::East => config.east,
        GamepadButton::North => config.north,
        GamepadButton::West => config.west,
        GamepadButton::LeftTrigger => config.left_trigger,
        GamepadButton::LeftTrigger2 => config.left_trigger2,
        GamepadButton::RightTrigger => config.right_trigger,
        GamepadButton::RightTrigger2 => config.right_trigger2,
        GamepadButton::Select => config.select,
        GamepadButton::Start => config.start,
        GamepadButton::LeftThumb => config.left_thumb,
        GamepadButton::RightThumb => config.right_thumb,
        GamepadButton::DPadUp => config.dpad_up,
        GamepadButton::DPadDown => config.dpad_down,
        GamepadButton::DPadLeft => config.dpad_left,
        GamepadButton::DPadRight => config.dpad_right,
    }
}

pub fn is_repeatable_gamepad_action(action: MangaAction) -> bool {
    matches!(
        action,
        MangaAction::NextPage
            | MangaAction::SlideImageDown
            | MangaAction::SlideImageUp
            | MangaAction::PrevPage
            | MangaAction::OneNextPage
            | MangaAction::OnePrevPage
            | MangaAction::ZoomIn
            | MangaAction::ZoomOut
            | MangaAction::NextFile
            | MangaAction::PrevFile
            | MangaAction::NextFolder
            | MangaAction::PrevFolder
    )
}
