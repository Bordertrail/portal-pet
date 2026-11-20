use tauri::WebviewWindow;

#[cfg(target_os = "macos")]
pub fn show(window: &WebviewWindow) -> Result<(), String> {
    let raw_window = window.ns_window().map_err(|error| error.to_string())?;
    // The main-thread caller still owns the Tauri window. Tauri exposes its
    // underlying NSWindow, which remains alive for this non-activating call.
    unsafe {
        (&*raw_window.cast::<objc2_app_kit::NSWindow>()).orderFrontRegardless();
    }
    Ok(())
}

#[cfg(target_os = "windows")]
pub fn show(window: &WebviewWindow) -> Result<(), String> {
    use windows::Win32::UI::WindowsAndMessaging::{
        SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SWP_SHOWWINDOW, SetWindowPos,
    };

    let hwnd = window.hwnd().map_err(|error| error.to_string())?;
    // The live HWND was retrieved from the window above. The existing size,
    // position and stacking order are retained.
    unsafe {
        SetWindowPos(
            hwnd,
            None,
            0,
            0,
            0,
            0,
            SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_SHOWWINDOW,
        )
    }
    .map_err(|error| error.to_string())
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn show(window: &WebviewWindow) -> Result<(), String> {
    window.show().map_err(|error| error.to_string())
}
