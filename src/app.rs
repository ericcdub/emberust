use crate::api::EphEmberApi;
use crate::models::*;
use crate::mqtt;
use eframe::egui;
use std::collections::HashMap;

// --- Colors ---

const COLOR_HEATING: egui::Color32 = egui::Color32::from_rgb(255, 140, 50);
const COLOR_BOILER: egui::Color32 = egui::Color32::from_rgb(255, 70, 30);
const COLOR_BOOST: egui::Color32 = egui::Color32::from_rgb(255, 200, 50);
const COLOR_ONLINE: egui::Color32 = egui::Color32::from_rgb(80, 200, 80);
const COLOR_OFFLINE: egui::Color32 = egui::Color32::from_rgb(200, 80, 80);

fn temp_color(temp: f32) -> egui::Color32 {
    let t = ((temp - 10.0) / 20.0).clamp(0.0, 1.0);
    let r = (t * 255.0) as u8;
    let b = ((1.0 - t) * 255.0) as u8;
    egui::Color32::from_rgb(r, 80, b)
}

// --- Screens ---

#[derive(Debug, PartialEq)]
enum Screen {
    Login,
    Dashboard,
}

// --- App ---

pub struct EphEmberApp {
    screen: Screen,

    // Login
    username: String,
    password: String,
    login_error: Option<String>,
    loading: bool,

    // Data
    zones: Vec<Zone>,
    status_message: Option<String>,

    // UI state for zone controls
    pending_targets: HashMap<String, f32>,
    boost_hours: HashMap<String, u32>,

    // Backend communication
    cmd_tx: tokio::sync::mpsc::UnboundedSender<Command>,
    update_rx: std::sync::mpsc::Receiver<Update>,
}

