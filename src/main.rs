use eframe::egui;
use limiter::Limiter;
use single_instance::SingleInstance;
use std::sync::Arc;
use tray_icon::{
    Icon, TrayIconBuilder,
    menu::{Menu, MenuId, MenuItem},
};
use ui::CpuLimiterApp;

mod limiter;
mod ui;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    // Garantir que apenas uma instância está rodando
    let instance = SingleInstance::new("cpu-limiter-app")?;
    if !instance.is_single() {
        eprintln!("Outra instância do CPU Limiter já está rodando!");
        std::process::exit(1);
    }

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([400.0, 600.0]),
        ..Default::default()
    };

    eframe::run_native(
        "CPU Limiter",
        native_options,
        Box::new(move |_cc| {
            let limiter = Arc::new(Limiter::new());
            limiter.start_background_task();

            // Load icon from embedded file
            let icon_bytes = include_bytes!("icon.png");
            let image = image::load_from_memory(icon_bytes)
                .expect("Failed to load icon")
                .into_rgba8();
            let (width, height) = image.dimensions();
            let rgba = image.into_raw();
            let icon = Icon::from_rgba(rgba, width, height)
                .expect("Failed to create icon from RGBA");

            // Menu
            let tray_menu = Menu::new();
            let quit_menu_id = MenuId::new("tray-quit");
            let quit_i = MenuItem::with_id(quit_menu_id.clone(), "Sair", true, None);
            tray_menu.append(&quit_i)
                .expect("Failed to append quit menu item");

            let tray_icon = TrayIconBuilder::new()
                .with_menu(Box::new(tray_menu))
                .with_tooltip("CPU Limiter")
                .with_icon(icon)
                .with_menu_on_left_click(false)
                .build()
                .ok();

            // Manter a instância viva para manter o lock
            let _instance = instance;

            Ok(Box::new(CpuLimiterApp::new(
                _cc,
                limiter.clone(),
                tray_icon,
                quit_menu_id,
            )))
        }),
    )
    .map_err(|e| e.into())
}
