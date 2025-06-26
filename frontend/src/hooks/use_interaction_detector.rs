use yew::prelude::*;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{window, HtmlElement};
use gloo::timers::future::TimeoutFuture;
use wasm_bindgen_futures::spawn_local;

/// Configuration for interaction detection
#[derive(Clone, PartialEq)]
pub struct InteractionDetectorConfig {
    pub grace_period_ms: u32,
    pub enable_logging: bool,
}

impl Default for InteractionDetectorConfig {
    fn default() -> Self {
        Self {
            grace_period_ms: 5000, // 5 seconds
            enable_logging: false,  // Disable by default for production
        }
    }
}

/// Result from the interaction detector hook
#[derive(Clone, PartialEq)]
pub struct UseInteractionDetectorResult {
    pub is_active: bool,
    pub last_interaction_time: Option<f64>,
    pub interaction_count: u32,
}

/// Hook for detecting user interactions to pause periodic refreshing
/// 
/// Monitors DOM events that indicate active user interaction:
/// - Form inputs (typing, selecting)
/// - Calendar chip hovers
/// - Settings menu interactions
/// - Date picker usage
/// 
/// # Features
/// - 5-second grace period after last interaction
/// - Efficient event listener management
/// - Proper cleanup on component unmount
/// - Optional debug logging
#[hook]
pub fn use_interaction_detector(
    config: Option<InteractionDetectorConfig>,
) -> UseInteractionDetectorResult {
    let config = config.unwrap_or_default();
    
    let is_active = use_state(|| false);
    let last_interaction_time = use_state(|| Option::<f64>::None);
    let interaction_count = use_state(|| 0u32);
    let grace_timer_active = use_state(|| false);

    // Track if component is mounted
    let is_mounted = use_state(|| true);

    // Event handler for interactions
    let handle_interaction = {
        let is_active = is_active.clone();
        let last_interaction_time = last_interaction_time.clone();
        let interaction_count = interaction_count.clone();
        let grace_timer_active = grace_timer_active.clone();
        let is_mounted = is_mounted.clone();
        let config = config.clone();

        Callback::from(move |event_type: String| {
            if !*is_mounted {
                return;
            }

            let current_time = js_sys::Date::now();
            
            // Update interaction state
            is_active.set(true);
            last_interaction_time.set(Some(current_time));
            interaction_count.set(*interaction_count + 1);

            if config.enable_logging {
                gloo::console::debug!(&format!(
                    "👆 User interaction detected: {} (count: {})",
                    event_type,
                    *interaction_count
                ));
            }

            // Start grace period timer if not already active
            if !*grace_timer_active {
                grace_timer_active.set(true);
                
                let is_active = is_active.clone();
                let grace_timer_active = grace_timer_active.clone();
                let is_mounted = is_mounted.clone();
                let grace_period = config.grace_period_ms;
                let enable_logging = config.enable_logging;

                spawn_local(async move {
                    TimeoutFuture::new(grace_period).await;
                    
                    if *is_mounted {
                        is_active.set(false);
                        grace_timer_active.set(false);
                        
                        if enable_logging {
                            gloo::console::debug!("⏳ Grace period ended, interactions now inactive");
                        }
                    }
                });
            }
        })
    };

    // Set up event listeners on mount
    {
        let handle_interaction = handle_interaction.clone();
        let is_mounted = is_mounted.clone();
        
        use_effect_with((), move |_| {
            let window = window().expect("should have window");
            let document = window.document().expect("should have document");
            
            // Create closures for event handlers
            let handle_input = {
                let handle_interaction = handle_interaction.clone();
                Closure::wrap(Box::new(move |_event: web_sys::Event| {
                    handle_interaction.emit("input".to_string());
                }) as Box<dyn FnMut(_)>)
            };

            let handle_focus = {
                let handle_interaction = handle_interaction.clone();
                Closure::wrap(Box::new(move |_event: web_sys::Event| {
                    handle_interaction.emit("focus".to_string());
                }) as Box<dyn FnMut(_)>)
            };

            let handle_mouseenter = {
                let handle_interaction = handle_interaction.clone();
                Closure::wrap(Box::new(move |event: web_sys::Event| {
                    // Only trigger for specific elements (calendar chips, tooltips)
                    if let Some(target) = event.target() {
                        if let Ok(element) = target.dyn_into::<HtmlElement>() {
                            let class_name = element.class_name();
                            // Check for calendar chip or tooltip classes
                            if class_name.contains("transaction-chip") 
                                || class_name.contains("calendar-day") 
                                || class_name.contains("tooltip")
                                || element.has_attribute("data-tooltip") {
                                handle_interaction.emit("calendar_hover".to_string());
                            }
                        }
                    }
                }) as Box<dyn FnMut(_)>)
            };

            let handle_click = {
                let handle_interaction = handle_interaction.clone();
                Closure::wrap(Box::new(move |event: web_sys::Event| {
                    // Only trigger for specific elements (settings menu, buttons)
                    if let Some(target) = event.target() {
                        if let Ok(element) = target.dyn_into::<HtmlElement>() {
                            let class_name = element.class_name();
                            // Check for settings menu, modal, or important button classes
                            if class_name.contains("settings-menu") 
                                || class_name.contains("modal")
                                || class_name.contains("dropdown") {
                                handle_interaction.emit("menu_interaction".to_string());
                            }
                        }
                    }
                }) as Box<dyn FnMut(_)>)
            };

            // Add event listeners
            let _ = document.add_event_listener_with_callback(
                "input",
                handle_input.as_ref().unchecked_ref()
            );
            let _ = document.add_event_listener_with_callback(
                "focus",
                handle_focus.as_ref().unchecked_ref()
            );
            let _ = document.add_event_listener_with_callback(
                "mouseenter",
                handle_mouseenter.as_ref().unchecked_ref()
            );
            let _ = document.add_event_listener_with_callback(
                "click",
                handle_click.as_ref().unchecked_ref()
            );

            // Store closures to prevent them from being dropped
            let closures = vec![handle_input, handle_focus, handle_mouseenter, handle_click];

            // Cleanup function
            move || {
                // Remove event listeners
                for closure in closures.iter() {
                    let _ = document.remove_event_listener_with_callback(
                        "input",
                        closure.as_ref().unchecked_ref()
                    );
                    let _ = document.remove_event_listener_with_callback(
                        "focus", 
                        closure.as_ref().unchecked_ref()
                    );
                    let _ = document.remove_event_listener_with_callback(
                        "mouseenter",
                        closure.as_ref().unchecked_ref()
                    );
                    let _ = document.remove_event_listener_with_callback(
                        "click",
                        closure.as_ref().unchecked_ref()
                    );
                }
                
                gloo::console::debug!("🧹 Interaction detector event listeners cleaned up");
            }
        });
    }

    // Cleanup on component unmount
    {
        let is_mounted = is_mounted.clone();
        
        use_effect_with((), move |_| {
            move || {
                is_mounted.set(false);
                gloo::console::debug!("🧹 Interaction detector hook cleaned up");
            }
        });
    }

    UseInteractionDetectorResult {
        is_active: *is_active,
        last_interaction_time: *last_interaction_time,
        interaction_count: *interaction_count,
    }
}

/// Convenience function for default interaction detection
#[hook]
pub fn use_interaction_detector_simple() -> UseInteractionDetectorResult {
    use_interaction_detector(None)
}

/// Convenience function for interaction detection with logging enabled (for debugging)
#[hook]
pub fn use_interaction_detector_debug() -> UseInteractionDetectorResult {
    let config = InteractionDetectorConfig {
        enable_logging: true,
        ..InteractionDetectorConfig::default()
    };
    use_interaction_detector(Some(config))
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::*;

    wasm_bindgen_test_configure!(run_in_browser);

    #[wasm_bindgen_test]
    fn test_config_default() {
        let config = InteractionDetectorConfig::default();
        assert_eq!(config.grace_period_ms, 5000);
        assert_eq!(config.enable_logging, false);
    }

    #[wasm_bindgen_test]
    fn test_config_debug() {
        let config = InteractionDetectorConfig {
            enable_logging: true,
            ..InteractionDetectorConfig::default()
        };
        assert_eq!(config.enable_logging, true);
        assert_eq!(config.grace_period_ms, 5000);
    }
} 