//! G29 regression test: ensures `terminal::init_console` is callable and the
//! windows-sys 0.59+ `HANDLE` type is used correctly.
//!
//! The test is compiled on ALL platforms but only exercises the Windows path
//! under `cfg(windows)`.  On non-Windows, it is a no-op that confirms the
//! function is reachable from outside the crate (public re-export check).

#![cfg_attr(not(windows), allow(dead_code))]

use sqlite_graphrag::terminal::{init_console, should_use_ansi};

/// `init_console` must be callable from any platform without panicking.
/// On non-Windows this is a no-op (UTF-8 + ANSI already supported natively);
/// on Windows it routes to `init_windows_console` which uses
/// `windows_sys::Win32::Foundation::{HANDLE, INVALID_HANDLE_VALUE}`.
#[test]
fn init_console_is_callable_on_current_platform() {
    init_console();
}

/// `should_use_ansi` honours `NO_COLOR` and `CLICOLOR_FORCE` env vars.
///
/// We snapshot the current `NO_COLOR` value to restore after the test,
/// because the function reads it eagerly and our test must not pollute the
/// environment for downstream tests running in the same process.
#[test]
fn should_use_ansi_respects_no_color_env() {
    let original = std::env::var_os("NO_COLOR");
    // SAFETY: tests are single-threaded with respect to env mutation here;
    // we restore the original value before returning.
    unsafe {
        std::env::set_var("NO_COLOR", "1");
    }
    assert!(
        !should_use_ansi(),
        "NO_COLOR=1 must force should_use_ansi() == false"
    );
    match original {
        Some(v) => unsafe { std::env::set_var("NO_COLOR", v) },
        None => unsafe { std::env::remove_var("NO_COLOR") },
    }
}

/// On Windows, the `HANDLE` constant from `windows-sys 0.59+` is a
/// `*mut c_void` (not `isize` as in 0.48/0.52).  The fix in `terminal.rs`
/// imports it via `use windows_sys::Win32::Foundation::{HANDLE, INVALID_HANDLE_VALUE}`
/// and uses `.is_null()` + `!= INVALID_HANDLE_VALUE` for type-safe comparison.
///
/// This test simply references the function to make sure the build is wired
/// up; if the type check regresses, `cargo check --target x86_64-pc-windows-msvc`
/// in CI will fail before this test is even reached.
#[cfg(windows)]
#[test]
fn windows_console_init_uses_type_safe_handle_check() {
    use sqlite_graphrag::terminal::init_console;
    init_console();
}
