//! Global low-level keyboard hook that intercepts the volume media keys.
//!
//! While interception is enabled, `VolumeUp`/`VolumeDown` key presses are
//! swallowed (the OS never changes the volume) and a message is posted to the
//! application window so the brightness change happens on the normal message
//! loop.
//!
//! Important: a `WH_KEYBOARD_LL` callback must return quickly (within
//! `LowLevelHooksTimeout`, ~300 ms) and must not perform slow, cross-process
//! work such as WMI calls. So the callback does nothing but post a message and
//! return; the actual brightness step is done in the window procedure.
//!
//! The callback is a stateless C function, so its inputs (enabled flag, target
//! window, message id) live in statics. The hook is installed on the thread
//! that runs the message loop, and the callback fires on that same thread.

#[cfg(windows)]
mod imp {
    use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, Ordering};

    use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::Input::KeyboardAndMouse::{VK_VOLUME_DOWN, VK_VOLUME_UP};
    use windows::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, PostMessageW, SetWindowsHookExW, UnhookWindowsHookEx, HC_ACTION, HHOOK,
        KBDLLHOOKSTRUCT, WH_KEYBOARD_LL, WM_KEYDOWN, WM_SYSKEYDOWN,
    };

    static ENABLED: AtomicBool = AtomicBool::new(true);
    static TARGET_HWND: AtomicIsize = AtomicIsize::new(0);
    static STEP_MESSAGE: AtomicU32 = AtomicU32::new(0);

    /// `wparam` value posted for a brightness-up step.
    pub const DIR_UP: usize = 1;
    /// `wparam` value posted for a brightness-down step.
    pub const DIR_DOWN: usize = 0;

    /// An installed keyboard hook. Dropping it removes the hook.
    pub struct Hook {
        handle: HHOOK,
    }

    impl Hook {
        /// Install the hook. On each volume key-down while enabled, the hook
        /// posts `message` to `hwnd` with `wparam` set to [`DIR_UP`]/[`DIR_DOWN`].
        pub fn install(enabled: bool, hwnd: HWND, message: u32) -> Result<Hook, String> {
            ENABLED.store(enabled, Ordering::SeqCst);
            TARGET_HWND.store(hwnd.0 as isize, Ordering::SeqCst);
            STEP_MESSAGE.store(message, Ordering::SeqCst);
            unsafe {
                let hmod = GetModuleHandleW(None).map_err(|e| e.to_string())?;
                let hinst: HINSTANCE = hmod.into();
                let handle = SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), hinst, 0)
                    .map_err(|e| e.to_string())?;
                Ok(Hook { handle })
            }
        }
    }

    impl Drop for Hook {
        fn drop(&mut self) {
            unsafe {
                let _ = UnhookWindowsHookEx(self.handle);
            }
        }
    }

    /// Enable or disable interception at runtime (from the tray menu).
    pub fn set_enabled(enabled: bool) {
        ENABLED.store(enabled, Ordering::SeqCst);
    }

    /// Whether interception is currently enabled.
    pub fn is_enabled() -> bool {
        ENABLED.load(Ordering::SeqCst)
    }

    unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code == HC_ACTION as i32 {
            let kb = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
            let is_up = kb.vkCode == VK_VOLUME_UP.0 as u32;
            let is_down = kb.vkCode == VK_VOLUME_DOWN.0 as u32;
            if (is_up || is_down) && ENABLED.load(Ordering::SeqCst) {
                let message = wparam.0 as u32;
                if message == WM_KEYDOWN || message == WM_SYSKEYDOWN {
                    let hwnd = HWND(TARGET_HWND.load(Ordering::SeqCst) as *mut core::ffi::c_void);
                    let step_msg = STEP_MESSAGE.load(Ordering::SeqCst);
                    let dir = if is_up { DIR_UP } else { DIR_DOWN };
                    // Post (don't send) so the slow WMI work runs on the message
                    // loop, not inside this time-limited hook callback.
                    let _ = PostMessageW(hwnd, step_msg, WPARAM(dir), LPARAM(0));
                }
                // Swallow key-down and key-up for the volume keys so the OS
                // never sees them.
                return LRESULT(1);
            }
        }
        CallNextHookEx(None, code, wparam, lparam)
    }
}

#[cfg(not(windows))]
mod imp {
    pub const DIR_UP: usize = 1;
    pub const DIR_DOWN: usize = 0;

    /// Stub hook for non-Windows builds.
    pub struct Hook;

    impl Hook {
        pub fn install(_enabled: bool, _hwnd: (), _message: u32) -> Result<Hook, String> {
            Err("keyboard hook is only supported on Windows".into())
        }
    }

    pub fn set_enabled(_enabled: bool) {}

    pub fn is_enabled() -> bool {
        false
    }
}

pub use imp::{is_enabled, set_enabled, Hook, DIR_UP};
