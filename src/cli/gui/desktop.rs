//! AlphaCode Native Desktop Window Host using `tao` + `wry`
//!
//! Provides a dedicated native desktop window embedding Microsoft Edge WebView2
//! on Windows and WKWebView on macOS.
//!
//! Characteristics:
//! - Native window with dedicated title, icon, and sizing (1320x840)
//! - Zero browser chrome (no tabs, no URL bar, no browser menus)
//! - Isolated WebView2 profile in `%LOCALAPPDATA%\AlphaCode\webview_data`
//! - Resilient fallback to Chromium App mode if native windowing cannot initialize

use anyhow::Result;
use std::path::PathBuf;

/// Check if the current process was launched directly from Windows Explorer (double-click)
/// rather than from an existing terminal/console shell.
#[cfg(windows)]
pub fn is_launched_from_explorer() -> bool {
    unsafe {
        use windows_sys::Win32::System::Console::GetConsoleProcessList;
        let mut pids = [0u32; 2];
        let count = GetConsoleProcessList(pids.as_mut_ptr(), 2);
        count <= 1
    }
}

#[cfg(not(windows))]
pub fn is_launched_from_explorer() -> bool {
    false
}

/// Hide the console window if this process was launched directly from Windows Explorer.
#[cfg(windows)]
pub fn hide_console_if_standalone() {
    if is_launched_from_explorer() {
        unsafe {
            use windows_sys::Win32::System::Console::GetConsoleWindow;
            use windows_sys::Win32::UI::WindowsAndMessaging::{SW_HIDE, ShowWindow};
            let hwnd = GetConsoleWindow();
            if !hwnd.is_null() {
                ShowWindow(hwnd, SW_HIDE);
            }
        }
    }
}

#[cfg(not(windows))]
pub fn hide_console_if_standalone() {}

/// Resolve persistent webview data directory for storing session state,
/// local storage, and caching across app restarts.
fn get_webview_data_dir() -> PathBuf {
    if let Ok(local_app) = std::env::var("LOCALAPPDATA") {
        PathBuf::from(local_app)
            .join("AlphaCode")
            .join("webview_data")
    } else if let Ok(home) = std::env::var("USERPROFILE") {
        PathBuf::from(home).join(".alphacode").join("webview_data")
    } else {
        std::env::temp_dir().join("alphacode-webview-data")
    }
}

/// Fallback to launching Microsoft Edge or Chrome in standalone `--app=` mode.
/// This creates an OS desktop window without tabs or URL bar even if Tao/Wry fails.
pub fn launch_app_mode_window(url: &str) -> bool {
    let edge_paths = [
        r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
        r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
    ];

    let chrome_paths = [
        r"C:\Program Files\Google\Chrome\Application\chrome.exe",
        r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
    ];

    let candidates = edge_paths.iter().chain(chrome_paths.iter());

    for path_str in candidates {
        let p = std::path::Path::new(path_str);
        if p.exists() {
            let user_data = get_webview_data_dir();
            let app_arg = format!("--app={url}");
            let user_arg = format!("--user-data-dir={}", user_data.display());

            let mut cmd = std::process::Command::new(p);
            cmd.arg(&app_arg)
                .arg(&user_arg)
                .arg("--window-size=1360,860")
                .arg("--app-id=AlphaCode.Desktop");

            if cmd.spawn().is_ok() {
                return true;
            }
        }
    }

    false
}

#[cfg(any(windows, target_os = "macos"))]
#[derive(Debug, Clone, Copy)]
pub enum UserEvent {
    Minimize,
    Maximize,
    Close,
    Drag,
}

#[cfg(any(windows, target_os = "macos"))]
fn load_desktop_icon() -> Option<tao::window::Icon> {
    const ICON_PNG: &[u8] = include_bytes!("../../../assets/app-icons/alphacode.png");
    let decoded = image::load_from_memory(ICON_PNG).ok()?.into_rgba8();
    let (width, height) = decoded.dimensions();
    tao::window::Icon::from_rgba(decoded.into_raw(), width, height).ok()
}

