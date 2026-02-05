use crate::limiter::Limiter;
use eframe::egui;
use std::sync::Arc;
use std::time::{Duration, Instant};
use std::collections::VecDeque;
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
    cached_processes: Vec<(i32, String, f32)>,
    selected_pid: Option<i32>,
    limit_value: u32,
    is_active: bool,
    global_mode: bool,
    pub _tray_icon: Option<TrayIcon>,
    quit_menu_id: MenuId,
    allow_close: bool,
    
    // Visual & State Extras
    cpu_history: VecDeque<f64>,
    total_cpu_usage: f32,
}

impl CpuLimiterApp {
    pub fn new(
        cc: &eframe::CreationContext,
        limiter: Arc<Limiter>,
        tray_icon: Option<TrayIcon>,
        quit_menu_id: MenuId,
    ) -> Self {
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
            cpu_history: VecDeque::with_capacity(300),
            total_cpu_usage: 0.0,
        }
    }

    fn refresh_processes(&mut self) {
        self.system.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
        self.system.refresh_cpu_all();
        
        self.total_cpu_usage = self.system.global_cpu_usage();
        
        if self.cpu_history.len() >= 300 {
            self.cpu_history.pop_front();
        }
        self.cpu_history.push_back(self.total_cpu_usage as f64);

        let mut procs: Vec<_> = self.system.processes().iter()
            .map(|(pid, proc)| {
                (
                    pid.as_u32() as i32,
                    proc.name().to_string_lossy().to_string(),
                    proc.cpu_usage(),
                )
            })
            .collect();

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
                TrayIconEvent::Click { button, button_state, .. } => {
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
        if self.last_update.elapsed() > Duration::from_millis(1000) {
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

        // --- DESIGN CONFIG ---
        let accent_color = egui::Color32::from_rgb(0, 212, 255);
        let bg_color = egui::Color32::from_rgb(20, 21, 30);
        
        let custom_frame = egui::Frame::NONE
            .fill(bg_color);

        egui::CentralPanel::default().frame(custom_frame).show(ctx, |ui| {
            ui.vertical(|ui| {
                
                // === HEADER ===
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.add_space(4.0);
                    ui.label(egui::RichText::new("⚡").size(28.0).color(accent_color));
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new("CPU LIMITER").size(20.0).strong().color(egui::Color32::WHITE));
                        ui.label(egui::RichText::new("SYSTEM CONTROL").size(10.0).color(egui::Color32::from_white_alpha(150)).extra_letter_spacing(2.0));
                    });
                    
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let status_text = if self.is_active { "ACTIVE" } else { "STANDBY" };
                        let status_color = if self.is_active { egui::Color32::GREEN } else { egui::Color32::from_white_alpha(100) };
                        
                        egui::Frame::group(ui.style())
                            .fill(status_color.gamma_multiply(0.1))
                            .stroke(egui::Stroke::new(1.0, status_color))
                            .corner_radius(16)
                            .inner_margin(egui::Margin::symmetric(12, 4))
                            .show(ui, |ui| {
                                ui.label(egui::RichText::new(status_text).size(12.0).strong().color(status_color));
                            });
                    });
                });
                ui.add_space(15.0);

                // === GRAPH ===
                egui::Frame::canvas(ui.style())
                    .fill(egui::Color32::from_rgb(28, 29, 40))
                    .corner_radius(12)
                    .inner_margin(0.0)
                    .show(ui, |ui| {
                        let response = ui.allocate_response(
                            egui::vec2(ui.available_width(), 80.0), 
                            egui::Sense::hover()
                        );
                        let painter = ui.painter_at(response.rect);
                        
                        let rect = response.rect;
                        
                        if self.cpu_history.len() > 1 {
                             let history_max = 300.0;
                             let points: Vec<egui::Pos2> = self.cpu_history.iter().rev().enumerate().map(|(i, &val)| {
                                // Right to left
                                let x = egui::remap(i as f64, 0.0..=history_max, rect.right() as f64..=rect.left() as f64);
                                let y = egui::remap(val, 0.0..=100.0, rect.bottom() as f64..=rect.top() as f64);
                                egui::Pos2::new(x as f32, y as f32)
                             }).collect();

                             let stroke = egui::Stroke::new(2.0, accent_color);
                             painter.add(egui::Shape::line(points.clone(), stroke));
                             
                             // Gradient fill removed for simplicity to ensure stability
                             // just the line is drawn

                        }
                    });
                
                ui.add_space(15.0);

                // === CONTROLS ===
                egui::Frame::group(ui.style())
                    .fill(egui::Color32::from_rgb(30, 32, 45))
                    .stroke(egui::Stroke::NONE)
                    .corner_radius(12)
                    .inner_margin(12.0)
                    .show(ui, |ui| {
                        ui.label(egui::RichText::new("CONFIGURATION").size(12.0).strong().color(egui::Color32::from_white_alpha(120)));
                        ui.add_space(8.0);
                        
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("Limit Target").color(egui::Color32::WHITE));
                            ui.add_space(10.0);
                            let slider = egui::Slider::new(&mut self.limit_value, 1..=99)
                                .text("CPU %")
                                .show_value(true)
                                .smart_aim(true)
                                .trailing_fill(true);
                            if ui.add(slider).changed() {
                                self.limiter.set_limit(self.limit_value);
                            }
                        });

                        ui.add_space(8.0);
                        
                        let checkbox = egui::Checkbox::new(&mut self.global_mode, egui::RichText::new("Global Auto-Limit").color(egui::Color32::LIGHT_GRAY));
                        if ui.add(checkbox).changed() {
                            if self.global_mode {
                                self.limiter.set_global(self.limit_value);
                            } else if let Some(pid) = self.selected_pid {
                                self.limiter.set_target(pid);
                            }
                        }

                        ui.add_space(12.0);

                        let btn_color = if self.is_active { egui::Color32::from_rgb(255, 60, 60) } else { accent_color };
                        let btn_text = if self.is_active { "STOP LIMITER" } else { "ACTIVATE LIMITER" };
                        
                        let btn = egui::Button::new(egui::RichText::new(btn_text).heading().strong().color(egui::Color32::from_rgb(20, 20, 20)))
                            .min_size(egui::vec2(ui.available_width(), 40.0))
                            .fill(btn_color)
                            .corner_radius(8);

                        if ui.add(btn).clicked() {
                            self.is_active = !self.is_active;
                            self.limiter.toggle(self.is_active);
                        }
                    });

                ui.add_space(15.0);

                // === PROCESSES ===
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("PROCESSES").size(12.0).strong().color(egui::Color32::from_white_alpha(120)));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                         ui.add(egui::TextEdit::singleline(&mut self.filter_text)
                            .hint_text("Search...")
                            .min_size(egui::vec2(120.0, 10.0))
                        );
                    });
                });
                ui.add_space(5.0);

                egui::Frame::group(ui.style())
                    .fill(egui::Color32::from_rgb(25, 26, 35))
                    .stroke(egui::Stroke::new(1.0, egui::Color32::from_white_alpha(10)))
                    .corner_radius(8)
                    .inner_margin(0.0)
                    .show(ui, |ui| {
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
                                        ui.label(egui::RichText::new("No matching processes").weak());
                                        ui.add_space(20.0);
                                    });
                                } else {
                                    egui::Grid::new("process_grid_modern")
                                        .striped(true)
                                        .num_columns(3)
                                        .spacing([10.0, 8.0])
                                        .min_col_width(ui.available_width() / 4.0)
                                        .show(ui, |ui| {
                                            ui.style_mut().visuals.widgets.noninteractive.bg_fill = egui::Color32::from_white_alpha(5);
                                            ui.label(egui::RichText::new("PID").strong().color(egui::Color32::GRAY));
                                            ui.label(egui::RichText::new("NAME").strong().color(egui::Color32::GRAY));
                                            ui.label(egui::RichText::new("CPU").strong().color(egui::Color32::GRAY));
                                            ui.end_row();

                                            for (pid, name, cpu) in filtered {
                                                let is_selected = Some(*pid) == self.selected_pid;
                                                let text_color = if is_selected { accent_color } else { egui::Color32::LIGHT_GRAY };
                                                
                                                if ui.add(egui::Label::new(
                                                    egui::RichText::new(format!("{}", pid)).color(text_color).monospace()
                                                ).sense(egui::Sense::click())).clicked() 
                                                {
                                                    self.selected_pid = Some(*pid);
                                                    if !self.global_mode { self.limiter.set_target(*pid); }
                                                }
                                                
                                                let display_name = if name.len() > 20 { format!("{}...", &name[0..18]) } else { name.clone() };
                                                if ui.add(egui::Label::new(
                                                    egui::RichText::new(display_name).color(text_color)
                                                ).sense(egui::Sense::click())).clicked() 
                                                {
                                                    self.selected_pid = Some(*pid);
                                                    if !self.global_mode { self.limiter.set_target(*pid); }
                                                }

                                                let cpu_intensity = (*cpu / 100.0).clamp(0.0, 1.0);
                                                let cpu_color = egui::Color32::from_rgb(
                                                    (20.0 + (cpu_intensity * 235.0)) as u8,
                                                    (200.0 - (cpu_intensity * 100.0)) as u8, 
                                                    100
                                                );
                                                
                                                if ui.add(egui::Label::new(
                                                    egui::RichText::new(format!("{:.1}%", cpu)).color(cpu_color).strong()
                                                ).sense(egui::Sense::click())).clicked()
                                                {
                                                    self.selected_pid = Some(*pid);
                                                    if !self.global_mode { self.limiter.set_target(*pid); }
                                                }
                                                
                                                ui.end_row();
                                            }
                                        });
                                }
                            });
                    });
            });
        });
        
        ctx.request_repaint_after(Duration::from_millis(100)); // Animated feel
    }
}