impl EphEmberApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // Larger default font sizes for touch-friendliness (Android)
        let mut style = (*cc.egui_ctx.style()).clone();
        style.text_styles.insert(
            egui::TextStyle::Body,
            egui::FontId::proportional(15.0),
        );
        style.text_styles.insert(
            egui::TextStyle::Button,
            egui::FontId::proportional(15.0),
        );
        style.text_styles.insert(
            egui::TextStyle::Heading,
            egui::FontId::proportional(22.0),
        );
        style.spacing.button_padding = egui::vec2(10.0, 6.0);
        cc.egui_ctx.set_style(style);

        let (cmd_tx, cmd_rx) = tokio::sync::mpsc::unbounded_channel();
        let (update_tx, update_rx) = std::sync::mpsc::channel();
        let ctx = cc.egui_ctx.clone();

        // Spawn the async backend on a background thread
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");
            rt.block_on(backend_loop(cmd_rx, update_tx, ctx));
        });

        // Load saved credentials
        let (username, password) = cc.storage
            .map(|s| {
                let u = s.get_string("username").unwrap_or_default();
                let p = s.get_string("password").unwrap_or_default();
                log::info!("Loaded credentials: username={}, has_password={}", 
                    if u.is_empty() { "(empty)" } else { "(set)" },
                    !p.is_empty());
                (u, p)
            })
            .unwrap_or_else(|| {
                log::info!("No storage available");
                (String::new(), String::new())
            });

        let has_saved_creds = !username.is_empty() && !password.is_empty();

        let app = Self {
            screen: Screen::Login,
            username,
            password,
            login_error: None,
            loading: has_saved_creds,
            zones: Vec::new(),
            status_message: None,
            pending_targets: HashMap::new(),
            boost_hours: HashMap::new(),
            cmd_tx,
            update_rx,
        };

        // Auto-login if we have saved credentials
        if has_saved_creds {
            app.send(Command::Login {
                username: app.username.clone(),
                password: app.password.clone(),
            });
        }

        app
    }

    fn process_updates(&mut self) {
        while let Ok(update) = self.update_rx.try_recv() {
            match update {
                Update::LoggedIn => {
                    self.screen = Screen::Dashboard;
                    self.loading = false;
                    self.login_error = None;
                }
                Update::LoggedOut => {
                    self.screen = Screen::Login;
                    self.zones.clear();
                    self.username.clear();
                    self.password.clear();
                    self.loading = false;
                }
                Update::LoginFailed(msg) => {
                    self.loading = false;
                    self.login_error = Some(msg);
                }
                Update::ZonesUpdated(zones) => {
                    self.zones = zones;
                    self.pending_targets.clear();
                    self.loading = false;
                }
                Update::Error(msg) => {
                    self.status_message = Some(msg);
                    self.loading = false;
                }
                Update::CommandSent(msg) => {
                    self.status_message = Some(msg);
                }
            }
        }
    }

    fn send(&self, cmd: Command) {
        self.cmd_tx.send(cmd).ok();
    }

    // --- Login screen ---

    fn show_login(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(ui.available_height() / 4.0);

                ui.heading("Emberust");
                ui.add_space(24.0);

                ui.label("Email");
                let email_resp =
                    ui.add_sized([300.0, 28.0], egui::TextEdit::singleline(&mut self.username));

                ui.add_space(8.0);
                ui.label("Password");
                let pass_resp = ui.add_sized(
                    [300.0, 28.0],
                    egui::TextEdit::singleline(&mut self.password).password(true),
                );

                ui.add_space(16.0);

                let enter_pressed = (email_resp.lost_focus() || pass_resp.lost_focus())
                    && ui.input(|i| i.key_pressed(egui::Key::Enter));

                let btn = ui.add_enabled(!self.loading, egui::Button::new("Sign In"));

                if (btn.clicked() || enter_pressed) && !self.loading {
                    self.loading = true;
                    self.login_error = None;
                    self.send(Command::Login {
                        username: self.username.clone(),
                        password: self.password.clone(),
                    });
                }

                if self.loading {
                    ui.add_space(8.0);
                    ui.spinner();
                }

                if let Some(ref err) = self.login_error {
                    ui.add_space(8.0);
                    ui.colored_label(egui::Color32::RED, err);
                }
            });
        });
    }

    // --- Dashboard ---

    fn show_dashboard(&mut self, ctx: &egui::Context) {
        // Top bar
        egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Emberust");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Log Out").clicked() {
                        self.send(Command::Logout);
                    }
                    ui.separator();
                    if ui.add_enabled(!self.loading, egui::Button::new("Refresh")).clicked() {
                        self.loading = true;
                        self.send(Command::RefreshZones);
                    }
                    if self.loading {
                        ui.spinner();
                    }
                    if let Some(ref msg) = self.status_message {
                        ui.label(msg);
                    }
                });
            });
        });

        // Zone grid
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                if self.zones.is_empty() {
                    ui.vertical_centered(|ui| {
                        ui.add_space(40.0);
                        ui.label("No zones found.");
                    });
                    return;
                }

                let card_width = 300.0_f32;
                let cols = (ui.available_width() / card_width).floor().max(1.0) as usize;

                // Collect zone data we need for rendering
                // We need to work around borrow checker since zone_card borrows self mutably
                let zone_indices: Vec<usize> = (0..self.zones.len()).collect();

                egui::Grid::new("zone_grid")
                    .num_columns(cols)
                    .spacing([12.0, 12.0])
                    .show(ui, |ui| {
                        for (i, &idx) in zone_indices.iter().enumerate() {
                            ui.vertical(|ui| {
                                ui.set_width(card_width - 12.0);
                                self.zone_card(ui, idx);
                            });
                            if (i + 1) % cols == 0 {
                                ui.end_row();
                            }
                        }
                    });
            });
        });
    }

    // --- Zone card ---

    fn zone_card(&mut self, ui: &mut egui::Ui, zone_idx: usize) {
        let zone = &self.zones[zone_idx];
        let zone_name = zone.name.clone();

        egui::Frame::default()
            .inner_margin(12.0)
            .rounding(8.0)
            .fill(ui.visuals().faint_bg_color)
            .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
            .show(ui, |ui| {
                ui.set_min_width(260.0);

                // Zone name + online status
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(&zone_name)
                            .size(16.0)
                            .strong(),
                    );
                    let (color, text) = if self.zones[zone_idx].is_online {
                        (COLOR_ONLINE, "Online")
                    } else {
                        (COLOR_OFFLINE, "Offline")
                    };
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.colored_label(color, text);
                    });
                });

                ui.add_space(8.0);

                // Current temperature
                let zone = &self.zones[zone_idx];
                if let Some(temp) = zone.current_temperature() {
                    ui.label(
                        egui::RichText::new(format!("{temp:.1}\u{00B0}C"))
                            .size(38.0)
                            .color(temp_color(temp)),
                    );
                } else {
                    ui.label(egui::RichText::new("--.-\u{00B0}C").size(38.0));
                }

                ui.add_space(4.0);

                // Status indicators
                let zone = &self.zones[zone_idx];
                ui.horizontal(|ui| {
                    if zone.is_active() {
                        ui.colored_label(COLOR_HEATING, "\u{25CF} Heating");
                    }
                    if zone.is_boiler_on() {
                        ui.colored_label(COLOR_BOILER, "\u{25CF} Boiler");
                    }
                    if zone.is_boost_active() {
                        ui.colored_label(
                            COLOR_BOOST,
                            format!("\u{25CF} Boost {}h", zone.boost_hours().unwrap_or(0)),
                        );
                    }
                    if zone.is_advance_active() {
                        ui.label("\u{25B6} Advance");
                    }
                });

                ui.separator();

                // Target temperature with +/- controls
                let zone = &self.zones[zone_idx];
                let server_target = zone.target_temperature().unwrap_or(20.0);
                let display_target = *self
                    .pending_targets
                    .get(&zone_name)
                    .unwrap_or(&server_target);

                ui.horizontal(|ui| {
                    ui.label("Target:");

                    if ui
                        .add(egui::Button::new("\u{2212}").min_size(egui::vec2(32.0, 28.0)))
                        .clicked()
                    {
                        let new = (display_target - 0.5).max(5.0);
                        self.pending_targets.insert(zone_name.clone(), new);
                    }

                    ui.label(
                        egui::RichText::new(format!("{display_target:.1}\u{00B0}C"))
                            .strong()
                            .size(16.0),
                    );

                    if ui
                        .add(egui::Button::new("+").min_size(egui::vec2(32.0, 28.0)))
                        .clicked()
                    {
                        let new = (display_target + 0.5).min(25.5);
                        self.pending_targets.insert(zone_name.clone(), new);
                    }

                    // Show "Set" when value has been changed
                    if self.pending_targets.contains_key(&zone_name) {
                        if ui.button("Set").clicked() {
                            if let Some(temp) = self.pending_targets.remove(&zone_name) {
                                self.send(Command::SetTargetTemperature {
                                    zone_name: zone_name.clone(),
                                    temperature: temp,
                                });
                            }
                        }
                    }
                });

                ui.add_space(4.0);

                // Mode selector
                let zone = &self.zones[zone_idx];
                let current_mode = zone.mode().unwrap_or(ZoneMode::Off);
                ui.horizontal(|ui| {
                    ui.label("Mode:");
                    for mode in ZoneMode::ALL {
                        let selected = current_mode == mode;
                        if ui.selectable_label(selected, mode.label()).clicked() && !selected {
                            self.send(Command::SetMode {
                                zone_name: zone_name.clone(),
                                mode,
                            });
                        }
                    }
                });

                ui.add_space(4.0);

                // Boost controls - get values before the closure to avoid borrow issues
                let boost_active = self.zones[zone_idx].is_boost_active();
                let boost_hours_display = self.zones[zone_idx].boost_hours().unwrap_or(0);
                
                ui.horizontal(|ui| {
                    ui.label("Boost:");
                    
                    if boost_active {
                        ui.colored_label(
                            COLOR_BOOST,
                            format!("{}h", boost_hours_display),
                        );
                    } else {
                        let hours = self.boost_hours.entry(zone_name.clone()).or_insert(1);
                        ui.selectable_value(hours, 1, "1h");
                        ui.selectable_value(hours, 2, "2h");
                        ui.selectable_value(hours, 3, "3h");
                    }
                    
                    if boost_active {
                        if ui.button("Cancel").clicked() {
                            self.send(Command::DeactivateBoost {
                                zone_name: zone_name.clone(),
                            });
                        }
                    } else {
                        let h = *self.boost_hours.get(&zone_name).unwrap_or(&1);
                        if ui.button("Activate").clicked() {
                            self.send(Command::ActivateBoost {
                                zone_name: zone_name.clone(),
                                temperature: None,
                                hours: h,
                            });
                        }
                    }
                    
                    // Always show a disable button in case detection isn't working
                    if !boost_active {
                        if ui.small_button("Off").on_hover_text("Force disable boost").clicked() {
                            self.send(Command::DeactivateBoost {
                                zone_name: zone_name.clone(),
                            });
                        }
                    }
                });
                
                ui.add_space(8.0);
                
                // Debug: Show all point data - clone data to avoid borrow issues
                let zone = &self.zones[zone_idx];
                let product_id = zone.product_id.clone();
                let uid = zone.uid.clone();
                let mac = zone.mac.clone();
                let mut points: Vec<_> = zone.point_data_list.iter()
                    .map(|p| (p.point_index, p.value.clone()))
                    .collect();
                points.sort_by_key(|(idx, _)| *idx);
                
                egui::CollapsingHeader::new("Debug Info")
                    .id_salt(format!("debug_{}", zone_name))
                    .default_open(false)
                    .show(ui, |ui| {
                        ui.label(format!("productId: {}", product_id));
                        ui.label(format!("uid: {}", uid));
                        ui.label(format!("mac: {}", mac));
                        ui.add_space(4.0);
                        
                        egui::Grid::new(format!("points_{}", zone_name))
                            .num_columns(3)
                            .spacing([8.0, 2.0])
                            .show(ui, |ui| {
                                ui.label(egui::RichText::new("Idx").strong().small());
                                ui.label(egui::RichText::new("Value").strong().small());
                                ui.label(egui::RichText::new("Description").strong().small());
                                ui.end_row();
                                
                                for (idx, val) in &points {
                                    ui.label(format!("{}", idx));
                                    ui.label(crate::models::format_point_value(*idx, val));
                                    ui.label(egui::RichText::new(crate::models::point_index_description(*idx)).small());
                                    ui.end_row();
                                }
                            });
                    });
            });
    }
}

