use eframe::egui;

fn main() -> eframe::Result {
    env_logger::init();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([900.0, 640.0])
            .with_min_inner_size([400.0, 300.0])
            .with_title("EPH Ember Controller"),
        ..Default::default()
    };

    eframe::run_native(
        "EPH Ember Controller",
        options,
        Box::new(|cc| Ok(Box::new(eph_ember::app::EphEmberApp::new(cc)))),
    )
}
