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
        _cc: &eframe::CreationContext,
        limiter: Arc<Limiter>,
        tray_icon: Option<TrayIcon>,
        quit_menu_id: MenuId,
    ) -> Self {
        // Customize look if needed in cc.egui_ctx
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
                // Ensure window is visible in case user expects confirmation flash.
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

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("CPU Limiter");

            ui.add_space(10.0);

            // Controls
            ui.horizontal(|ui| {
                ui.label("Limit %:");
                if ui
                    .add(egui::Slider::new(&mut self.limit_value, 1..=99).text("%"))
                    .changed()
                {
                    self.limiter.set_limit(self.limit_value);
                }
            });

            ui.horizontal(|ui| {
                if ui
                    .checkbox(&mut self.global_mode, "Global Auto-Limit Mode")
                    .changed()
                {
                    if self.global_mode {
                        self.limiter.set_global(self.limit_value);
                    } else {
                        // Re-set target if we go back to targeted mode
                        if let Some(pid) = self.selected_pid {
                            self.limiter.set_target(pid);
                        }
                    }
                }
            });

            let btn_text = if self.is_active {
                "Stop Limiting"
            } else {
                "Start Limiting"
            };
            if ui.button(btn_text).clicked() {
                self.is_active = !self.is_active;
                self.limiter.toggle(self.is_active);
            }

            if self.is_active {
                ui.colored_label(egui::Color32::RED, "Limiter ACTIVE");
            } else {
                ui.label("Limiter Inactive");
            }

            ui.separator();

            // Process List
            ui.horizontal(|ui| {
                ui.label("Search:");
                ui.text_edit_singleline(&mut self.filter_text);
            });

            ui.separator();

            egui::ScrollArea::vertical().show(ui, |ui| {
                // Determine which subset to show
                let filtered: Vec<_> = self
                    .cached_processes
                    .iter()
                    .filter(|(_, name, _)| {
                        self.filter_text.is_empty()
                            || name
                                .to_lowercase()
                                .contains(&self.filter_text.to_lowercase())
                    })
                    .collect();

                for (pid, name, cpu) in filtered {
                    let is_selected = Some(*pid) == self.selected_pid;
                    let label = format!("[{}] {} - {:.1}% CPU", pid, name, cpu);

                    if ui.selectable_label(is_selected, label).clicked() {
                        self.selected_pid = Some(*pid);
                        if !self.global_mode {
                            self.limiter.set_target(*pid);
                        }
                    }
                }
            });
        });

        // Request repaint periodically for stats update
        ctx.request_repaint_after(Duration::from_millis(500));
    }
}
