#![allow(non_snake_case)]

#[cfg(target_os = "windows")]
use super::*;

#[cfg(all(target_os = "windows", test))]
fn foreground_process_name_lower() -> Option<String> {
    unsafe { process_name_lower_for_hwnd(GetForegroundWindow()) }
}

#[cfg(all(target_os = "windows", test))]
fn foreground_is_current_process() -> bool {
    let Some(foreground) = foreground_process_name_lower() else {
        return false;
    };
    current_process_name_lower().as_deref() == Some(foreground.as_str())
}

#[cfg(target_os = "windows")]
unsafe fn process_name_lower_for_hwnd(hwnd: Hwnd) -> Option<String> {
    if hwnd.is_null() {
        return None;
    }
    let mut process_id = 0u32;
    if GetWindowThreadProcessId(hwnd, &mut process_id as *mut u32) == 0 || process_id == 0 {
        return None;
    }
    let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process_id);
    if process.is_null() {
        return None;
    }
    let mut buffer = [0u16; 1024];
    let mut size = buffer.len() as u32;
    let ok = QueryFullProcessImageNameW(process, 0, buffer.as_mut_ptr(), &mut size) != 0;
    CloseHandle(process);
    if !ok || size == 0 {
        return None;
    }
    let path = String::from_utf16_lossy(&buffer[..size as usize]);
    std::path::Path::new(&path)
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.to_ascii_lowercase())
}
#[cfg(target_os = "windows")]
pub(super) fn foreground_app_candidate() -> Option<TextExpanderAppCandidate> {
    unsafe { app_candidate_for_hwnd(GetForegroundWindow()) }
}

#[cfg(target_os = "windows")]
unsafe fn app_candidate_for_hwnd(hwnd: Hwnd) -> Option<TextExpanderAppCandidate> {
    let exe = process_name_lower_for_hwnd(hwnd)?;
    let title = window_title(hwnd).unwrap_or_default();
    Some(TextExpanderAppCandidate { exe, title })
}

#[cfg(any(target_os = "windows", test))]
fn windows_picker_process_is_system(exe: &str) -> bool {
    matches!(
        exe.trim().to_ascii_lowercase().as_str(),
        "applicationframehost.exe"
            | "systemsettings.exe"
            | "textinputhost.exe"
            | "shellexperiencehost.exe"
            | "startmenuexperiencehost.exe"
            | "searchhost.exe"
            | "searchapp.exe"
            | "lockapp.exe"
            | "runtimebroker.exe"
            | "securityhealthsystray.exe"
            | "widgets.exe"
            | "widgetservice.exe"
            | "taskhostw.exe"
            | "dllhost.exe"
            | "ctfmon.exe"
    )
}

#[cfg(target_os = "windows")]
unsafe fn window_is_user_facing(hwnd: Hwnd) -> bool {
    if hwnd.is_null() || IsWindowVisible(hwnd) == 0 {
        return false;
    }
    let mut cloaked = 0u32;
    if DwmGetWindowAttribute(
        hwnd,
        DWMWA_CLOAKED,
        &mut cloaked as *mut u32 as *mut core::ffi::c_void,
        std::mem::size_of::<u32>() as u32,
    ) == 0
        && cloaked != 0
    {
        return false;
    }
    let extended_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
    if extended_style & WS_EX_TOOLWINDOW != 0 && extended_style & WS_EX_APPWINDOW == 0 {
        return false;
    }
    if !GetWindow(hwnd, GW_OWNER).is_null() && extended_style & WS_EX_APPWINDOW == 0 {
        return false;
    }
    true
}

#[cfg(target_os = "windows")]
unsafe fn uwp_child_candidate(hwnd: Hwnd, parent_title: &str) -> Option<TextExpanderAppCandidate> {
    struct Search<'a> {
        parent_title: &'a str,
        candidate: Option<TextExpanderAppCandidate>,
    }

    unsafe extern "system" fn enum_child(child: Hwnd, lparam: isize) -> i32 {
        let search = &mut *(lparam as *mut Search<'_>);
        let Some(exe) = process_name_lower_for_hwnd(child) else {
            return 1;
        };
        if windows_picker_process_is_system(&exe) {
            return 1;
        }
        search.candidate = Some(TextExpanderAppCandidate {
            exe,
            title: search.parent_title.to_owned(),
        });
        0
    }

    let mut search = Search {
        parent_title,
        candidate: None,
    };
    EnumChildWindows(
        hwnd,
        Some(enum_child),
        &mut search as *mut Search<'_> as isize,
    );
    search.candidate
}

