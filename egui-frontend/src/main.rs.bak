use eframe::egui;
use env_logger;
use log::{info, error};
use image::GenericImageView;

mod app;

use allowance_tracker_egui::ui::AllowanceTrackerApp;

fn load_app_icon() -> Result<egui::IconData, Box<dyn std::error::Error>> {
    let icon_bytes = include_bytes!("../assets/app-icon.png");
    let image = image::load_from_memory(icon_bytes)?;
    let rgba = image.to_rgba8();
    let (width, height) = image.dimensions();
    
    Ok(egui::IconData {
        rgba: rgba.into_raw(),
        width,
        height,
    })
}

fn main() -> Result<(), eframe::Error> {
    // Initialize logging for debugging
    env_logger::init();
    info!("Starting Allowance Tracker egui application");

    // Load custom app icon
    let app_icon = match load_app_icon() {
        Ok(icon) => {
            info!("✅ Successfully loaded custom app icon");
            Some(icon)
        }
        Err(e) => {
            error!("Failed to load custom app icon: {}", e);
            None
        }
    };

    // Create window options optimized for a kid-friendly app
    let mut viewport_builder = egui::ViewportBuilder::default()
        .with_inner_size([1200.0, 800.0])  // Good size for calendar + forms
        .with_min_inner_size([800.0, 600.0])   // Minimum usable size
        .with_max_inner_size([1600.0, 1200.0]) // Prevent it from getting too big
        .with_title("My Allowance Tracker")
        .with_resizable(true); // Removed with_centered - not supported in this version

    // Set custom icon if loaded successfully
    if let Some(icon) = app_icon {
        viewport_builder = viewport_builder.with_icon(icon);
    }

    let options = eframe::NativeOptions {
        viewport: viewport_builder,
        ..Default::default()
    };

    // Run the application
    info!("Launching egui window");
    eframe::run_native(
        "My Allowance Tracker",
        options,
        Box::new(|cc| {
            // Enable persistence for window state
            if let Some(_storage) = cc.storage {
                info!("Persistence storage available");
            }

            // Initialize the app
            match AllowanceTrackerApp::new(cc) {
                Ok(app) => {
                    info!("Successfully initialized Allowance Tracker app");
                    Ok(Box::new(app))
                }
                Err(e) => {
                    error!("Failed to initialize app: {}", e);
                    // Convert anyhow::Error to eframe::Error
                    Err(format!("Failed to initialize app: {}", e).into())
                }
            }
        }),
    )
} 