//! Cross-platform terminal initialization: UTF-8, ANSI colors, NO_COLOR.

/// Initializes the console for correct UTF-8 output and ANSI escape
/// support.  On non-Windows platforms this is a no-op because modern
/// Unix terminals handle both natively.
pub fn init_console() {
    #[cfg(windows)]
    init_windows_console();
}

#[cfg(windows)]
fn init_windows_console() {
    use windows_sys::Win32::System::Console::{
        GetConsoleMode, GetStdHandle, SetConsoleCP, SetConsoleMode, SetConsoleOutputCP,
        ENABLE_VIRTUAL_TERMINAL_PROCESSING, STD_ERROR_HANDLE, STD_OUTPUT_HANDLE,
    };
    const CP_UTF8: u32 = 65001;

    // SAFETY: Win32 console functions are safe to call from a single-threaded
    // context before any output occurs.  GetStdHandle returns
    // INVALID_HANDLE_VALUE on failure (checked below); SetConsoleMode failure
    // is silently tolerated so the CLI degrades to plain text.
    unsafe {
        SetConsoleOutputCP(CP_UTF8);
        SetConsoleCP(CP_UTF8);

        for handle_id in [STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
            let handle = GetStdHandle(handle_id);
            if handle != 0 && handle as isize != -1 {
                let mut mode: u32 = 0;
                if GetConsoleMode(handle, &mut mode) != 0 {
                    let _ = SetConsoleMode(handle, mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING);
                }
            }
        }
    }
}

/// Returns whether ANSI escape codes should be emitted to stderr.
///
/// Precedence:
/// 1. `NO_COLOR` set (any value) → false (<https://no-color.org> standard)
/// 2. `CLICOLOR_FORCE=1` → true (force colors even without TTY)
/// 3. stderr is a terminal → true
/// 4. fallback → false
pub fn should_use_ansi() -> bool {
    if std::env::var_os("NO_COLOR").is_some() {
        return false;
    }
    if std::env::var("CLICOLOR_FORCE").ok().as_deref() == Some("1") {
        return true;
    }
    std::io::IsTerminal::is_terminal(&std::io::stderr())
}