#[cfg(target_os = "windows")]
unsafe fn picker_candidate_for_hwnd(hwnd: Hwnd) -> Option<TextExpanderAppCandidate> {
    if !window_is_user_facing(hwnd) {
        return None;
    }
    let candidate = app_candidate_for_hwnd(hwnd)?;
    if candidate.title.is_empty() {
        return None;
    }
    if candidate.exe == "applicationframehost.exe" {
        return uwp_child_candidate(hwnd, &candidate.title);
    }
    (!windows_picker_process_is_system(&candidate.exe)).then_some(candidate)
}

#[cfg(target_os = "windows")]
unsafe fn window_title(hwnd: Hwnd) -> Option<String> {
    let len = GetWindowTextLengthW(hwnd);
    if len <= 0 {
        return None;
    }
    let mut buffer = vec![0u16; len as usize + 1];
    let copied = GetWindowTextW(hwnd, buffer.as_mut_ptr(), buffer.len() as i32);
    if copied <= 0 {
        return None;
    }
    Some(
        String::from_utf16_lossy(&buffer[..copied as usize])
            .trim()
            .to_owned(),
    )
}

#[cfg(target_os = "windows")]
pub(super) fn platform_open_window_candidates() -> Vec<TextExpanderAppCandidate> {
    unsafe extern "system" fn enum_proc(hwnd: Hwnd, lparam: isize) -> i32 {
        let apps = &mut *(lparam as *mut Vec<TextExpanderAppCandidate>);
        let Some(candidate) = picker_candidate_for_hwnd(hwnd) else {
            return 1;
        };
        if current_process_name_lower().as_deref() == Some(candidate.exe.as_str()) {
            return 1;
        }
        if !apps.iter().any(|app| app.exe == candidate.exe) {
            apps.push(candidate);
        }
        1
    }

    let mut apps: Vec<TextExpanderAppCandidate> = Vec::new();
    unsafe {
        EnumWindows(Some(enum_proc), &mut apps as *mut _ as isize);
    }
    apps.sort_by(|a, b| a.exe.cmp(&b.exe));
    apps
}

#[cfg(all(target_os = "windows", test))]
fn remember_current_foreground_app() {
    if let Some(candidate) = foreground_app_candidate() {
        remember_foreground_app(candidate);
    }
}

#[cfg(all(target_os = "windows", test))]
pub(super) fn foreground_app_blacklisted(app_blacklist: &[String]) -> bool {
    if app_blacklist.is_empty() {
        return false;
    }
    foreground_process_name_lower()
        .map(|name| {
            let name_stem = name.strip_suffix(".exe").unwrap_or(&name);
            app_blacklist.iter().any(|blocked| {
                let blocked_stem = blocked.strip_suffix(".exe").unwrap_or(blocked);
                name == *blocked || name_stem == blocked_stem
            })
        })
        .unwrap_or(false)
}

#[cfg(all(target_os = "windows", test))]
pub(super) fn start() {
    use std::sync::Once;
    static START: Once = Once::new();
    START.call_once(|| {
        std::thread::spawn(|| unsafe {
            run_windows_keyboard_hook_loop();
        });
    });
}

#[cfg(all(target_os = "windows", test))]
fn modifier_down(vk: i32) -> bool {
    unsafe { GetAsyncKeyState(vk) & 0x8000u16 as i16 != 0 }
}

#[cfg(all(target_os = "windows", test))]
unsafe fn run_windows_keyboard_hook_loop() {
    let module = GetModuleHandleW(std::ptr::null());
    let hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), module, 0);
    if hook.is_null() {
        log::warn!("Smart Input: failed to install keyboard hook");
        return;
    }

    remember_current_foreground_app();
    let foreground_hook = SetWinEventHook(
        EVENT_SYSTEM_FOREGROUND,
        EVENT_SYSTEM_FOREGROUND,
        std::ptr::null_mut(),
        Some(foreground_event_proc),
        0,
        0,
        WINEVENT_OUTOFCONTEXT,
    );
    if foreground_hook.is_null() {
        log::warn!("Smart Input: failed to install foreground app tracker");
    }

    let mut msg = Msg::default();
    while GetMessageW(&mut msg as *mut Msg, std::ptr::null_mut(), 0, 0) > 0 {
        TranslateMessage(&msg as *const Msg);
        DispatchMessageW(&msg as *const Msg);
    }

    if !foreground_hook.is_null() {
        UnhookWinEvent(foreground_hook);
    }
    UnhookWindowsHookEx(hook);
}

