#[cfg(any(target_os = "macos", target_os = "windows"))]
use crate::domain::about;

// native menu bar; None where the UI draws its own (Linux)
pub struct AppMenu(Option<Inner>);

#[cfg(any(target_os = "macos", target_os = "windows"))]
type Inner = muda::Menu;
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
type Inner = ();

impl AppMenu {
    // macOS: must run after the event loop starts, or winit's default menu replaces it
    // `hwnd` is the Win32 window handle (Windows only)
    pub fn install(hwnd: Option<isize>, icon_rgba: Vec<u8>, icon_size: u32) -> Self {
        Self(install(hwnd, icon_rgba, icon_size))
    }

    pub fn is_native(&self) -> bool {
        self.0.is_some()
    }

    pub fn take_setup_guide_request(&self) -> bool {
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        {
            let mut found = false;
            while let Ok(event) = muda::MenuEvent::receiver().try_recv() {
                if event.id == "proxyproxy.setup-guide" {
                    found = true;
                }
            }
            found
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            false
        }
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn metadata(icon: Option<muda::Icon>) -> muda::AboutMetadata {
    muda::AboutMetadata {
        name: Some(about::NAME.into()),
        version: Some(about::VERSION.into()),
        comments: Some(about::DESCRIPTION.into()),
        authors: Some(vec![about::AUTHOR.into()]),
        license: Some(about::LICENSE.into()),
        copyright: Some(about::COPYRIGHT.into()),
        icon,
        ..Default::default()
    }
}

#[cfg(target_os = "macos")]
fn install(_hwnd: Option<isize>, icon_rgba: Vec<u8>, icon_size: u32) -> Option<Inner> {
    use muda::{Icon, Menu, MenuItem, PredefinedMenuItem, Submenu};
    let menu = Menu::new();
    let icon = Icon::from_rgba(icon_rgba, icon_size, icon_size).ok();
    let meta = || metadata(icon.clone());

    let app = Submenu::new(about::NAME, true);
    app.append_items(&[
        &PredefinedMenuItem::about(None, Some(meta())),
        &PredefinedMenuItem::separator(),
        &PredefinedMenuItem::services(None),
        &PredefinedMenuItem::separator(),
        &PredefinedMenuItem::hide(None),
        &PredefinedMenuItem::hide_others(None),
        &PredefinedMenuItem::show_all(None),
        &PredefinedMenuItem::separator(),
        &PredefinedMenuItem::quit(None),
    ])
    .ok()?;
    menu.append(&app).ok()?;

    let edit = Submenu::new("Edit", true);
    edit.append_items(&[
        &PredefinedMenuItem::undo(None),
        &PredefinedMenuItem::redo(None),
        &PredefinedMenuItem::separator(),
        &PredefinedMenuItem::cut(None),
        &PredefinedMenuItem::copy(None),
        &PredefinedMenuItem::paste(None),
        &PredefinedMenuItem::select_all(None),
    ])
    .ok()?;
    menu.append(&edit).ok()?;

    let window = Submenu::new("Window", true);
    window
        .append_items(&[
            &PredefinedMenuItem::minimize(None),
            &PredefinedMenuItem::maximize(None),
            &PredefinedMenuItem::fullscreen(None),
        ])
        .ok()?;
    menu.append(&window).ok()?;
    window.set_as_windows_menu_for_nsapp();

    let help = Submenu::new("Help", true);
    help.append_items(&[
        &MenuItem::with_id("proxyproxy.setup-guide", "Setup Guide", true, None),
        &PredefinedMenuItem::separator(),
        &PredefinedMenuItem::about(Some("About proxyproxy"), Some(meta())),
    ])
    .ok()?;
    menu.append(&help).ok()?;
    help.set_as_help_menu_for_nsapp();

    menu.init_for_nsapp();
    Some(menu)
}

#[cfg(target_os = "windows")]
fn install(hwnd: Option<isize>, icon_rgba: Vec<u8>, icon_size: u32) -> Option<Inner> {
    use muda::{Icon, Menu, MenuItem, PredefinedMenuItem, Submenu};
    let hwnd = hwnd?;
    let menu = Menu::new();
    let icon = Icon::from_rgba(icon_rgba, icon_size, icon_size).ok();
    let help = Submenu::new("Help", true);
    help.append_items(&[
        &MenuItem::with_id("proxyproxy.setup-guide", "Setup Guide", true, None),
        &PredefinedMenuItem::separator(),
        &PredefinedMenuItem::about(Some("About proxyproxy"), Some(metadata(icon))),
    ])
    .ok()?;
    menu.append(&help).ok()?;
    // SAFETY: hwnd belongs to the live app window and the menu outlives it (held by App)
    unsafe { menu.init_for_hwnd(hwnd) }.ok()?;
    Some(menu)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn install(_hwnd: Option<isize>, _icon_rgba: Vec<u8>, _icon_size: u32) -> Option<Inner> {
    None
}
