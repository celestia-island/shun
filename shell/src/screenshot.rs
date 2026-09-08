//! Offline window capture for both shell UIs.
//!
//! `shun-demo-shell --screenshot=<path.png>` renders the UI (webview or
//! egui), captures the window content with `PrintWindow(
//! PW_RENDERFULLCONTENT)` — window-content capture, not desktop
//! automation: no focus stealing, no input synthesis, works when the
//! window is fully occluded — saves the PNG, and exits. Used to check
//! the UIs without touching the interactive session.

use std::path::PathBuf;

use tauri::AppHandle;
use tauri::Manager;

/// Spawns the capture timer for the Tauri UI: waits `delay_ms` for the
/// webview to settle, captures, saves, exits.
pub fn schedule(app: AppHandle, path: PathBuf, delay_ms: u64) {
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(delay_ms));
        // tauri's HWND comes from the `windows` crate; windows-sys wants
        // the raw pointer.
        let hwnd = app
            .get_webview_window("installer")
            .and_then(|window| window.hwnd().ok())
            .map(|hwnd| hwnd.0 as isize)
            .unwrap_or_default();
        run_capture(hwnd, &path);
    });
}

/// Spawns the capture timer for the egui fallback UI: the window is
/// located by its exact title (`FindWindowW`) once it exists.
pub fn schedule_by_title(title: String, path: PathBuf, delay_ms: u64) {
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(delay_ms));
        let hwnd = find_window_by_title(&title).unwrap_or_default();
        run_capture(hwnd, &path);
    });
}

fn run_capture(hwnd: isize, path: &std::path::Path) {
    if hwnd == 0 {
        eprintln!("shun: screenshot failed: target window not found");
        std::process::exit(1);
    }
    if let Err(err) = capture_hwnd(hwnd, path) {
        eprintln!("shun: screenshot failed: {err}");
        std::process::exit(1);
    }
    std::process::exit(0);
}

#[cfg(windows)]
fn find_window_by_title(title: &str) -> Option<isize> {
    use windows_sys::Win32::UI::WindowsAndMessaging::FindWindowW;

    let wide: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
    let hwnd = unsafe { FindWindowW(std::ptr::null(), wide.as_ptr()) };
    (!hwnd.is_null()).then_some(hwnd as isize)
}

#[cfg(not(windows))]
fn find_window_by_title(_title: &str) -> Option<isize> {
    None
}

#[cfg(windows)]
fn capture_hwnd(hwnd: isize, path: &std::path::Path) -> Result<(), String> {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC,
        DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, GetDIBits, ReleaseDC, SRCCOPY, SelectObject,
    };
    // PrintWindow itself lives in the Xps feature group in windows-sys.
    use windows_sys::Win32::Storage::Xps::PrintWindow;
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetClientRect, PW_RENDERFULLCONTENT};

    let hwnd = hwnd as HWND;

    unsafe {
        let mut rect = Default::default();
        GetClientRect(hwnd, &mut rect);
        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;
        if width <= 0 || height <= 0 {
            return Err("window has no client area".into());
        }

        let hdc_window = GetDC(hwnd);
        let hdc_mem = CreateCompatibleDC(hdc_window);
        let bitmap = CreateCompatibleBitmap(hdc_window, width, height);
        let old = SelectObject(hdc_mem, bitmap);

        // PW_RENDERFULLCONTENT pulls the DWM-composited content (webview,
        // GL surface); a plain BitBlt fallback covers the exotic failure
        // path.
        let mut ok = PrintWindow(hwnd, hdc_mem, PW_RENDERFULLCONTENT) != 0;
        if !ok {
            ok = BitBlt(hdc_mem, 0, 0, width, height, hdc_window, 0, 0, SRCCOPY) != 0;
        }

        let mut result = Err("PrintWindow and BitBlt both failed".to_string());
        if ok {
            let mut info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: width,
                    biHeight: -height, // top-down
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut pixels = vec![0u8; (width as usize) * (height as usize) * 4];
            let copied = GetDIBits(
                hdc_mem,
                bitmap,
                0,
                height as u32,
                pixels.as_mut_ptr().cast(),
                &mut info,
                DIB_RGB_COLORS,
            );
            if copied == height {
                // BGRA → RGBA.
                for chunk in pixels.chunks_exact_mut(4) {
                    chunk.swap(0, 2);
                    chunk[3] = 0xff;
                }
                let image = image::RgbaImage::from_raw(width as u32, height as u32, pixels)
                    .ok_or("bitmap buffer size mismatch")?;
                image.save(path).map_err(|e| e.to_string())?;
                result = Ok(());
            } else {
                result = Err(format!("GetDIBits copied {copied} of {height} rows"));
            }
        }

        SelectObject(hdc_mem, old);
        DeleteObject(bitmap);
        DeleteDC(hdc_mem);
        ReleaseDC(hwnd, hdc_window);
        result
    }
}

#[cfg(not(windows))]
fn capture_hwnd(_hwnd: isize, _path: &std::path::Path) -> Result<(), String> {
    Err("--screenshot is implemented for the Windows shell".into())
}
