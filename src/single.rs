//! Single-instance guard via a named mutex. A second launch brings the running
//! instance's window to the front (found by title, across processes) and exits.

use windows::core::w;
use windows::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE, HWND};
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONINFORMATION, MB_OK};

const MUTEX_NAME: windows::core::PCWSTR = w!("Local\\ChatMix_Singleton_v1");

/// Held for the process lifetime; releases the mutex on exit.
pub struct Instance {
    mutex: HANDLE,
}

impl Drop for Instance {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.mutex);
        }
    }
}

/// `Some(Instance)` for the first instance. A later instance surfaces the
/// running window (or shows a message box) and returns `None` so main() exits.
pub fn acquire() -> Option<Instance> {
    unsafe {
        let mutex = CreateMutexW(None, false, MUTEX_NAME).ok()?;
        if GetLastError() == ERROR_ALREADY_EXISTS {
            if !crate::tray::show_window() {
                message_box();
            }
            let _ = CloseHandle(mutex);
            return None;
        }
        Some(Instance { mutex })
    }
}

fn message_box() {
    unsafe {
        MessageBoxW(
            HWND::default(),
            w!("ChatMix is already running.\nCheck your system tray (bottom-right)."),
            w!("ChatMix"),
            MB_OK | MB_ICONINFORMATION,
        );
    }
}