#[cfg(all(target_os = "windows", test))]
unsafe extern "system" fn foreground_event_proc(
    _hook: Hwineventhook,
    event: u32,
    hwnd: Hwnd,
    _id_object: i32,
    _id_child: i32,
    _event_thread: u32,
    _event_time: u32,
) {
    if event == EVENT_SYSTEM_FOREGROUND {
        if let Some(candidate) = app_candidate_for_hwnd(hwnd) {
            remember_foreground_app(candidate);
        }
    }
}

#[cfg(all(target_os = "windows", test))]
unsafe extern "system" fn keyboard_proc(n_code: i32, w_param: usize, l_param: isize) -> isize {
    if n_code == HC_ACTION {
        let info = &*(l_param as *const Kbdllhookstruct);
        let is_key_down = w_param == WM_KEYDOWN || w_param == WM_SYSKEYDOWN;
        let injected = info.flags & LLKHF_INJECTED != 0;
        if !injected {
            if is_key_down {
                remember_current_foreground_app();
            }
            if foreground_is_current_process() {
                return CallNextHookEx(std::ptr::null_mut(), n_code, w_param, l_param);
            }
            if is_key_down && text_expander_enabled() {
                if text_expander_suppressed_for_context() {
                    if let Ok(mut engine) = text_expander_engine().lock() {
                        engine.reset();
                    }
                } else if info.vkCode == VK_BACK as u32 {
                    if let Ok(mut engine) = text_expander_engine().lock() {
                        engine.backspace();
                    }
                } else if should_reset_text_expander_for_vk(info.vkCode) {
                    if let Ok(mut engine) = text_expander_engine().lock() {
                        engine.reset();
                    }
                } else if let Some(ch) = text_expander_char_for_key(info) {
                    let expansion = text_expander_engine()
                        .lock()
                        .ok()
                        .and_then(|mut engine| engine.push_char(ch));
                    if let Some(expansion) = expansion {
                        schedule_text_expansion(expansion);
                    }
                }
            }
        }
    }
    CallNextHookEx(std::ptr::null_mut(), n_code, w_param, l_param)
}

#[cfg(all(target_os = "windows", test))]
unsafe fn text_expander_char_for_key(info: &Kbdllhookstruct) -> Option<char> {
    if modifier_down(VK_CONTROL)
        || modifier_down(VK_MENU)
        || modifier_down(VK_LWIN)
        || modifier_down(VK_RWIN)
    {
        return None;
    }

    let mut keyboard_state = [0u8; 256];
    if GetKeyboardState(keyboard_state.as_mut_ptr()) == 0 {
        return None;
    }
    set_keyboard_state_down(&mut keyboard_state, VK_SHIFT, modifier_down(VK_SHIFT));
    set_keyboard_state_down(&mut keyboard_state, VK_CONTROL, false);
    set_keyboard_state_down(&mut keyboard_state, VK_MENU, false);
    set_keyboard_state_down(&mut keyboard_state, VK_LWIN, false);
    set_keyboard_state_down(&mut keyboard_state, VK_RWIN, false);
    if GetKeyState(VK_CAPITAL) & 1 != 0 {
        keyboard_state[VK_CAPITAL as usize] |= 1;
    }
    if (info.vkCode as usize) < keyboard_state.len() {
        keyboard_state[info.vkCode as usize] = 0x80;
    }

    let mut buffer = [0u16; 8];
    let len = ToUnicodeEx(
        info.vkCode,
        info.scanCode,
        keyboard_state.as_ptr(),
        buffer.as_mut_ptr(),
        buffer.len() as i32,
        0,
        foreground_keyboard_layout(),
    );
    if len <= 0 {
        return None;
    }

    char::decode_utf16(buffer[..len as usize].iter().copied())
        .next()
        .and_then(Result::ok)
        .filter(|ch| !ch.is_control())
}

#[cfg(all(target_os = "windows", test))]
unsafe fn foreground_keyboard_layout() -> *mut core::ffi::c_void {
    let hwnd = GetForegroundWindow();
    if hwnd.is_null() {
        return GetKeyboardLayout(0);
    }
    let thread_id = GetWindowThreadProcessId(hwnd, std::ptr::null_mut());
    if thread_id == 0 {
        return GetKeyboardLayout(0);
    }
    GetKeyboardLayout(thread_id)
}