fn configure_visuals(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.window_fill = egui::Color32::from_rgb(20, 21, 30);
    visuals.panel_fill = egui::Color32::from_rgb(20, 21, 30);
    
    let accent = egui::Color32::from_rgb(0, 212, 255);
    
    visuals.widgets.noninteractive.bg_fill = egui::Color32::from_rgb(28, 29, 40);
    visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, egui::Color32::from_white_alpha(180));
    
    visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(40, 42, 55);
    visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, egui::Color32::from_white_alpha(200));
    
    visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(60, 64, 80);
    visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.5, egui::Color32::WHITE);
    
    visuals.widgets.active.bg_fill = accent;
    visuals.widgets.active.fg_stroke = egui::Stroke::new(2.0, egui::Color32::WHITE);
    
    visuals.selection.bg_fill = accent.gamma_multiply(0.3);
    visuals.selection.stroke = egui::Stroke::new(1.0, accent);

    visuals.window_corner_radius = egui::CornerRadius::same(12);
    
    let fonts = egui::FontDefinitions::default();
    ctx.set_fonts(fonts);
    ctx.set_visuals(visuals);
    
    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    style.spacing.window_margin = egui::Margin::same(16);
    style.spacing.button_padding = egui::vec2(12.0, 8.0);
    ctx.set_style(style);
}
