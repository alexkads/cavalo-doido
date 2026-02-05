use crate::limiter::Limiter;
use eframe::egui;
use std::sync::Arc;
use std::time::{Duration, Instant};
use sysinfo::System;
use tray_icon::{
    MouseButton, MouseButtonState, TrayIcon, TrayIconEvent,
    menu::{MenuEvent, MenuId},
};

pub struct CpuLimiterApp {
    limiter: Arc<Limiter>,
    system: System,
    last_update: Instant,
    filter_text: String,
    // (Pid, Name, CPU%)
    cached_processes: Vec<(i32, String, f32)>,
    selected_pid: Option<i32>,
    limit_value: u32,
    is_active: bool,
    global_mode: bool,
    pub _tray_icon: Option<TrayIcon>,
    quit_menu_id: MenuId,
    allow_close: bool,
}

impl CpuLimiterApp {
    pub fn new(
        cc: &eframe::CreationContext,
        limiter: Arc<Limiter>,
        tray_icon: Option<TrayIcon>,
        quit_menu_id: MenuId,
    ) -> Self {
        // --- Visual Customization ---
        configure_visuals(&cc.egui_ctx);

        Self {
            limiter,
            system: System::new_all(),
            last_update: Instant::now(),
            filter_text: String::new(),
            cached_processes: Vec::new(),
            selected_pid: None,
            limit_value: 50,
            is_active: false,
            global_mode: false,
            _tray_icon: tray_icon,
            quit_menu_id,
            allow_close: false,
        }
    }

