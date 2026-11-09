//! Only one RustyLauncher at a time. A second launch brings the first window to the front and exits.

#[cfg(windows)]
mod imp {
    use windows_sys::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError};
    use windows_sys::Win32::System::Threading::CreateMutexW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        FindWindowW, IsIconic, SW_RESTORE, SetForegroundWindow, ShowWindow,
    };

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// Returns `true` if this is the first instance (and keeps the guard for the process lifetime).
    /// Returns `false` if another instance already exists, after trying to focus its window.
    pub fn acquire(window_title: &str, data_dir_key: Option<&str>) -> bool {
        // Normal runs share one guard. A run with its own data folder (RL_DATA_DIR) gets its own,
        // so a test copy never collides with the copy you are using.
        let name = match data_dir_key {
            None => "Local\\RustyLauncher.SingleInstance".to_string(),
            Some(dir) => {
                let hash = dir
                    .to_lowercase()
                    .bytes()
                    .fold(0xcbf29ce484222325u64, |h, b| {
                        (h ^ u64::from(b)).wrapping_mul(0x100000001b3)
                    });
                format!("Local\\RustyLauncher.SingleInstance.{hash:x}")
            }
        };
        let name = wide(&name);
        // SAFETY: plain Win32 calls with valid, NUL-terminated wide strings. The mutex handle is
        // deliberately never closed: it must live as long as the process.
        unsafe {
            let handle = CreateMutexW(std::ptr::null(), 0, name.as_ptr());
            if handle.is_null() {
                // Could not create the guard; run anyway rather than refuse to start.
                return true;
            }
            if GetLastError() != ERROR_ALREADY_EXISTS {
                return true;
            }
            let title = wide(window_title);
            let hwnd = FindWindowW(std::ptr::null(), title.as_ptr());
            if !hwnd.is_null() {
                if IsIconic(hwnd) != 0 {
                    ShowWindow(hwnd, SW_RESTORE);
                }
                SetForegroundWindow(hwnd);
            }
            false
        }
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn acquire(_window_title: &str, _data_dir_key: Option<&str>) -> bool {
        true
    }
}

pub use imp::acquire;
