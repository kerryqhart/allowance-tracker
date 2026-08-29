//! # Startup banner
//!
//! A coloured strip across the top of the window carrying what startup found
//! and could not fix by itself.
//!
//! It exists because three failures were log-only, and a desktop app's log is
//! not a user interface. All three end in the same place — a child that is not
//! in the picker:
//!
//! - **`children.yaml` would not parse** (or names a version this build does
//!   not understand). The roster starts empty and the file is left untouched;
//!   without a banner that is a wiped app with no explanation.
//! - **Migration skipped a folder** — most importantly a redirect stub whose
//!   iCloud target has not been delivered yet, which is the expected state of
//!   a fresh Mac and the one case that must not read as data loss.
//! - **Migration found an orphan** — a folder holding `transactions.csv` or
//!   `goals.csv` but no `child.yaml`, quite possibly holding real money
//!   history. Its path is named in full, because the path is the only way to
//!   find it again.
//!
//! Deliberately not a notification system: no queue, no timers, no severity
//! routing beyond two colours. Notices are produced once, during
//! `Backend::with_data_dir`, and the user dismisses them.

use eframe::egui;

use crate::backend::{NoticeSeverity, StartupNotice};

/// The notices raised at startup, minus the ones the user has dismissed.
#[derive(Debug, Default)]
pub struct StartupBanner {
    notices: Vec<StartupNotice>,
}

impl StartupBanner {
    pub fn new(notices: Vec<StartupNotice>) -> Self {
        Self { notices }
    }

    pub fn is_empty(&self) -> bool {
        self.notices.is_empty()
    }

    pub fn notices(&self) -> &[StartupNotice] {
        &self.notices
    }

    /// Dismiss one notice. Dismissal is for this run only — the condition is
    /// re-detected at the next launch if it is still true, which is the point:
    /// an unreadable `children.yaml` should keep saying so until it is fixed.
    pub fn dismiss(&mut self, index: usize) {
        if index < self.notices.len() {
            self.notices.remove(index);
        }
    }
}

/// Foreground/background for a severity. Chosen to read against the app's
/// pale background without competing with the header.
fn colors(severity: NoticeSeverity) -> (egui::Color32, egui::Color32) {
    match severity {
        NoticeSeverity::Warning => (
            egui::Color32::from_rgb(255, 244, 214),
            egui::Color32::from_rgb(120, 80, 10),
        ),
        NoticeSeverity::Error => (
            egui::Color32::from_rgb(255, 227, 227),
            egui::Color32::from_rgb(140, 35, 35),
        ),
    }
}

impl crate::ui::app_state::AllowanceTrackerApp {
    /// Paint the banner above everything else, if there is anything to say.
    ///
    /// A `TopBottomPanel` rather than a slice of the `CentralPanel`: the
    /// central layout divides `available_rect` into four fixed-height bands by
    /// hand, and a panel keeps the banner out of that arithmetic entirely.
    pub fn render_startup_banner(&mut self, ctx: &egui::Context) {
        if self.startup_banner.is_empty() {
            return;
        }

        let mut dismissed: Option<usize> = None;

        egui::TopBottomPanel::top("startup_banner")
            .frame(egui::Frame::NONE)
            .show_separator_line(false)
            .show(ctx, |ui| {
                for (index, notice) in self.startup_banner.notices().iter().enumerate() {
                    let (background, text_color) = colors(notice.severity);
                    egui::Frame::NONE
                        .fill(background)
                        .inner_margin(egui::Margin::symmetric(14, 10))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.vertical(|ui| {
                                    // Leave room for the dismiss control so a
                                    // long path doesn't run underneath it.
                                    ui.set_max_width((ui.available_width() - 40.0).max(120.0));
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(&notice.title)
                                                .font(egui::FontId::new(
                                                    15.0,
                                                    egui::FontFamily::Proportional,
                                                ))
                                                .strong()
                                                .color(text_color),
                                        )
                                        .wrap(),
                                    );
                                    for detail in &notice.details {
                                        ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(detail)
                                                    .font(egui::FontId::new(
                                                        13.0,
                                                        egui::FontFamily::Proportional,
                                                    ))
                                                    .color(text_color),
                                            )
                                            .wrap(),
                                        );
                                    }
                                });

                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::TOP),
                                    |ui| {
                                        if ui
                                            .add(
                                                egui::Button::new(
                                                    egui::RichText::new("✖")
                                                        .color(text_color),
                                                )
                                                .frame(false),
                                            )
                                            .on_hover_text("Dismiss")
                                            .clicked()
                                        {
                                            dismissed = Some(index);
                                        }
                                    },
                                );
                            });
                        });
                    ui.add_space(2.0);
                }
            });

        if let Some(index) = dismissed {
            self.startup_banner.dismiss(index);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notice(title: &str) -> StartupNotice {
        StartupNotice {
            severity: NoticeSeverity::Warning,
            title: title.to_string(),
            details: vec!["detail".to_string()],
        }
    }

    #[test]
    fn a_banner_with_no_notices_is_empty() {
        assert!(StartupBanner::default().is_empty());
    }

    #[test]
    fn dismissing_removes_only_that_notice() {
        let mut banner = StartupBanner::new(vec![notice("first"), notice("second")]);
        banner.dismiss(0);
        assert_eq!(banner.notices().len(), 1);
        assert_eq!(banner.notices()[0].title, "second");
    }

    #[test]
    fn dismissing_a_notice_that_is_not_there_is_a_no_op() {
        let mut banner = StartupBanner::new(vec![notice("only")]);
        banner.dismiss(7);
        assert_eq!(banner.notices().len(), 1);
    }
}
