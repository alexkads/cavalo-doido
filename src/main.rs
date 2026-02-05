use eframe::egui;
use limiter::Limiter;
use std::sync::Arc;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use tray_icon::{
    Icon, TrayIconBuilder,
    menu::{Menu, MenuId, MenuItem},
};
use ui::CpuLimiterApp;

mod limiter;
mod ui;

// Estrutura para gerenciar o lock de instância única
struct SingleInstanceLock {
    _file: File,
    lock_path: PathBuf,
}

impl SingleInstanceLock {
    fn try_acquire() -> Result<Self, Box<dyn std::error::Error>> {
        let lock_path = std::env::temp_dir().join("cpu-limiter.lock");
        
        // Tenta criar o arquivo de lock exclusivamente
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock_path);
        
        match file {
            Ok(mut f) => {
                // Escreve o PID no arquivo de lock
                writeln!(f, "{}", std::process::id())?;
                Ok(Self {
                    _file: f,
                    lock_path,
                })
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                // Arquivo já existe, verifica se o processo ainda está ativo
                if let Ok(content) = std::fs::read_to_string(&lock_path) {
                    if let Ok(pid) = content.trim().parse::<i32>() {
                        // Verifica se o processo ainda existe
                        use std::process::Command;
                        let output = Command::new("ps")
                            .arg("-p")
                            .arg(pid.to_string())
                            .output();
                        
                        if let Ok(output) = output {
                            if output.status.success() {
                                // Processo ainda existe
                                return Err("Outra instância do CPU Limiter já está rodando!".into());
                            }
                        }
                    }
                }
                
                // Processo não existe mais, remove o lock antigo e tenta novamente
                let _ = std::fs::remove_file(&lock_path);
                let mut f = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&lock_path)?;
                writeln!(f, "{}", std::process::id())?;
                Ok(Self {
                    _file: f,
                    lock_path,
                })
            }
            Err(e) => Err(e.into()),
        }
    }
}

impl Drop for SingleInstanceLock {
    fn drop(&mut self) {
        // Remove o arquivo de lock quando a aplicação terminar
        let _ = std::fs::remove_file(&self.lock_path);
    }
}

#[cfg(target_os = "macos")]
fn set_tray_fixed_length(tray_icon: &tray_icon::TrayIcon) {
    if let Some(status_item) = tray_icon.ns_status_item() {
        // Fixed width prevents icon shifting when title changes.
        status_item.setLength(60.0);
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    // Garantir que apenas uma instância está rodando
    let _instance_lock = match SingleInstanceLock::try_acquire() {
        Ok(lock) => lock,
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    };

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

            // Load tray icon from embedded file (template icon for macOS)
            let icon_bytes = include_bytes!("tray.png");
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
                .with_icon_as_template(true)
                .with_menu_on_left_click(false)
                .build()
                .ok();

            #[cfg(target_os = "macos")]
            if let Some(tray_icon) = &tray_icon {
                set_tray_fixed_length(tray_icon);
            }

            // Manter o lock vivo durante toda a execução
            let _lock = _instance_lock;

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