#[cfg(all(target_os = "windows", test))]
fn set_keyboard_state_down(keyboard_state: &mut [u8; 256], vk: i32, down: bool) {
    if vk >= 0 && (vk as usize) < keyboard_state.len() {
        if down {
            keyboard_state[vk as usize] |= 0x80;
        } else {
            keyboard_state[vk as usize] &= !0x80;
        }
    }
}

#[cfg(all(target_os = "windows", test))]
fn should_reset_text_expander_for_vk(vk: u32) -> bool {
    matches!(
        vk as i32,
        VK_RETURN | VK_TAB | VK_ESCAPE | VK_LEFT | VK_UP | VK_RIGHT | VK_DOWN
    )
}

#[cfg(all(target_os = "windows", test))]
fn schedule_text_expansion(expansion: crate::text_expander::TextExpansionMatch) {
    std::thread::spawn(move || unsafe {
        std::thread::sleep(std::time::Duration::from_millis(8));
        send_text_expansion(&expansion);
    });
}

#[cfg(all(target_os = "windows", test))]
unsafe fn send_text_expansion(expansion: &crate::text_expander::TextExpansionMatch) {
    for _ in 0..expansion.typed_trigger_chars {
        send_vk_tap(VK_BACK as u16);
    }

    let pasted = should_use_clipboard_paste(&expansion.replacement)
        && paste_text_with_clipboard_restore(&expansion.replacement);
    if !pasted {
        send_unicode_text(&expansion.replacement);
    }

    for _ in 0..expansion.cursor_back_chars {
        send_vk_tap(VK_LEFT as u16);
    }
}

#[cfg(all(target_os = "windows", test))]
fn should_use_clipboard_paste(text: &str) -> bool {
    text.chars().count() > 64 || text.contains(['\n', '\r', '\t'])
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
enum WindowsTextUnit {
    Character(char),
    LineBreak,
}

#[cfg(test)]
fn windows_text_units(text: &str) -> impl Iterator<Item = WindowsTextUnit> + '_ {
    let mut chars = text.chars().peekable();
    std::iter::from_fn(move || {
        let ch = chars.next()?;
        Some(match ch {
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                WindowsTextUnit::LineBreak
            }
            '\n' => WindowsTextUnit::LineBreak,
            _ => WindowsTextUnit::Character(ch),
        })
    })
}

#[cfg(test)]
fn windows_clipboard_text(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    for unit in windows_text_units(text) {
        match unit {
            WindowsTextUnit::Character(ch) => output.push(ch),
            WindowsTextUnit::LineBreak => output.push_str("\r\n"),
        }
    }
    output
}

#[cfg(all(target_os = "windows", test))]
unsafe fn paste_text_with_clipboard_restore(text: &str) -> bool {
    let _guard = match CLIPBOARD_PASTE_LOCK.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    let Some(previous_text) = read_clipboard_text() else {
        return false;
    };
    if !set_clipboard_text(text) {
        return false;
    }
    std::thread::sleep(std::time::Duration::from_millis(12));
    let pasted = send_ctrl_v();
    std::thread::sleep(std::time::Duration::from_millis(80));
    if !set_clipboard_text(&previous_text) {
        log::warn!("Smart Input: failed to restore clipboard text after paste");
    }
    pasted
}

#[cfg(all(target_os = "windows", test))]
unsafe fn read_clipboard_text() -> Option<String> {
    if OpenClipboard(std::ptr::null_mut()) == 0 {
        return None;
    }
    let handle = GetClipboardData(CF_UNICODETEXT);
    if handle.is_null() {
        CloseClipboard();
        return None;
    }
    let locked = GlobalLock(handle) as *const u16;
    if locked.is_null() {
        CloseClipboard();
        return None;
    }
    let mut len = 0usize;
    while *locked.add(len) != 0 {
        len += 1;
    }
    let text = String::from_utf16_lossy(std::slice::from_raw_parts(locked, len));
    GlobalUnlock(handle);
    CloseClipboard();
    Some(text)
}

