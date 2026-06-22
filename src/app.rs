//! Windows tray application: first-run prompt, tray icon and menu, keyboard
//! hook, and the message loop that ties them together.

use std::cell::RefCell;
use std::mem::size_of;

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DispatchMessageW,
    GetCursorPos, GetMessageW, LoadIconW, MessageBoxW, PostQuitMessage, RegisterClassW,
    SetForegroundWindow, TrackPopupMenu, TranslateMessage, HICON, IDI_APPLICATION, IDYES,
    MB_ICONERROR, MB_ICONQUESTION, MB_ICONWARNING, MB_OK, MB_YESNO, MESSAGEBOX_STYLE, MF_CHECKED,
    MF_SEPARATOR, MF_STRING, MF_UNCHECKED, MSG, TPM_RETURNCMD, TPM_RIGHTBUTTON, WINDOW_EX_STYLE,
    WINDOW_STYLE, WM_APP, WM_CONTEXTMENU, WM_LBUTTONUP, WM_RBUTTONUP, WNDCLASSW,
};

use crate::brightness::Brightness;
use crate::config::Config;
use crate::hook;

const TRAY_ID: u32 = 1;
const WM_TRAY: u32 = WM_APP + 1;

const ID_ENABLED: u32 = 1;
const ID_AUTOSTART: u32 = 2;
const ID_EXIT: u32 = 3;

/// State the window procedure needs. Single-threaded, so a thread-local holds it.
struct AppState {
    config: Config,
    nid: NOTIFYICONDATAW,
}

thread_local! {
    static STATE: RefCell<Option<AppState>> = const { RefCell::new(None) };
}

/// Run the application. Returns when the user selects Exit.
pub fn run() {
    unsafe {
        // COM is needed for the WMI brightness backend.
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);

        let mut config = Config::load();

        // First-run autostart prompt.
        if !config.first_run_complete {
            let yes = ask_autostart();
            if let Err(e) = crate::autostart::set(yes) {
                warn(&format!("Could not change the autostart setting: {e}"));
            }
            config.first_run_complete = true;
            let _ = config.save();
        }

        // The registry is the source of truth for autostart; reconcile in case
        // the user added or removed the entry outside the app.
        config.autostart = crate::autostart::is_enabled();

        // Connect to brightness; if it fails the tray still works so the user
        // can disable interception or exit.
        let hook_result = match Brightness::connect() {
            Ok(b) => hook::Hook::install(config.intercept_enabled, move |delta| {
                let _ = b.step(delta);
            }),
            Err(e) => {
                warn(&format!(
                    "Brightness control is unavailable on this system: {e}\n\nThe volume keys will not be remapped."
                ));
                hook::Hook::install(config.intercept_enabled, |_| {})
            }
        };
        // Keep the hook alive for the whole message loop; dropping it unhooks.
        let _hook = match hook_result {
            Ok(h) => h,
            Err(e) => {
                error(&format!("Failed to install the keyboard hook: {e}"));
                CoUninitialize();
                return;
            }
        };

        let hinstance: HINSTANCE = match GetModuleHandleW(None) {
            Ok(h) => h.into(),
            Err(e) => {
                error(&format!("Could not get module handle: {e}"));
                CoUninitialize();
                return;
            }
        };

        let class_name = w!("brightvolHiddenWindow");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: hinstance,
            lpszClassName: class_name,
            ..Default::default()
        };
        RegisterClassW(&wc);

        let hwnd = match CreateWindowExW(
            WINDOW_EX_STYLE(0),
            class_name,
            w!("brightvol"),
            WINDOW_STYLE(0),
            0,
            0,
            0,
            0,
            None,
            None,
            hinstance,
            None,
        ) {
            Ok(h) => h,
            Err(e) => {
                error(&format!("Could not create the message window: {e}"));
                CoUninitialize();
                return;
            }
        };

        // Add the tray icon.
        let hicon: HICON = LoadIconW(None, IDI_APPLICATION).unwrap_or_default();
        let mut nid = NOTIFYICONDATAW {
            cbSize: size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: hwnd,
            uID: TRAY_ID,
            uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
            uCallbackMessage: WM_TRAY,
            hIcon: hicon,
            ..Default::default()
        };
        set_tip(&mut nid, "brightvol: volume keys control brightness");
        let _ = Shell_NotifyIconW(NIM_ADD, &nid);

        STATE.with(|s| *s.borrow_mut() = Some(AppState { config, nid }));

        // Message loop.
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        // _hook drops here, removing the keyboard hook.
        CoUninitialize();
    }
}

