use eframe::egui;
use std::sync::Arc;
use limiter::Limiter;
use ui::CpuLimiterApp;
use tray_icon::{TrayIconBuilder, menu::{Menu, MenuItem}, Icon};

mod limiter;
mod ui;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let limiter = Arc::new(Limiter::new());
    limiter.start_background_task();

    // Load icon from embedded file
    let icon_bytes = include_bytes!("icon.png");
    let image = image::load_from_memory(icon_bytes)
        .expect("Failed to load icon")
        .into_rgba8();
    let (width, height) = image.dimensions();
    let rgba = image.into_raw();
    let icon = Icon::from_rgba(rgba, width, height)?;

    // Menu
    let tray_menu = Menu::new();
    let quit_i = MenuItem::new("Quit", true, None);
    tray_menu.append(&quit_i)?;
    
    let tray_icon = TrayIconBuilder::new()
        .with_menu(Box::new(tray_menu))
        .with_tooltip("CPU Limiter")
        .with_icon(icon)
        .build()?;

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([400.0, 600.0]),
        ..Default::default()
    };
    
    eframe::run_native(
        "CPU Limiter",
        native_options,
        Box::new(move |cc| {
            // Pass tray_icon to App to keep it alive
            Ok(Box::new(CpuLimiterApp::new(cc, limiter, Some(tray_icon))))
        }),
    ).map_err(|e| e.into())
}