impl eframe::App for EphEmberApp {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        // Persist credentials if logged in, clear if logged out
        if self.screen == Screen::Dashboard && !self.username.is_empty() {
            log::info!("Saving credentials for user");
            storage.set_string("username", self.username.clone());
            storage.set_string("password", self.password.clone());
        } else if self.screen == Screen::Login {
            log::info!("Clearing saved credentials");
            storage.set_string("username", String::new());
            storage.set_string("password", String::new());
        }
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.process_updates();

        match self.screen {
            Screen::Login => self.show_login(ctx),
            Screen::Dashboard => self.show_dashboard(ctx),
        }
    }
}

// --- Backend ---

async fn backend_loop(
    mut cmd_rx: tokio::sync::mpsc::UnboundedReceiver<Command>,
    update_tx: std::sync::mpsc::Sender<Update>,
    ctx: egui::Context,
) {
    let mut api: Option<EphEmberApi> = None;

    let send = |update: Update| {
        update_tx.send(update).ok();
        ctx.request_repaint();
    };

    while let Some(cmd) = cmd_rx.recv().await {
        match cmd {
            Command::Login { username, password } => {
                let mut client = EphEmberApi::new(username, password);
                match client.login_and_fetch().await {
                    Ok(zones) => {
                        send(Update::LoggedIn);
                        send(Update::ZonesUpdated(zones));
                        api = Some(client);
                    }
                    Err(e) => {
                        send(Update::LoginFailed(e.to_string()));
                    }
                }
            }

            Command::Logout => {
                api = None;
                send(Update::LoggedOut);
            }

            Command::RefreshZones => {
                if let Some(ref mut client) = api {
                    match client.get_zones().await {
                        Ok(zones) => send(Update::ZonesUpdated(zones)),
                        Err(e) => send(Update::Error(e.to_string())),
                    }
                }
            }

            Command::SetTargetTemperature {
                zone_name,
                temperature,
            } => {
                if let Some(ref client) = api {
                    if let Some(zone) = client.find_zone(&zone_name) {
                        if let Some(creds) = client.mqtt_credentials() {
                            let cmds = mqtt::set_target_temp_commands(temperature);
                            match mqtt::send_zone_commands(&creds, zone, &cmds).await {
                                Ok(()) => send(Update::CommandSent(format!(
                                    "Set {zone_name} target to {temperature:.1}\u{00B0}C"
                                ))),
                                Err(e) => send(Update::Error(e.to_string())),
                            }
                        }
                    }
                }
            }

            Command::SetMode { zone_name, mode } => {
                if let Some(ref client) = api {
                    if let Some(zone) = client.find_zone(&zone_name) {
                        if let Some(creds) = client.mqtt_credentials() {
                            let cmds = mqtt::set_mode_commands(mode);
                            match mqtt::send_zone_commands(&creds, zone, &cmds).await {
                                Ok(()) => send(Update::CommandSent(format!(
                                    "Set {zone_name} mode to {}",
                                    mode.label()
                                ))),
                                Err(e) => send(Update::Error(e.to_string())),
                            }
                        }
                    }
                }
            }

            Command::ActivateBoost {
                zone_name,
                temperature,
                hours,
            } => {
                if let Some(ref client) = api {
                    if let Some(zone) = client.find_zone(&zone_name) {
                        if let Some(creds) = client.mqtt_credentials() {
                            let cmds = mqtt::activate_boost_commands(temperature, hours);
                            match mqtt::send_zone_commands(&creds, zone, &cmds).await {
                                Ok(()) => send(Update::CommandSent(format!(
                                    "Activated {hours}h boost on {zone_name}"
                                ))),
                                Err(e) => send(Update::Error(e.to_string())),
                            }
                        }
                    }
                }
            }

            Command::DeactivateBoost { zone_name } => {
                if let Some(ref client) = api {
                    if let Some(zone) = client.find_zone(&zone_name) {
                        if let Some(creds) = client.mqtt_credentials() {
                            let cmds = mqtt::deactivate_boost_commands();
                            match mqtt::send_zone_commands(&creds, zone, &cmds).await {
                                Ok(()) => send(Update::CommandSent(format!(
                                    "Deactivated boost on {zone_name}"
                                ))),
                                Err(e) => send(Update::Error(e.to_string())),
                            }
                        }
                    }
                }
            }
        }
    }
}
