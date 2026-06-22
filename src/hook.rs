//! Global low-level keyboard hook that intercepts the volume media keys.
//!
//! While interception is enabled, `VolumeUp`/`VolumeDown` key presses are
//! swallowed (the OS never changes the volume) and trigger a brightness step
//! through a caller-supplied action instead.
//!
//! A `WH_KEYBOARD_LL` hook callback is a stateless C function, so the enabled
//! flag lives in a static and the action lives in a thread-local. The hook is
//! installed on the thread that runs the message loop (the main thread), and
//! the callback fires on that same thread, so the thread-local is always set.

/// Step size in percentage points per key press.
pub use crate::brightness::STEP;

#[cfg(windows)]
mod imp {
    use std::cell::RefCell;
    use std::sync::atomic::{AtomicBool, Ordering};

    use windows::Win32::Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::Input::KeyboardAndMouse::{VK_VOLUME_DOWN, VK_VOLUME_UP};
    use windows::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, SetWindowsHookExW, UnhookWindowsHookEx, HC_ACTION, HHOOK, KBDLLHOOKSTRUCT,
        WH_KEYBOARD_LL, WM_KEYDOWN, WM_SYSKEYDOWN,
    };

    static ENABLED: AtomicBool = AtomicBool::new(true);

    thread_local! {
        static ACTION: RefCell<Option<Box<dyn Fn(i8)>>> = const { RefCell::new(None) };
    }

    /// An installed keyboard hook. Dropping it removes the hook.
    pub struct Hook {
        handle: HHOOK,
    }

    impl Hook {
        /// Install the hook. `action` is invoked with `+STEP`/`-STEP` on each
        /// volume key-down while interception is enabled.
        pub fn install<F>(enabled: bool, action: F) -> Result<Hook, String>
        where
            F: Fn(i8) + 'static,
        {
            ENABLED.store(enabled, Ordering::SeqCst);
            ACTION.with(|a| *a.borrow_mut() = Some(Box::new(action)));
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
            ACTION.with(|a| *a.borrow_mut() = None);
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
                    let delta = if is_up { super::STEP } else { -super::STEP };
                    ACTION.with(|a| {
                        if let Some(f) = a.borrow().as_ref() {
                            f(delta);
                        }
                    });
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
    /// Stub hook for non-Windows builds.
    pub struct Hook;

    impl Hook {
        pub fn install<F>(_enabled: bool, _action: F) -> Result<Hook, String>
        where
            F: Fn(i8) + 'static,
        {
            Err("keyboard hook is only supported on Windows".into())
        }
    }

    pub fn set_enabled(_enabled: bool) {}

    pub fn is_enabled() -> bool {
        false
    }
}

pub use imp::{is_enabled, set_enabled, Hook};
