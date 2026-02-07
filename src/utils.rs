use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use windows::core::PCWSTR;
use windows::Win32::UI::Shell::StrCmpLogicalW;

/// Performs Windows-native natural alphanumeric sorting
pub fn windows_natural_sort(paths: &mut [PathBuf]) {
    paths.sort_by(|a, b| {
        // Convert OsStr to null-terminated Wide Strings (UTF-16) for Windows API
        let a_name: Vec<u16> = a.file_name().unwrap_or_default().encode_wide().chain(Some(0)).collect();
        let b_name: Vec<u16> = b.file_name().unwrap_or_default().encode_wide().chain(Some(0)).collect();

        let result = unsafe {
            StrCmpLogicalW(PCWSTR(a_name.as_ptr()), PCWSTR(b_name.as_ptr()))
        };

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
        let a_name: Vec<u16> = Path::new(a).file_name().unwrap_or_default().encode_wide().chain(Some(0)).collect();
        let b_name: Vec<u16> = Path::new(b).file_name().unwrap_or_default().encode_wide().chain(Some(0)).collect();

        let result = unsafe {
            StrCmpLogicalW(PCWSTR(a_name.as_ptr()), PCWSTR(b_name.as_ptr()))
        };

        match result {
            r if r < 0 => std::cmp::Ordering::Less,
            r if r > 0 => std::cmp::Ordering::Greater,
            _ => std::cmp::Ordering::Equal,
        }
    });
}