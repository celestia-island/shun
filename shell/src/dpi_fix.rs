//! The winit-0.30 cross-DPI drag guard — Windows only.
//!
//! eframe 0.32 pins winit 0.30, whose `WM_DPICHANGED` handler replaces
//! the OS-suggested rectangle with a "conservative" one of its own
//! (logical-size preserving, drag-anchored, then nudged onto the
//! monitor `MonitorFromWindow` names). On Windows 11 24H2+ that monitor
//! query can name the WRONG monitor, so the nudge pushes the window
//! back across the boundary, which fires another `WM_DPICHANGED`
//! before the first finishes — the window grows a step per crossing
//! and walks away from the cursor mid-drag (rust-windowing/winit#4041;
//! the egui face of it is emilk/egui#7648). Upstream removed the
//! custom re-adjustment for the 0.31 line (PR #4341) — exactly the
//! Chromium behavior: apply the suggested rect Windows hands you.
//!
//! Until the eframe/winit bump, this module lands the same semantics
//! as a comctl32 subclass (the same chain `window_frame.rs` joins for
//! the tauri windows): the OUTERMOST `WM_DPICHANGED` is forwarded to
//! winit untouched — its scale-factor bookkeeping and the
//! `ScaleFactorChanged`/`Resized` events egui renders from stay
//! correct — and after winit's handler (and any feedback loop it
//! spawned synchronously inside `DefSubclassProc`) has settled, the
//! window is re-pinned to the rect the OS suggested for the ORIGINAL
//! logical size: cursor-anchored, correct size, on the correct
//! monitor. A window already sitting on its suggested rect triggers no
//! further DPI change, so the sequence converges. Nested invocations
//! (the loop's own crossings) only forward; re-pinning to the
//! outermost suggestion is what breaks the compounding.

#[cfg(windows)]
pub(crate) fn install(frame: &eframe::Frame) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let Ok(handle) = frame.window_handle() else {
        return;
    };
    let RawWindowHandle::Win32(win) = handle.as_raw() else {
        return;
    };
    // SAFETY: plain subclass installation on our own window handle; the
    // id is this module's alone (see window_frame.rs for the chain).
    unsafe {
        let hwnd = win.hwnd.get() as windows_sys::Win32::Foundation::HWND;
        if windows_sys::Win32::UI::Shell::SetWindowSubclass(
            hwnd,
            Some(dpi_guard_proc),
            SUBCLASS_ID,
            0,
        ) == 0
        {
            crate::diag!("shun: dpi guard subclass not installed");
        }
    }
}

/// Resizes (centered on the window's CURRENT monitor work area) in
/// PHYSICAL pixels, bypassing egui's logical
/// [`egui::ViewportCommand::InnerSize`]: on DPI-scaled displays the
/// eframe-0.32/winit-0.30 logical path can grow the window without
/// growing the render surface — the enlarged region then stays
/// transparent (the passive failure pane's one-shot enlarge hit exactly
/// that). A physical `SetWindowPos` rides the ordinary `WM_SIZE` path
/// both layers resize on, the same one a user drag takes. Centering
/// uses the window's own monitor (work area), never the primary — a
/// secondary screen keeps the window on-screen.
#[cfg(windows)]
pub(crate) fn resize_centered(frame: &eframe::Frame, width: f32, height: f32, scale: f32) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows_sys::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOOWNERZORDER, SWP_NOZORDER, SetWindowPos,
    };
    let Ok(handle) = frame.window_handle() else {
        return;
    };
    let RawWindowHandle::Win32(win) = handle.as_raw() else {
        return;
    };
    let hwnd = win.hwnd.get() as windows_sys::Win32::Foundation::HWND;
    let (w, h) = ((width * scale) as i32, (height * scale) as i32);
    // SAFETY: plain monitor/window queries and one SetWindowPos on our
    // own window handle.
    unsafe {
        let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(monitor, &mut info) == 0 {
            // No work area to center in — grow in place instead of
            // jumping to a guessed position.
            SetWindowPos(
                hwnd,
                std::ptr::null_mut(),
                0,
                0,
                w,
                h,
                SWP_NOZORDER | SWP_NOOWNERZORDER | SWP_NOACTIVATE | SWP_NOMOVE,
            );
            return;
        }
        let area = info.rcWork;
        let x = area.left + (area.right - area.left - w) / 2;
        let y = area.top + (area.bottom - area.top - h) / 2;
        SetWindowPos(
            hwnd,
            std::ptr::null_mut(),
            x,
            y,
            w,
            h,
            SWP_NOZORDER | SWP_NOOWNERZORDER | SWP_NOACTIVATE,
        );
    }
}

#[cfg(windows)]
const SUBCLASS_ID: usize = 0x4450_4947; // "DPIG"

#[cfg(windows)]
static IN_OUTERMOST: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[cfg(windows)]
unsafe extern "system" fn dpi_guard_proc(
    hwnd: windows_sys::Win32::Foundation::HWND,
    msg: u32,
    wparam: windows_sys::Win32::Foundation::WPARAM,
    lparam: windows_sys::Win32::Foundation::LPARAM,
    _id: usize,
    _data: usize,
) -> windows_sys::Win32::Foundation::LRESULT {
    use std::sync::atomic::Ordering;
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::UI::Shell::DefSubclassProc;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SWP_NOACTIVATE, SWP_NOOWNERZORDER, SWP_NOZORDER, SetWindowPos, WM_DPICHANGED,
    };

    unsafe {
        if msg == WM_DPICHANGED && !IN_OUTERMOST.load(Ordering::Relaxed) {
            // Copy BEFORE forwarding: winit's SetWindowPos inside the
            // forwarded call rewrites what a later lparam would point at.
            let suggested = *(lparam as *const RECT);
            IN_OUTERMOST.store(true, Ordering::Relaxed);
            let result = DefSubclassProc(hwnd, msg, wparam, lparam);
            IN_OUTERMOST.store(false, Ordering::Relaxed);
            // Re-pin to the OS suggestion for the original logical size.
            // Best-effort: a failure leaves winit's placement standing
            // (the pre-guard behavior, no worse).
            SetWindowPos(
                hwnd,
                std::ptr::null_mut(),
                suggested.left,
                suggested.top,
                suggested.right - suggested.left,
                suggested.bottom - suggested.top,
                SWP_NOZORDER | SWP_NOOWNERZORDER | SWP_NOACTIVATE,
            );
            return result;
        }
        DefSubclassProc(hwnd, msg, wparam, lparam)
    }
}

#[cfg(not(windows))]
pub(crate) fn install(_frame: &eframe::Frame) {}