/// Ask, on first run, whether to start on login.
unsafe fn ask_autostart() -> bool {
    let result = MessageBoxW(
        None,
        w!("Start brightvol automatically when you sign in?"),
        w!("brightvol"),
        MB_YESNO | MB_ICONQUESTION,
    );
    result == IDYES
}

unsafe fn warn(message: &str) {
    message_box(message, MB_ICONWARNING);
}

unsafe fn error(message: &str) {
    message_box(message, MB_ICONERROR);
}

unsafe fn message_box(message: &str, icon: MESSAGEBOX_STYLE) {
    let wide: Vec<u16> = message.encode_utf16().chain(std::iter::once(0)).collect();
    MessageBoxW(None, PCWSTR(wide.as_ptr()), w!("brightvol"), MB_OK | icon);
}

/// Copy a tooltip string into the fixed-size `szTip` buffer.
fn set_tip(nid: &mut NOTIFYICONDATAW, text: &str) {
    let wide: Vec<u16> = text.encode_utf16().collect();
    let n = wide.len().min(nid.szTip.len() - 1);
    nid.szTip[..n].copy_from_slice(&wide[..n]);
    nid.szTip[n] = 0;
}

extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        if msg == WM_TRAY {
            let event = (lparam.0 as u32) & 0xFFFF;
            if event == WM_RBUTTONUP || event == WM_LBUTTONUP || event == WM_CONTEXTMENU {
                show_menu(hwnd);
            }
            return LRESULT(0);
        }
        DefWindowProcW(hwnd, msg, wparam, lparam)
    }
}

/// Build and show the tray context menu, then act on the selection.
unsafe fn show_menu(hwnd: HWND) {
    let enabled = hook::is_enabled();
    let autostart_on = STATE.with(|s| {
        s.borrow()
            .as_ref()
            .map(|st| st.config.autostart)
            .unwrap_or(false)
    });

    let menu = match CreatePopupMenu() {
        Ok(m) => m,
        Err(_) => return,
    };
    let enabled_flag = if enabled { MF_CHECKED } else { MF_UNCHECKED };
    let autostart_flag = if autostart_on {
        MF_CHECKED
    } else {
        MF_UNCHECKED
    };
    let _ = AppendMenuW(
        menu,
        MF_STRING | enabled_flag,
        ID_ENABLED as usize,
        w!("Enabled"),
    );
    let _ = AppendMenuW(
        menu,
        MF_STRING | autostart_flag,
        ID_AUTOSTART as usize,
        w!("Start on login"),
    );
    let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
    let _ = AppendMenuW(menu, MF_STRING, ID_EXIT as usize, w!("Exit"));

    let mut pt = POINT::default();
    let _ = GetCursorPos(&mut pt);
    // Required so the menu dismisses correctly when focus is lost.
    let _ = SetForegroundWindow(hwnd);
    let selected = TrackPopupMenu(
        menu,
        TPM_RIGHTBUTTON | TPM_RETURNCMD,
        pt.x,
        pt.y,
        0,
        hwnd,
        None,
    );
    let _ = DestroyMenu(menu);

    handle_command(selected.0 as u32);
}

/// Apply a menu selection.
unsafe fn handle_command(id: u32) {
    match id {
        ID_ENABLED => {
            let new = !hook::is_enabled();
            hook::set_enabled(new);
            STATE.with(|s| {
                if let Some(st) = s.borrow_mut().as_mut() {
                    st.config.intercept_enabled = new;
                    let _ = st.config.save();
                }
            });
        }
        ID_AUTOSTART => {
            let new = !STATE.with(|s| {
                s.borrow()
                    .as_ref()
                    .map(|st| st.config.autostart)
                    .unwrap_or(false)
            });
            match crate::autostart::set(new) {
                Ok(()) => STATE.with(|s| {
                    if let Some(st) = s.borrow_mut().as_mut() {
                        st.config.autostart = new;
                        let _ = st.config.save();
                    }
                }),
                Err(e) => warn(&format!("Could not change the autostart setting: {e}")),
            }
        }
        ID_EXIT => {
            STATE.with(|s| {
                if let Some(st) = s.borrow().as_ref() {
                    let _ = Shell_NotifyIconW(NIM_DELETE, &st.nid);
                }
            });
            PostQuitMessage(0);
        }
        _ => {}
    }
}
