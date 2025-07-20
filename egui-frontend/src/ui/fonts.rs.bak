use log::{info, warn};
use eframe::egui;
use std::path::Path;

/// Helper function to load system fonts on macOS
pub fn load_system_font(font_name: &str) -> Option<Vec<u8>> {
    // macOS system font directories
    let font_paths = [
        format!("/System/Library/Fonts/{}.ttc", font_name),
        format!("/System/Library/Fonts/{}.ttf", font_name),
        format!("/Library/Fonts/{}.ttc", font_name),
        format!("/Library/Fonts/{}.ttf", font_name),
        format!("/System/Library/Fonts/Supplemental/{}.ttf", font_name),
        format!("/System/Library/Fonts/Supplemental/{}.ttc", font_name),
    ];
    
    for path in &font_paths {
        if Path::new(path).exists() {
            match std::fs::read(path) {
                Ok(font_data) => {
                    info!("🎨 Successfully loaded font: {}", path);
                    return Some(font_data);
                }
                Err(e) => {
                    warn!("Failed to read font file {}: {}", path, e);
                }
            }
        }
    }
    
    warn!("Could not find font: {}", font_name);
    None
}

/// Helper function to load Apple Color Emoji font on macOS
pub fn load_emoji_font() -> Option<Vec<u8>> {
    // Apple Color Emoji is typically in a specific location
    let emoji_paths = [
        "/System/Library/Fonts/Apple Color Emoji.ttc",
        "/System/Library/Fonts/Supplemental/Apple Color Emoji.ttc",
    ];
    
    for path in &emoji_paths {
        if Path::new(path).exists() {
            match std::fs::read(path) {
                Ok(font_data) => {
                    info!("🎨 Successfully loaded emoji font: {}", path);
                    return Some(font_data);
                }
                Err(e) => {
                    warn!("Failed to read emoji font file {}: {}", path, e);
                }
            }
        }
    }
    
    warn!("Could not find Apple Color Emoji font");
    None
}

/// Setup custom fonts including Chalkboard and Apple Color Emoji
pub fn setup_custom_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    
    // Try to load Chalkboard font
    if let Some(font_data) = load_system_font("Chalkboard") {
        fonts.font_data.insert(
            "Chalkboard".to_owned(),
            egui::FontData::from_owned(font_data),
        );
        
        // Create a new font family for Chalkboard
        fonts.families.insert(
            egui::FontFamily::Name("Chalkboard".into()),
            vec!["Chalkboard".to_owned()],
        );
        
        info!("✅ Chalkboard font loaded successfully!");
    } else {
        warn!("⚠️ Could not load Chalkboard font, using default fonts");
    }
    
    // Try to load Apple Color Emoji font and add it to default fonts (not Chalkboard)
    if let Some(emoji_data) = load_emoji_font() {
        fonts.font_data.insert(
            "AppleColorEmoji".to_owned(),
            egui::FontData::from_owned(emoji_data),
        );
        
        // Add emoji font as fallback to default system fonts only
        if let Some(proportional_fonts) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
            proportional_fonts.push("AppleColorEmoji".to_owned());
            info!("✅ Added emoji support to Proportional font family");
        }
        
        if let Some(monospace_fonts) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
            monospace_fonts.push("AppleColorEmoji".to_owned());
            info!("✅ Added emoji support to Monospace font family");
        }
        
        info!("✅ Apple Color Emoji font loaded and added to default fonts!");
    } else {
        warn!("⚠️ Could not load Apple Color Emoji font");
    }
    
    // Set the fonts
    ctx.set_fonts(fonts);
} 