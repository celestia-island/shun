//! Give an undecorated-with-shadow window its whole surface back — surface and
//! drop shadow at the same time.
//!
//! The wizard's window is `decorations: false` + `shadow: true`. tao keeps the
//! drop shadow by keeping a native frame: it grows the window by the system
//! resize-frame metrics and insets the client area by the same amount — see
//! `calculate_insets_for_dpi` in `tao-0.35.3/src/platform_impl/windows/util.rs`
//! and the `WM_NCCALCSIZE` arm in its `event_loop.rs`. That inset *is* what DWM
//! draws the shadow from, so the webview always stops short of the window edge
//! and the leftover non-client strip reads as a stray padding on the
//! left/bottom/right (the top strip hides under the title bar). tao does not
//! call any DWM API for this, and the window styles are identical with the
//! shadow on or off — which is why the previous `shadow: false` attempt lost
//! the shadow along with the strip.
//!
//! No stylesheet can paint there: the strip is *outside* the page viewport, so
//! the `html`/`body`/`#app` background and negative insets are invisible to it.
//! Both halves therefore have to happen at the window level, which is what this
//! module does:
//!
//! 1. answer `WM_NCCALCSIZE` with "client area == whole window", so the strip
//!    is gone, and
//! 2. re-anchor the drop shadow through DWM — a one-pixel frame extension on
//!    the left/right/bottom edges (`cyTopHeight` stays 0: on Windows 10 any
//!    non-client area at the top makes the system draw a native title bar) with
//!    the border colour suppressed.
//!
//! Step 2 mirrors upstream tauri's `crates/tauri-runtime-wry/src/shadow.rs`
//! (PR #16192, "decouple frameless window shadows from the native frame"), which
//! solves this exact problem the same way. `DWMWA_BORDER_COLOR` exists on
//! Windows 11 and up; on Windows 10 the call fails silently and the system
//! default outline remains.
//!
//! tao delivers its own `WM_NCCALCSIZE` handling through a comctl32 subclass
//! (`SetWindowSubclass`), so this module joins the same chain instead of
//! swapping `GWLP_WNDPROC`: comctl32 calls the most recently installed subclass
//! first, and returning without `DefSubclassProc` keeps tao's inset out of the
//! way. Every other message is forwarded untouched.

use std::ffi::c_void;
use std::mem::size_of;

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Dwm::{
    DWMNCRP_ENABLED, DWMWA_BORDER_COLOR, DWMWA_NCRENDERING_POLICY, DwmSetWindowAttribute,
};
use windows_sys::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetClientRect, GetWindowRect, IsZoomed, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOZORDER,
    SetWindowPos, WM_NCCALCSIZE,
};

/// `MARGINS`, the frame-extension inset `DwmExtendFrameIntoClientArea` takes.
///
/// Declared here rather than imported from `windows_sys::Win32::UI::Controls`:
/// that module sits behind a `windows-sys` feature this crate does not enable,
/// and turning the feature on would change the `windows-sys` fingerprint and
/// rebuild the whole dependency graph for one four-field POD struct. The layout
/// is the SDK's (`MARGINS{dwLeftWidth, dwRightWidth, dwTopHeight,
/// dwBottomHeight}` in declaration order, all `int`).
#[repr(C)]
#[derive(Clone, Copy)]
struct Margins {
    cx_left_width: i32,
    cx_right_width: i32,
    cy_top_height: i32,
    cy_bottom_height: i32,
}

// The SDK's MARGINS is four ints in that order; pin the size so a future field
// edit cannot silently change the ABI we hand to dwmapi.
const _: () = assert!(size_of::<Margins>() == 16);

// Declared here for the same reason as `Margins`: the `windows-sys` binding for
// this call names a type from a feature-gated module. `DwmSetWindowAttribute`
// below keeps its `windows-sys` binding, which uses only core types.
#[link(name = "dwmapi")]
unsafe extern "system" {
    fn DwmExtendFrameIntoClientArea(hwnd: HWND, margins: *const Margins) -> i32;
}

/// Identifies our entry in the window's subclass chain.
const SUBCLASS_ID: usize = 0x4556_4e46; // "EVNF"

/// `DWMWA_COLOR_NONE` — draw no border outline at all (Windows 11+).
const NO_BORDER_COLOR: u32 = 0xFFFF_FFFE;

