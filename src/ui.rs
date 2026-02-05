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
    memory_used: u64,
    memory_total: u64,
    uptime_seconds: u64,
    start_time: Instant,
}

impl CpuLimiterApp {
    pub fn new(
        cc: &eframe::CreationContext,
        limiter: Arc<Limiter>,
        tray_icon: Option<TrayIcon>,
        quit_menu_id: MenuId,
    ) -> Self {
        configure_visuals(&cc.egui_ctx);

        let mut system = System::new_all();
        system.refresh_memory();
        
        Self {
            limiter,
            memory_used: system.used_memory(),
            memory_total: system.total_memory(),
            system,
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
            uptime_seconds: 0,
            start_time: Instant::now(),
        }
    }

    fn refresh_processes(&mut self) {
        self.system.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
        self.system.refresh_cpu_all();
        self.system.refresh_memory();
        
        self.total_cpu_usage = self.system.global_cpu_usage();
        self.memory_used = self.system.used_memory();
        self.memory_total = self.system.total_memory();
        self.uptime_seconds = self.start_time.elapsed().as_secs();
        
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
    
    fn format_bytes(bytes: u64) -> String {
        let gb = bytes as f64 / 1_073_741_824.0;
        format!("{:.1} GB", gb)
    }
    
    fn format_uptime(seconds: u64) -> String {
        let hours = seconds / 3600;
        let minutes = (seconds % 3600) / 60;
        let secs = seconds % 60;
        format!("{:02}:{:02}:{:02}", hours, minutes, secs)
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
        let accent_green = egui::Color32::from_rgb(0, 230, 118);
        let accent_purple = egui::Color32::from_rgb(167, 139, 250);
        let accent_orange = egui::Color32::from_rgb(251, 146, 60);
        let bg_color = egui::Color32::from_rgb(20, 21, 30);
        let card_color = egui::Color32::from_rgb(30, 32, 45);
        
        let custom_frame = egui::Frame::NONE
            .fill(bg_color)
            .inner_margin(egui::Margin::symmetric(24, 16));

        egui::CentralPanel::default().frame(custom_frame).show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.set_width(ui.available_width());
                
                // === HEADER ===
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.add_space(4.0);
                    // Pulsating icon effect (simple color change based on time)
                    let pulse = ((ctx.input(|i| i.time) * 2.0).sin() * 0.3 + 0.7) as f32;
                    let icon_color = egui::Color32::from_rgb(
                        (accent_color.r() as f32 * pulse) as u8,
                        (accent_color.g() as f32 * pulse) as u8,
                        (accent_color.b() as f32 * pulse) as u8,
                    );
                    ui.label(egui::RichText::new("⚡").size(32.0).color(icon_color));
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new("CPU LIMITER").size(22.0).strong().color(egui::Color32::WHITE));
                        ui.label(egui::RichText::new("SYSTEM CONTROL").size(10.0).color(egui::Color32::from_white_alpha(150)).extra_letter_spacing(2.0));
                    });
                    
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let status_text = if self.is_active { "● ACTIVE" } else { "○ STANDBY" };
                        let status_color = if self.is_active { accent_green } else { egui::Color32::from_white_alpha(100) };
                        
                        egui::Frame::group(ui.style())
                            .fill(status_color.gamma_multiply(0.15))
                            .stroke(egui::Stroke::new(1.5, status_color))
                            .corner_radius(20)
                            .inner_margin(egui::Margin::symmetric(14, 6))
                            .show(ui, |ui| {
                                ui.label(egui::RichText::new(status_text).size(12.0).strong().color(status_color));
                            });
                    });
                });
                ui.add_space(16.0);
                
                // === STATS CARDS ROW ===
                ui.horizontal(|ui| {
                    let card_width = (ui.available_width() - 24.0) / 4.0;
                    
                    // CPU Card
                    Self::stat_card(ui, card_width, card_color, accent_color, "CPU", &format!("{:.1}%", self.total_cpu_usage), "📊");
                    ui.add_space(8.0);
                    
                    // Memory Card
                    let mem_percent = if self.memory_total > 0 { 
                        (self.memory_used as f64 / self.memory_total as f64 * 100.0) as f32 
                    } else { 0.0 };
                    Self::stat_card(ui, card_width, card_color, accent_purple, "RAM", &format!("{:.1}%", mem_percent), "💾");
                    ui.add_space(8.0);
                    
                    // Processes Card
                    Self::stat_card(ui, card_width, card_color, accent_orange, "PROCS", &format!("{}", self.cached_processes.len()), "🔢");
                    ui.add_space(8.0);
                    
                    // Uptime Card
                    Self::stat_card(ui, card_width, card_color, accent_green, "UPTIME", &Self::format_uptime(self.uptime_seconds), "⏱");
                });
                ui.add_space(16.0);

                // === CPU GRAPH ===
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("📈 CPU HISTORY").size(11.0).strong().color(egui::Color32::from_white_alpha(180)));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(egui::RichText::new(&format!("Current: {:.1}%", self.total_cpu_usage)).size(11.0).color(accent_color));
                    });
                });
                ui.add_space(4.0);
                
                egui::Frame::canvas(ui.style())
                    .fill(egui::Color32::from_rgb(28, 29, 40))
                    .stroke(egui::Stroke::new(1.0, egui::Color32::from_white_alpha(20)))
                    .corner_radius(12)
                    .inner_margin(8.0)
                    .show(ui, |ui| {
                        let response = ui.allocate_response(
                            egui::vec2(ui.available_width(), 100.0), 
                            egui::Sense::hover()
                        );
                        let painter = ui.painter_at(response.rect);
                        let rect = response.rect;
                        
                        // Draw grid lines
                        let grid_color = egui::Color32::from_white_alpha(15);
                        for i in 1..4 {
                            let y = rect.top() + (rect.height() / 4.0) * i as f32;
                            painter.line_segment(
                                [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
                                egui::Stroke::new(1.0, grid_color)
                            );
                        }
                        
                        // Draw CPU line
                        if self.cpu_history.len() > 1 {
                            let history_max = 300.0;
                            let points: Vec<egui::Pos2> = self.cpu_history.iter().rev().enumerate().map(|(i, &val)| {
                                let x = egui::remap(i as f64, 0.0..=history_max, rect.right() as f64..=rect.left() as f64);
                                let y = egui::remap(val, 0.0..=100.0, (rect.bottom() - 4.0) as f64..=(rect.top() + 4.0) as f64);
                                egui::Pos2::new(x as f32, y as f32)
                            }).collect();
                            
                            // Glow effect (draw thicker, semi-transparent line behind)
                            let glow_stroke = egui::Stroke::new(6.0, accent_color.gamma_multiply(0.2));
                            painter.add(egui::Shape::line(points.clone(), glow_stroke));
                            
                            let stroke = egui::Stroke::new(2.5, accent_color);
                            painter.add(egui::Shape::line(points, stroke));
                        }
                        
                        // Y-axis labels
                        painter.text(egui::pos2(rect.left() + 4.0, rect.top() + 8.0), egui::Align2::LEFT_TOP, "100%", egui::FontId::proportional(9.0), egui::Color32::from_white_alpha(80));
                        painter.text(egui::pos2(rect.left() + 4.0, rect.bottom() - 8.0), egui::Align2::LEFT_BOTTOM, "0%", egui::FontId::proportional(9.0), egui::Color32::from_white_alpha(80));
                    });
                
                ui.add_space(16.0);
                
                // === MEMORY PROGRESS BAR ===
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("💾 MEMORY USAGE").size(11.0).strong().color(egui::Color32::from_white_alpha(180)));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(egui::RichText::new(&format!("{} / {}", Self::format_bytes(self.memory_used), Self::format_bytes(self.memory_total))).size(11.0).color(accent_purple));
                    });
                });
                ui.add_space(4.0);
                
                let mem_fraction = if self.memory_total > 0 { self.memory_used as f32 / self.memory_total as f32 } else { 0.0 };
                let progress_bar = egui::ProgressBar::new(mem_fraction)
                    .fill(accent_purple)
                    .corner_radius(6);
                ui.add(progress_bar);

                ui.add_space(16.0);
                
                // === SEPARATOR ===
                ui.add(egui::Separator::default().spacing(8.0));
                ui.add_space(8.0);

                // === CONTROLS ===
                egui::Frame::group(ui.style())
                    .fill(card_color)
                    .stroke(egui::Stroke::new(1.0, egui::Color32::from_white_alpha(10)))
                    .corner_radius(12)
                    .inner_margin(16.0)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("⚙").size(16.0).color(accent_color));
                            ui.label(egui::RichText::new("CONFIGURATION").size(12.0).strong().color(egui::Color32::from_white_alpha(180)));
                        });
                        ui.add_space(12.0);
                        
                        // Limit Slider with label
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("Limit Target").color(egui::Color32::WHITE));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.label(egui::RichText::new(&format!("{}%", self.limit_value)).size(14.0).strong().color(accent_color));
                            });
                        });
                        ui.add_space(4.0);
                        let slider = egui::Slider::new(&mut self.limit_value, 1..=99)
                            .show_value(false)
                            .trailing_fill(true);
                        if ui.add(slider).changed() {
                            self.limiter.set_limit(self.limit_value);
                        }

                        ui.add_space(12.0);
                        
                        // Checkbox with icon
                        ui.horizontal(|ui| {
                            let checkbox = egui::Checkbox::new(&mut self.global_mode, "");
                            ui.add(checkbox);
                            ui.label(egui::RichText::new("🌐 Global Auto-Limit Mode").color(egui::Color32::LIGHT_GRAY));
                        }).response.on_hover_text("Automatically limit the highest CPU consuming process");
                        
                        if self.global_mode {
                            self.limiter.set_global(self.limit_value);
                        } else if let Some(pid) = self.selected_pid {
                            self.limiter.set_target(pid);
                        }

                        ui.add_space(16.0);

                        // Main Action Button
                        let btn_color = if self.is_active { egui::Color32::from_rgb(239, 68, 68) } else { accent_green };
                        let btn_text = if self.is_active { "⏹ STOP LIMITER" } else { "▶ ACTIVATE LIMITER" };
                        
                        let btn = egui::Button::new(egui::RichText::new(btn_text).size(15.0).strong().color(egui::Color32::from_rgb(20, 20, 20)))
                            .min_size(egui::vec2(ui.available_width(), 44.0))
                            .fill(btn_color)
                            .corner_radius(10);

                        if ui.add(btn).clicked() {
                            self.is_active = !self.is_active;
                            self.limiter.toggle(self.is_active);
                        }
                    });

                ui.add_space(16.0);

                // === PROCESSES ===
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("📋 PROCESSES").size(11.0).strong().color(egui::Color32::from_white_alpha(180)));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add(egui::TextEdit::singleline(&mut self.filter_text)
                            .hint_text("🔍 Search...")
                            .min_size(egui::vec2(140.0, 10.0))
                        );
                    });
                });
                ui.add_space(8.0);

                egui::Frame::group(ui.style())
                    .fill(egui::Color32::from_rgb(25, 26, 35))
                    .stroke(egui::Stroke::new(1.0, egui::Color32::from_white_alpha(10)))
                    .corner_radius(10)
                    .inner_margin(8.0)
                    .show(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .max_height(350.0)
                            .auto_shrink([false; 2])
                            .show(ui, |ui| {
                                let filtered: Vec<_> = self.cached_processes.iter()
                                    .filter(|(_, name, _)| {
                                        self.filter_text.is_empty() || name.to_lowercase().contains(&self.filter_text.to_lowercase())
                                    })
                                    .take(100) // Limit for performance
                                    .collect();

                                if filtered.is_empty() {
                                    ui.vertical_centered(|ui| {
                                        ui.add_space(30.0);
                                        ui.label(egui::RichText::new("No matching processes").weak().size(14.0));
                                        ui.add_space(30.0);
                                    });
                                } else {
                                    egui::Grid::new("process_grid_modern")
                                        .striped(true)
                                        .num_columns(3)
                                        .spacing([20.0, 10.0])
                                        .min_col_width(ui.available_width() / 4.0)
                                        .show(ui, |ui| {
                                            // Header
                                            ui.label(egui::RichText::new("PID").size(10.0).strong().color(egui::Color32::GRAY));
                                            ui.label(egui::RichText::new("NAME").size(10.0).strong().color(egui::Color32::GRAY));
                                            ui.label(egui::RichText::new("CPU").size(10.0).strong().color(egui::Color32::GRAY));
                                            ui.end_row();

                                            for (pid, name, cpu) in filtered {
                                                let is_selected = Some(*pid) == self.selected_pid;
                                                let text_color = if is_selected { accent_color } else { egui::Color32::LIGHT_GRAY };
                                                let row_bg = if is_selected { accent_color.gamma_multiply(0.1) } else { egui::Color32::TRANSPARENT };
                                                
                                                // Add subtle background for selected
                                                ui.painter().rect_filled(
                                                    ui.cursor(),
                                                    0.0,
                                                    row_bg
                                                );
                                                
                                                if ui.add(egui::Label::new(
                                                    egui::RichText::new(format!("{}", pid)).color(text_color).monospace().size(11.0)
                                                ).sense(egui::Sense::click())).clicked() 
                                                {
                                                    self.selected_pid = Some(*pid);
                                                    if !self.global_mode { self.limiter.set_target(*pid); }
                                                }
                                                
                                                let display_name = if name.len() > 22 { format!("{}...", &name[0..20]) } else { name.clone() };
                                                if ui.add(egui::Label::new(
                                                    egui::RichText::new(&display_name).color(text_color).size(11.0)
                                                ).sense(egui::Sense::click())).on_hover_text(name).clicked() 
                                                {
                                                    self.selected_pid = Some(*pid);
                                                    if !self.global_mode { self.limiter.set_target(*pid); }
                                                }

                                                // CPU with mini progress bar visual
                                                let cpu_intensity = (*cpu / 100.0).clamp(0.0, 1.0);
                                                let cpu_color = if *cpu > 80.0 {
                                                    egui::Color32::from_rgb(239, 68, 68) // Red
                                                } else if *cpu > 50.0 {
                                                    accent_orange
                                                } else if *cpu > 20.0 {
                                                    egui::Color32::from_rgb(250, 204, 21) // Yellow
                                                } else {
                                                    accent_green
                                                };
                                                
                                                ui.horizontal(|ui| {
                                                    // Mini bar
                                                    let bar_width = 40.0;
                                                    let (rect, _) = ui.allocate_exact_size(egui::vec2(bar_width, 8.0), egui::Sense::hover());
                                                    let painter = ui.painter();
                                                    painter.rect_filled(rect, 3.0, egui::Color32::from_white_alpha(20));
                                                    let filled_rect = egui::Rect::from_min_size(rect.min, egui::vec2(bar_width * cpu_intensity, 8.0));
                                                    painter.rect_filled(filled_rect, 3.0, cpu_color);
                                                    
                                                    ui.add_space(4.0);
                                                    if ui.add(egui::Label::new(
                                                        egui::RichText::new(format!("{:.1}%", cpu)).color(cpu_color).strong().size(11.0)
                                                    ).sense(egui::Sense::click())).clicked()
                                                    {
                                                        self.selected_pid = Some(*pid);
                                                        if !self.global_mode { self.limiter.set_target(*pid); }
                                                    }
                                                });
                                                
                                                ui.end_row();
                                            }
                                        });
                                }
                            });
                    });

                ui.add_space(16.0);
                
                // === FOOTER ===
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("CPU Limiter v0.1.0").size(10.0).color(egui::Color32::from_white_alpha(60)));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(egui::RichText::new("Made with ❤️ in Rust").size(10.0).color(egui::Color32::from_white_alpha(60)));
                    });
                });

            });
        });
        
        ctx.request_repaint_after(Duration::from_millis(100));
    }
}

impl CpuLimiterApp {
    fn stat_card(ui: &mut egui::Ui, width: f32, bg_color: egui::Color32, accent: egui::Color32, label: &str, value: &str, icon: &str) {
        egui::Frame::group(ui.style())
            .fill(bg_color)
            .stroke(egui::Stroke::new(1.0, accent.gamma_multiply(0.3)))
            .corner_radius(10)
            .inner_margin(10.0)
            .show(ui, |ui| {
                ui.set_width(width);
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(icon).size(14.0));
                        ui.label(egui::RichText::new(label).size(10.0).color(egui::Color32::from_white_alpha(150)));
                    });
                    ui.add_space(2.0);
                    ui.label(egui::RichText::new(value).size(16.0).strong().color(accent));
                });
            });
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
