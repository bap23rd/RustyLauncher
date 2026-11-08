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
    pub fn acquire(window_title: &str) -> bool {
        let name = wide("Local\\RustyLauncher.SingleInstance");
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
    pub fn acquire(_window_title: &str) -> bool {
        true
    }
}

pub use imp::acquire;