/// Subclass proc that reports a zero-thickness non-client area.
unsafe extern "system" fn frame_filling_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    unsafe {
        // A maximized window keeps tao's own handling, which reserves room for an
        // auto-hidden taskbar.
        if msg == WM_NCCALCSIZE && IsZoomed(hwnd) == 0 {
            // Both variants of the message hand us the proposed *window* rectangle
            // on entry — `rgrc[0]` when `wparam` is TRUE, the plain `RECT` when it
            // is FALSE — and both expect the client rectangle back. Leaving them
            // untouched and returning 0 therefore means "the client area is the
            // whole window", i.e. no non-client frame at all. Adjusting the rect
            // (as tao does with its insets) is the only thing that creates the
            // strip; copying a sibling rect would pin the client area to a stale
            // size.
            return 0;
        }
        DefSubclassProc(hwnd, msg, wparam, lparam)
    }
}

/// Re-anchors the drop shadow that tao's inset used to produce.
///
/// Best-effort and, crucially, *observable*: a failure here would read as
/// "no strip, no shadow" in the field, so every HRESULT lands on stderr — a
/// GUI-subsystem binary still inherits the console when launched from one.
/// On systems without `DWMWA_BORDER_COLOR` (Windows 10) that one call is
/// expected to fail and only the default outline remains.
unsafe fn anchor_shadow(hwnd: HWND) {
    unsafe {
        // windows-sys's HRESULT is a bare i32: negative = failure.
        let policy = DWMNCRP_ENABLED;
        let policy_hr = DwmSetWindowAttribute(
            hwnd,
            DWMWA_NCRENDERING_POLICY as u32,
            &policy as *const _ as *const c_void,
            size_of::<i32>() as u32,
        );
        if policy_hr < 0 {
            crate::diag!(
                "evernight-installer: DWM non-client rendering policy hr={policy_hr:#010x}"
            );
        }

        let border_color = NO_BORDER_COLOR;
        let border_hr = DwmSetWindowAttribute(
            hwnd,
            DWMWA_BORDER_COLOR as u32,
            &border_color as *const _ as *const c_void,
            size_of::<u32>() as u32,
        );
        if border_hr < 0 {
            // Expected on Windows 10, where the attribute does not exist.
            crate::diag!(
                "evernight-installer: DWM border colour hr={border_hr:#010x} (expected on Win10)"
            );
        }

        // One pixel on the left/right/bottom anchors the shadow; the top stays at
        // zero so Windows 10 does not draw a native title bar into the client area.
        let margins = Margins {
            cx_left_width: 1,
            cx_right_width: 1,
            cy_top_height: 0,
            cy_bottom_height: 1,
        };
        let extend_hr = DwmExtendFrameIntoClientArea(hwnd, &margins);
        if extend_hr < 0 {
            crate::diag!(
                "evernight-installer: DWM frame extension (shadow anchor) hr={extend_hr:#010x}"
            );
        }
    }
}

/// Drops the hidden non-client frame from `hwnd`, shrinks the window down to the
/// client area it already has — so the *visible* window keeps the size the
/// wizard asked for instead of growing by the frame metrics — and re-anchors the
/// drop shadow through DWM.
///
/// Best-effort. The steps are independent: a failure in a later one (the DWM
/// anchor, say) leaves the earlier ones applied — the window keeps its full
/// surface but, in that example, loses the shadow, with the reason on stderr.
pub fn fill(hwnd: HWND) {
    if hwnd.is_null() {
        return;
    }

    unsafe {
        if SetWindowSubclass(hwnd, Some(frame_filling_proc), SUBCLASS_ID, 0) == 0 {
            return;
        }

        // The window rect is currently `client + insets`. Re-apply the client
        // size as the outer size, centred on where the window already sits, and
        // let `SWP_FRAMECHANGED` run `WM_NCCALCSIZE` through the new proc.
        let mut client = RECT::default();
        let mut outer = RECT::default();
        if GetClientRect(hwnd, &mut client) == 0 || GetWindowRect(hwnd, &mut outer) == 0 {
            return;
        }
        let width = client.right - client.left;
        let height = client.bottom - client.top;
        if width <= 0 || height <= 0 {
            return;
        }
        SetWindowPos(
            hwnd,
            std::ptr::null_mut(),
            (outer.left + outer.right) / 2 - width / 2,
            (outer.top + outer.bottom) / 2 - height / 2,
            width,
            height,
            SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );

        anchor_shadow(hwnd);
    }
}