    fn refresh_processes(&mut self) {
        self.system
            .refresh_processes(sysinfo::ProcessesToUpdate::All, true);
        self.system.refresh_cpu_all();

        // Collect and sort
        let mut procs: Vec<_> = self
            .system
            .processes()
            .iter()
            .map(|(pid, proc)| {
                (
                    pid.as_u32() as i32,
                    proc.name().to_string_lossy().to_string(),
                    proc.cpu_usage(),
                )
            })
            .collect();

        // Sort by CPU usage descending
        procs.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));

        self.cached_processes = procs;
    }

    fn handle_menu_events(&mut self, ctx: &egui::Context) {
        let receiver = MenuEvent::receiver();
        while let Ok(event) = receiver.try_recv() {
            if event.id == self.quit_menu_id {
                self.allow_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    fn handle_tray_events(&mut self, ctx: &egui::Context) {
        let receiver = TrayIconEvent::receiver();
        while let Ok(event) = receiver.try_recv() {
            match event {
                TrayIconEvent::Click {
                    button,
                    button_state,
                    ..
                } => {
                    if button == MouseButton::Left && button_state == MouseButtonState::Up {
                        self.show_window(ctx);
                    }
                }
                TrayIconEvent::DoubleClick { button, .. } => {
                    if button == MouseButton::Left {
                        self.show_window(ctx);
                    }
                }
                _ => {}
            }
        }
    }

    fn show_window(&self, ctx: &egui::Context) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    }
}

impl eframe::App for CpuLimiterApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Refresh every 1s
        if self.last_update.elapsed() > Duration::from_secs(1) {
            self.refresh_processes();
            self.last_update = Instant::now();
        }

        self.handle_menu_events(ctx);
        self.handle_tray_events(ctx);

        if ctx.input(|i| i.viewport().close_requested()) && !self.allow_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }

        // --- Main UI Layout ---
        
        egui::CentralPanel::default().show(ctx, |ui| {
            // Header
            ui.add_space(10.0);
            ui.vertical_centered(|ui| {
                ui.heading(egui::RichText::new("⚡ CPU Limiter").size(24.0).strong());
                ui.label(egui::RichText::new("Monitor & Control Process Usage").weak().italics());
            });
            ui.add_space(15.0);

            // Status Card
            let status_color = if self.is_active {
                egui::Color32::from_rgb(0, 200, 100) // Green
            } else {
                egui::Color32::from_rgb(200, 200, 200) // Gray
            };
            
            egui::Frame::group(ui.style())
                .fill(ui.style().visuals.window_fill())
                .stroke(egui::Stroke::new(1.0, ui.style().visuals.widgets.noninteractive.bg_stroke.color))
                .inner_margin(10.0)
                .corner_radius(8)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Status:").strong());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                             if self.is_active {
                                ui.label(egui::RichText::new("ACTIVE").color(status_color).strong());
                            } else {
                                ui.label(egui::RichText::new("INACTIVE").color(status_color));
                            }
                        });
                    });
                });

            ui.add_space(10.0);

            // Controls Card
            egui::Frame::group(ui.style())
                .inner_margin(10.0)
                .corner_radius(8)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(egui::RichText::new("Configuration").strong());
                    ui.add_space(5.0);

                    // Limit Slider
                    ui.horizontal(|ui| {
                        ui.label("Limit Target:");
                        ui.add(egui::Slider::new(&mut self.limit_value, 1..=99).text("% CPU"))
                            .changed().then(|| {
                                self.limiter.set_limit(self.limit_value);
                            });
                    });
                    
                    ui.add_space(5.0);

                    // Global Mode Checkbox
                    if ui.checkbox(&mut self.global_mode, "Global Auto-Limit Mode")
                        .on_hover_text("Automatically limit the highest CPU consuming process")
                        .changed() 
                    {
                        if self.global_mode {
                            self.limiter.set_global(self.limit_value);
                        } else {
                            if let Some(pid) = self.selected_pid {
                                self.limiter.set_target(pid);
                            }
                        }
                    }

                    ui.add_space(10.0);

                    // Big Start/Stop Button
                    let btn_text = if self.is_active { "⏹ Stop Limiting" } else { "▶ Start Limiting" };
                    let btn = egui::Button::new(egui::RichText::new(btn_text).size(16.0).color(egui::Color32::WHITE))
                        .min_size(egui::vec2(ui.available_width(), 32.0))
                        .fill(if self.is_active { egui::Color32::from_rgb(200, 60, 60) } else { egui::Color32::from_rgb(60, 140, 200) });
                    
                    if ui.add(btn).clicked() {
                        self.is_active = !self.is_active;
                        self.limiter.toggle(self.is_active);
                    }
                });

            ui.add_space(10.0);

            // Process List Section
            ui.label(egui::RichText::new("Processes").strong());
            
            // Search Bar
            ui.horizontal(|ui| {
                ui.label("🔍");
                ui.add(egui::TextEdit::singleline(&mut self.filter_text).hint_text("Search process...").desired_width(ui.available_width()));
            });

            ui.add_space(5.0);

            // List
            // Calculate height for list (available - footer spacing if any)
            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .show(ui, |ui| {
                let filtered: Vec<_> = self.cached_processes.iter()
                    .filter(|(_, name, _)| {
                        self.filter_text.is_empty() || name.to_lowercase().contains(&self.filter_text.to_lowercase())
                    })
                    .collect();

                if filtered.is_empty() {
                    ui.vertical_centered(|ui| {
                        ui.add_space(20.0);
                        ui.label(egui::RichText::new("No processes found").weak());
                    });
                } else {
                     egui::Grid::new("process_grid")
                        .striped(true)
                        .spacing([10.0, 4.0])
                        .min_col_width(ui.available_width() / 4.0) 
                        .show(ui, |ui| {
                            // Header
                            ui.strong("PID");
                            ui.strong("Name");
                            ui.strong("CPU");
                            ui.end_row();

                            for (pid, name, cpu) in filtered {
                                let is_selected = Some(*pid) == self.selected_pid;
                                let pid_text = format!("{}", pid);
                                let cpu_text = format!("{:.1}%", cpu);
                                
                                // PID Column
                                if ui.selectable_label(is_selected, &pid_text).clicked() {
                                    self.selected_pid = Some(*pid);
                                    if !self.global_mode {
                                        self.limiter.set_target(*pid);
                                    }
                                }
                                
                                // Name Column
                                if ui.selectable_label(is_selected, name).clicked() {
                                    self.selected_pid = Some(*pid);
                                    if !self.global_mode {
                                        self.limiter.set_target(*pid);
                                    }
                                }

                                // CPU Column (Colorize high usage)
                                let cpu_color = if *cpu > 50.0 {
                                    egui::Color32::RED
                                } else if *cpu > 20.0 {
                                    egui::Color32::from_rgb(255, 165, 0) // Orange
                                } else {
                                    ui.visuals().text_color()
                                };
                                
                                if ui.selectable_label(is_selected, egui::RichText::new(&cpu_text).color(cpu_color)).clicked() {
                                    self.selected_pid = Some(*pid);
                                    if !self.global_mode {
                                        self.limiter.set_target(*pid);
                                    }
                                }

                                ui.end_row();
                            }
                        });
                }
            });
        });

        // Request repaint periodically for stats update
        ctx.request_repaint_after(Duration::from_millis(500));
    }
}

// Configures the overall look and feel
fn configure_visuals(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    
    visuals.window_corner_radius = egui::CornerRadius::same(8);
    visuals.widgets.noninteractive.corner_radius = egui::CornerRadius::same(4);
    visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(4);
    visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(4);
    visuals.widgets.active.corner_radius = egui::CornerRadius::same(4);
    visuals.widgets.open.corner_radius = egui::CornerRadius::same(4);
    
    visuals.selection.bg_fill = egui::Color32::from_rgb(0, 100, 200);
    visuals.selection.stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(100, 180, 255));
    
    ctx.set_visuals(visuals);
}
