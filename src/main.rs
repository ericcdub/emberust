use eframe::egui;

const APP_KEY: &str = "emberust";

fn main() -> eframe::Result {
    env_logger::init();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([900.0, 640.0])
            .with_min_inner_size([400.0, 300.0])
            .with_title("Emberust"),
        persist_window: true,
        ..Default::default()
    };

    eframe::run_native(
        APP_KEY,
        options,
        Box::new(|cc| Ok(Box::new(emberust::app::EphEmberApp::new(cc)))),
    )
}