/// Launch the native desktop window using `tao` and `wry`.
/// This runs the event loop on the calling thread on supported platforms.
#[cfg(any(windows, target_os = "macos"))]
pub fn run_desktop_window(url: &str) -> Result<()> {
    hide_console_if_standalone();

    #[cfg(windows)]
    unsafe {
        // Enable Windows Per-Monitor V2 DPI awareness for crisp WebView2 rendering.
        #[link(name = "user32")]
        unsafe extern "system" {
            fn SetProcessDpiAwarenessContext(value: isize) -> i32;
        }
        let _ = SetProcessDpiAwarenessContext(-4);
    }

    use tao::dpi::LogicalSize;
    use tao::event::Event;
    use tao::event_loop::{ControlFlow, EventLoopBuilder};
    #[cfg(target_os = "windows")]
    use tao::platform::windows::EventLoopBuilderExtWindows;
    #[cfg(target_os = "windows")]
    use tao::platform::windows::WindowBuilderExtWindows;
    use tao::window::WindowBuilder;
    use wry::WebViewBuilder;

    let mut builder = EventLoopBuilder::<UserEvent>::with_user_event();
    #[cfg(target_os = "windows")]
    builder.with_any_thread(true);
    let event_loop = builder.build();
    let proxy = event_loop.create_proxy();

    // Determine initial dimensions relative to monitor resolution
    let (initial_width, initial_height) = {
        let monitor = event_loop
            .primary_monitor()
            .or_else(|| event_loop.available_monitors().next());
        if let Some(m) = monitor {
            let screen_logical = m.size().to_logical::<f64>(m.scale_factor());
            let max_w = (screen_logical.width * 0.92).max(980.0);
            let max_h = (screen_logical.height * 0.88).max(640.0);
            (1320.0_f64.min(max_w), 840.0_f64.min(max_h))
        } else {
            (1320.0, 840.0)
        }
    };

    let app_icon = load_desktop_icon();
    let mut window_builder = WindowBuilder::new()
        .with_title("AlphaCode")
        .with_inner_size(LogicalSize::new(initial_width, initial_height))
        .with_min_inner_size(LogicalSize::new(980.0, 600.0))
        .with_resizable(true)
        .with_decorations(false)
        .with_window_icon(app_icon.clone())
        .with_background_color((12, 14, 20, 255));

    #[cfg(target_os = "windows")]
    {
        window_builder = window_builder.with_taskbar_icon(app_icon);
    }

    #[cfg(target_os = "macos")]
    {
        use tao::platform::macos::WindowBuilderExtMacOS;
        window_builder = window_builder
            .with_decorations(true)
            .with_title_hidden(true)
            .with_titlebar_transparent(true)
            .with_fullsize_content_view(true)
            .with_has_shadow(true)
            .with_movable_by_window_background(true);
    }

    let window = window_builder.build(&event_loop)?;

    #[cfg(windows)]
    {
        use tao::platform::windows::WindowExtWindows;
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            GWL_STYLE, GetWindowLongW, SWP_FRAMECHANGED, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
            SetWindowLongW, SetWindowPos, WS_MAXIMIZEBOX, WS_MINIMIZEBOX, WS_THICKFRAME,
        };

        let hwnd = window.hwnd() as windows_sys::Win32::Foundation::HWND;
        unsafe {
            let mut style = GetWindowLongW(hwnd, GWL_STYLE);
            style |= (WS_THICKFRAME | WS_MAXIMIZEBOX | WS_MINIMIZEBOX) as i32;
            SetWindowLongW(hwnd, GWL_STYLE, style);
            SetWindowPos(
                hwnd,
                std::ptr::null_mut(),
                0,
                0,
                0,
                0,
                SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER,
            );
        }
    }

    let webview_data = get_webview_data_dir();
    let _ = std::fs::create_dir_all(&webview_data);
    let mut web_context = wry::WebContext::new(Some(webview_data));

    let proxy_clone = proxy.clone();
    let webview = WebViewBuilder::with_web_context(&mut web_context)
        .with_url(url)
        .with_background_color((12, 14, 20, 255))
        .with_ipc_handler(move |req| {
            let body = req.body().trim();
            match body {
                "minimize" => {
                    let _ = proxy_clone.send_event(UserEvent::Minimize);
                }
                "maximize" => {
                    let _ = proxy_clone.send_event(UserEvent::Maximize);
                }
                "close" => {
                    let _ = proxy_clone.send_event(UserEvent::Close);
                }
                "drag" => {
                    let _ = proxy_clone.send_event(UserEvent::Drag);
                }
                _ => {}
            }
        })
        .build(&window)?;

    event_loop.run(move |event, _, control_flow| {
        let _keep_alive = &webview;
        *control_flow = ControlFlow::Wait;

        match event {
            Event::UserEvent(UserEvent::Minimize) => {
                #[cfg(windows)]
                unsafe {
                    use tao::platform::windows::WindowExtWindows;
                    use windows_sys::Win32::UI::WindowsAndMessaging::{SW_MINIMIZE, ShowWindow};
                    let hwnd = window.hwnd() as windows_sys::Win32::Foundation::HWND;
                    ShowWindow(hwnd, SW_MINIMIZE);
                }
                #[cfg(not(windows))]
                window.set_minimized(true);
            }
            Event::UserEvent(UserEvent::Maximize) => {
                #[cfg(windows)]
                unsafe {
                    use tao::platform::windows::WindowExtWindows;
                    use windows_sys::Win32::UI::WindowsAndMessaging::{
                        IsZoomed, SW_MAXIMIZE, SW_RESTORE, ShowWindow,
                    };
                    let hwnd = window.hwnd() as windows_sys::Win32::Foundation::HWND;
                    if IsZoomed(hwnd) != 0 {
                        ShowWindow(hwnd, SW_RESTORE);
                    } else {
                        ShowWindow(hwnd, SW_MAXIMIZE);
                    }
                }
                #[cfg(not(windows))]
                window.set_maximized(!window.is_maximized());
            }
            Event::UserEvent(UserEvent::Close) => {
                *control_flow = ControlFlow::Exit;
            }
            Event::UserEvent(UserEvent::Drag) => {
                #[cfg(windows)]
                unsafe {
                    use tao::platform::windows::WindowExtWindows;
                    use windows_sys::Win32::UI::WindowsAndMessaging::{
                        SendMessageW, WM_SYSCOMMAND,
                    };
                    unsafe extern "system" {
                        fn ReleaseCapture() -> i32;
                    }
                    let hwnd = window.hwnd() as windows_sys::Win32::Foundation::HWND;
                    ReleaseCapture();
                    SendMessageW(hwnd, WM_SYSCOMMAND, 0xF012, 0);
                }
                #[cfg(not(windows))]
                let _ = window.drag_window();
            }
            Event::WindowEvent {
                event: tao::event::WindowEvent::CloseRequested,
                ..
            } => {
                *control_flow = ControlFlow::Exit;
            }
            _ => {}
        }
    });
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn run_desktop_window(_url: &str) -> Result<()> {
    anyhow::bail!("Native WebView desktop window is only supported on Windows and macOS")
}