#[cfg(all(target_os = "windows", test))]
unsafe fn set_clipboard_text(text: &str) -> bool {
    if OpenClipboard(std::ptr::null_mut()) == 0 {
        return false;
    }
    if EmptyClipboard() == 0 {
        CloseClipboard();
        return false;
    }
    let clipboard_text = windows_clipboard_text(text);
    let utf16 = clipboard_text
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let bytes = utf16.len() * std::mem::size_of::<u16>();
    let handle = GlobalAlloc(GMEM_MOVEABLE, bytes);
    if handle.is_null() {
        CloseClipboard();
        return false;
    }
    let locked = GlobalLock(handle) as *mut u16;
    if locked.is_null() {
        GlobalFree(handle);
        CloseClipboard();
        return false;
    }
    std::ptr::copy_nonoverlapping(utf16.as_ptr(), locked, utf16.len());
    GlobalUnlock(handle);
    if SetClipboardData(CF_UNICODETEXT, handle).is_null() {
        GlobalFree(handle);
        CloseClipboard();
        return false;
    }
    CloseClipboard();
    true
}

#[cfg(all(target_os = "windows", test))]
unsafe fn send_ctrl_v() -> bool {
    let inputs = [
        Input::keyboard_vk(VK_CONTROL as u16, false),
        Input::keyboard_vk(VK_V as u16, false),
        Input::keyboard_vk(VK_V as u16, true),
        Input::keyboard_vk(VK_CONTROL as u16, true),
    ];
    let sent = SendInput(
        inputs.len() as u32,
        inputs.as_ptr(),
        std::mem::size_of::<Input>() as i32,
    );
    if sent != inputs.len() as u32 {
        log::warn!("Smart Input: SendInput failed for Ctrl+V");
        send_vk_keyup(VK_V as u16);
        send_vk_keyup(VK_CONTROL as u16);
    }
    sent >= 2
}

#[cfg(all(target_os = "windows", test))]
unsafe fn send_unicode_text(text: &str) {
    for text_unit in windows_text_units(text) {
        match text_unit {
            WindowsTextUnit::Character(ch) => {
                for unit in ch.encode_utf16(&mut [0; 2]) {
                    let down = Input::keyboard_unicode(*unit, false);
                    let up = Input::keyboard_unicode(*unit, true);
                    let inputs = [down, up];
                    let sent = SendInput(
                        inputs.len() as u32,
                        inputs.as_ptr(),
                        std::mem::size_of::<Input>() as i32,
                    );
                    if sent != inputs.len() as u32 {
                        log::warn!(
                            "Smart Input: SendInput failed for text expansion unit U+{:04X}",
                            *unit as u32
                        );
                    }
                }
            }
            WindowsTextUnit::LineBreak => send_text_expansion_line_break(),
        }
    }
}

#[cfg(all(target_os = "windows", test))]
unsafe fn send_text_expansion_line_break() {
    if modifier_down(VK_SHIFT) {
        send_vk_tap(VK_RETURN as u16);
        return;
    }

    // Shift+Enter inserts a line break without submitting chat and browser forms.
    let inputs = [
        Input::keyboard_vk(VK_SHIFT as u16, false),
        Input::keyboard_vk(VK_RETURN as u16, false),
        Input::keyboard_vk(VK_RETURN as u16, true),
        Input::keyboard_vk(VK_SHIFT as u16, true),
    ];
    let sent = SendInput(
        inputs.len() as u32,
        inputs.as_ptr(),
        std::mem::size_of::<Input>() as i32,
    );
    if sent != inputs.len() as u32 {
        log::warn!("Smart Input: SendInput failed for text expansion line break");
        send_vk_keyup(VK_RETURN as u16);
        send_vk_keyup(VK_SHIFT as u16);
    }
}

#[cfg(all(target_os = "windows", test))]
unsafe fn send_vk_tap(vk: u16) {
    let down = Input::keyboard_vk(vk, false);
    let up = Input::keyboard_vk(vk, true);
    let inputs = [down, up];
    let sent = SendInput(
        inputs.len() as u32,
        inputs.as_ptr(),
        std::mem::size_of::<Input>() as i32,
    );
    if sent != inputs.len() as u32 {
        log::warn!("Smart Input: SendInput failed for VK tap 0x{vk:02X}");
    }
}

#[cfg(all(target_os = "windows", test))]
unsafe fn send_vk_keyup(vk: u16) {
    let input = Input::keyboard_vk(vk, true);
    let sent = SendInput(
        1,
        &input as *const Input,
        std::mem::size_of::<Input>() as i32,
    );
    if sent != 1 {
        log::warn!("Smart Input: SendInput failed for VK keyup 0x{:02X}", vk);
    }
}

