use eframe::egui;
use env_logger;
use log::{info, error};

mod app;

use app::AllowanceTrackerApp;

fn main() -> Result<(), eframe::Error> {
    // Initialize logging for debugging
    env_logger::init();
    info!("Starting Allowance Tracker egui application");

    // Create window options optimized for a kid-friendly app
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])  // Good size for calendar + forms
            .with_min_inner_size([800.0, 600.0])   // Minimum usable size
            .with_max_inner_size([1600.0, 1200.0]) // Prevent it from getting too big
            .with_title("My Allowance Tracker")
            .with_resizable(true), // Removed with_centered - not supported in this version
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
            match AllowanceTrackerApp::new() {
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