#[cfg(all(target_os = "windows", test))]
const WH_KEYBOARD_LL: i32 = 13;
#[cfg(all(target_os = "windows", test))]
const HC_ACTION: i32 = 0;
#[cfg(all(target_os = "windows", test))]
const WM_KEYDOWN: usize = 0x0100;
#[cfg(all(target_os = "windows", test))]
const WM_SYSKEYDOWN: usize = 0x0104;
#[cfg(all(target_os = "windows", test))]
const VK_SHIFT: i32 = 0x10;
#[cfg(all(target_os = "windows", test))]
const VK_CONTROL: i32 = 0x11;
#[cfg(all(target_os = "windows", test))]
const VK_MENU: i32 = 0x12;
#[cfg(all(target_os = "windows", test))]
const VK_CAPITAL: i32 = 0x14;
#[cfg(all(target_os = "windows", test))]
const VK_BACK: i32 = 0x08;
#[cfg(all(target_os = "windows", test))]
const VK_TAB: i32 = 0x09;
#[cfg(all(target_os = "windows", test))]
const VK_RETURN: i32 = 0x0D;
#[cfg(all(target_os = "windows", test))]
const VK_ESCAPE: i32 = 0x1B;
#[cfg(all(target_os = "windows", test))]
const VK_LEFT: i32 = 0x25;
#[cfg(all(target_os = "windows", test))]
const VK_UP: i32 = 0x26;
#[cfg(all(target_os = "windows", test))]
const VK_RIGHT: i32 = 0x27;
#[cfg(all(target_os = "windows", test))]
const VK_DOWN: i32 = 0x28;
#[cfg(all(target_os = "windows", test))]
const VK_LWIN: i32 = 0x5B;
#[cfg(all(target_os = "windows", test))]
const VK_RWIN: i32 = 0x5C;
#[cfg(all(target_os = "windows", test))]
const VK_V: i32 = 0x56;
#[cfg(all(target_os = "windows", test))]
const LLKHF_INJECTED: u32 = 0x10;
#[cfg(all(target_os = "windows", test))]
const INPUT_KEYBOARD: u32 = 1;
#[cfg(all(target_os = "windows", test))]
const KEYEVENTF_KEYUP: u32 = 0x0002;
#[cfg(all(target_os = "windows", test))]
const KEYEVENTF_UNICODE: u32 = 0x0004;
#[cfg(all(target_os = "windows", test))]
const CF_UNICODETEXT: u32 = 13;
#[cfg(all(target_os = "windows", test))]
const GMEM_MOVEABLE: u32 = 0x0002;
#[cfg(target_os = "windows")]
const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
#[cfg(all(target_os = "windows", test))]
const EVENT_SYSTEM_FOREGROUND: u32 = 0x0003;
#[cfg(all(target_os = "windows", test))]
const WINEVENT_OUTOFCONTEXT: u32 = 0x0000;
#[cfg(target_os = "windows")]
const GWL_EXSTYLE: i32 = -20;
#[cfg(target_os = "windows")]
const GW_OWNER: u32 = 4;
#[cfg(target_os = "windows")]
const WS_EX_TOOLWINDOW: u32 = 0x0000_0080;
#[cfg(target_os = "windows")]
const WS_EX_APPWINDOW: u32 = 0x0004_0000;
#[cfg(target_os = "windows")]
const DWMWA_CLOAKED: u32 = 14;

#[cfg(all(target_os = "windows", test))]
static CLIPBOARD_PASTE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(all(target_os = "windows", test))]
type Hhook = *mut core::ffi::c_void;
#[cfg(all(target_os = "windows", test))]
type Hwineventhook = *mut core::ffi::c_void;
#[cfg(all(target_os = "windows", test))]
type Hinstance = *mut core::ffi::c_void;
#[cfg(target_os = "windows")]
type Hwnd = *mut core::ffi::c_void;
#[cfg(target_os = "windows")]
type Handle = *mut core::ffi::c_void;

#[cfg(all(target_os = "windows", test))]
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Kbdllhookstruct {
    vkCode: u32,
    scanCode: u32,
    flags: u32,
    time: u32,
    dwExtraInfo: usize,
}

#[cfg(all(target_os = "windows", test))]
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Msg {
    hwnd: Hwnd,
    message: u32,
    wParam: usize,
    lParam: isize,
    time: u32,
    pt_x: i32,
    pt_y: i32,
}

#[cfg(all(target_os = "windows", test))]
#[repr(C)]
#[derive(Clone, Copy)]
struct Input {
    input_type: u32,
    u: InputUnion,
}

#[cfg(all(target_os = "windows", test))]
impl Input {
    fn keyboard_unicode(unit: u16, key_up: bool) -> Self {
        let flags = KEYEVENTF_UNICODE | if key_up { KEYEVENTF_KEYUP } else { 0 };
        Self {
            input_type: INPUT_KEYBOARD,
            u: InputUnion {
                ki: Keybdinput {
                    wVk: 0,
                    wScan: unit,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }

    fn keyboard_vk(vk: u16, key_up: bool) -> Self {
        let flags = if key_up { KEYEVENTF_KEYUP } else { 0 };
        Self {
            input_type: INPUT_KEYBOARD,
            u: InputUnion {
                ki: Keybdinput {
                    wVk: vk,
                    wScan: 0,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }
}

#[cfg(all(target_os = "windows", test))]
#[repr(C)]
#[derive(Clone, Copy)]
union InputUnion {
    mi: Mouseinput,
    ki: Keybdinput,
}

#[cfg(all(target_os = "windows", test, target_pointer_width = "64"))]
const _: [(); 40] = [(); std::mem::size_of::<Input>()];
#[cfg(all(target_os = "windows", test, target_pointer_width = "32"))]
const _: [(); 28] = [(); std::mem::size_of::<Input>()];

#[cfg(all(target_os = "windows", test))]
#[repr(C)]
#[derive(Clone, Copy)]
struct Mouseinput {
    dx: i32,
    dy: i32,
    mouseData: u32,
    dwFlags: u32,
    time: u32,
    dwExtraInfo: usize,
}

#[cfg(all(target_os = "windows", test))]
#[repr(C)]
#[derive(Clone, Copy)]
struct Keybdinput {
    wVk: u16,
    wScan: u16,
    dwFlags: u32,
    time: u32,
    dwExtraInfo: usize,
}

#[cfg(target_os = "windows")]
#[link(name = "user32")]
extern "system" {
    #[cfg(test)]
    fn SetWindowsHookExW(
        idHook: i32,
        lpfn: Option<unsafe extern "system" fn(i32, usize, isize) -> isize>,
        hmod: Hinstance,
        dwThreadId: u32,
    ) -> Hhook;
    #[cfg(test)]
    fn CallNextHookEx(hhk: Hhook, nCode: i32, wParam: usize, lParam: isize) -> isize;
    #[cfg(test)]
    fn UnhookWindowsHookEx(hhk: Hhook) -> i32;
    #[cfg(test)]
    fn SetWinEventHook(
        eventMin: u32,
        eventMax: u32,
        hmodWinEventProc: Hinstance,
        pfnWinEventProc: Option<
            unsafe extern "system" fn(Hwineventhook, u32, Hwnd, i32, i32, u32, u32),
        >,
        idProcess: u32,
        idThread: u32,
        dwFlags: u32,
    ) -> Hwineventhook;
    #[cfg(test)]
    fn UnhookWinEvent(hWinEventHook: Hwineventhook) -> i32;
    #[cfg(test)]
    fn GetMessageW(lpMsg: *mut Msg, hWnd: Hwnd, wMsgFilterMin: u32, wMsgFilterMax: u32) -> i32;
    #[cfg(test)]
    fn TranslateMessage(lpMsg: *const Msg) -> i32;
    #[cfg(test)]
    fn DispatchMessageW(lpMsg: *const Msg) -> isize;
    #[cfg(test)]
    fn SendInput(cInputs: u32, pInputs: *const Input, cbSize: i32) -> u32;
    #[cfg(test)]
    fn GetAsyncKeyState(vKey: i32) -> i16;
    #[cfg(test)]
    fn GetKeyboardState(lpKeyState: *mut u8) -> i32;
    #[cfg(test)]
    fn GetKeyState(nVirtKey: i32) -> i16;
    #[cfg(test)]
    fn GetKeyboardLayout(idThread: u32) -> *mut core::ffi::c_void;
    #[cfg(test)]
    fn ToUnicodeEx(
        wVirtKey: u32,
        wScanCode: u32,
        lpKeyState: *const u8,
        pwszBuff: *mut u16,
        cchBuff: i32,
        wFlags: u32,
        dwhkl: *mut core::ffi::c_void,
    ) -> i32;
    fn GetForegroundWindow() -> Hwnd;
    fn GetWindowThreadProcessId(hWnd: Hwnd, lpdwProcessId: *mut u32) -> u32;
    fn EnumWindows(
        lpEnumFunc: Option<unsafe extern "system" fn(Hwnd, isize) -> i32>,
        lParam: isize,
    ) -> i32;
    fn EnumChildWindows(
        hWndParent: Hwnd,
        lpEnumFunc: Option<unsafe extern "system" fn(Hwnd, isize) -> i32>,
        lParam: isize,
    ) -> i32;
    fn IsWindowVisible(hWnd: Hwnd) -> i32;
    fn GetWindow(hWnd: Hwnd, uCmd: u32) -> Hwnd;
    fn GetWindowLongPtrW(hWnd: Hwnd, nIndex: i32) -> isize;
    fn GetWindowTextLengthW(hWnd: Hwnd) -> i32;
    fn GetWindowTextW(hWnd: Hwnd, lpString: *mut u16, nMaxCount: i32) -> i32;
    #[cfg(test)]
    fn OpenClipboard(hWndNewOwner: Hwnd) -> i32;
    #[cfg(test)]
    fn CloseClipboard() -> i32;
    #[cfg(test)]
    fn EmptyClipboard() -> i32;
    #[cfg(test)]
    fn GetClipboardData(uFormat: u32) -> Handle;
    #[cfg(test)]
    fn SetClipboardData(uFormat: u32, hMem: Handle) -> Handle;
}

#[cfg(target_os = "windows")]
#[link(name = "dwmapi")]
extern "system" {
    fn DwmGetWindowAttribute(
        hwnd: Hwnd,
        dwAttribute: u32,
        pvAttribute: *mut core::ffi::c_void,
        cbAttribute: u32,
    ) -> i32;
}

#[cfg(target_os = "windows")]
#[link(name = "kernel32")]
extern "system" {
    #[cfg(test)]
    fn GetModuleHandleW(lpModuleName: *const u16) -> Hinstance;
    fn OpenProcess(dwDesiredAccess: u32, bInheritHandle: i32, dwProcessId: u32) -> Handle;
    fn QueryFullProcessImageNameW(
        hProcess: Handle,
        dwFlags: u32,
        lpExeName: *mut u16,
        lpdwSize: *mut u32,
    ) -> i32;
    fn CloseHandle(hObject: Handle) -> i32;
    #[cfg(test)]
    fn GlobalAlloc(uFlags: u32, dwBytes: usize) -> Handle;
    #[cfg(test)]
    fn GlobalLock(hMem: Handle) -> *mut core::ffi::c_void;
    #[cfg(test)]
    fn GlobalUnlock(hMem: Handle) -> i32;
    #[cfg(test)]
    fn GlobalFree(hMem: Handle) -> Handle;
}

#[cfg(test)]
mod tests {
    use super::{
        windows_clipboard_text, windows_picker_process_is_system, windows_text_units,
        WindowsTextUnit,
    };

    #[test]
    fn windows_picker_excludes_shell_hosts_but_keeps_user_apps() {
        for exe in [
            "SystemSettings.exe",
            "ApplicationFrameHost.exe",
            "TextInputHost.exe",
            "SearchHost.exe",
            "RuntimeBroker.exe",
        ] {
            assert!(windows_picker_process_is_system(exe), "{exe}");
        }
        for exe in ["Telegram.exe", "firefox.exe", "Entropy.exe", "portable.exe"] {
            assert!(!windows_picker_process_is_system(exe), "{exe}");
        }
    }

    #[test]
    fn normalizes_clipboard_line_endings_for_windows() {
        assert_eq!(windows_clipboard_text("line1\nline2"), "line1\r\nline2");
        assert_eq!(windows_clipboard_text("line1\r\nline2"), "line1\r\nline2");
        assert_eq!(windows_clipboard_text("line1\rline2"), "line1\r\nline2");
    }

    #[test]
    fn treats_each_newline_encoding_as_one_windows_line_break() {
        for text in ["line1\nline2", "line1\r\nline2", "line1\rline2"] {
            let units = windows_text_units(text).collect::<Vec<_>>();
            assert_eq!(
                units
                    .iter()
                    .filter(|unit| matches!(unit, WindowsTextUnit::LineBreak))
                    .count(),
                1
            );
            assert_eq!(
                units
                    .iter()
                    .filter_map(|unit| match unit {
                        WindowsTextUnit::Character(ch) => Some(*ch),
                        WindowsTextUnit::LineBreak => None,
                    })
                    .collect::<String>(),
                "line1line2"
            );
        }
    }
}
