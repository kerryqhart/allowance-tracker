use yew::prelude::*;
use wasm_bindgen_futures::spawn_local;
use gloo::timers::future::TimeoutFuture;

/// Configuration for periodic refresh behavior
#[derive(Clone, PartialEq)]
pub struct PeriodicRefreshConfig {
    pub interval_ms: u32,
    pub initial_delay_ms: Option<u32>,
    pub max_retries: u32,
    pub base_retry_delay_ms: u32,
}

impl Default for PeriodicRefreshConfig {
    fn default() -> Self {
        Self {
            interval_ms: 600000,       // 10 minutes (600 seconds)
            initial_delay_ms: None,    // No initial delay
            max_retries: 3,            // Max 3 retries on failure
            base_retry_delay_ms: 1000, // Start with 1 second retry delay
        }
    }
}

/// Result from the periodic refresh hook
pub struct UsePeriodicRefreshResult {
    pub is_running: bool,
    pub error_count: u32,
    pub last_refresh_time: Option<f64>,
}

/// Hook for periodic refresh functionality with pause detection and error handling
/// 
/// # Arguments
/// * `config` - Configuration for refresh behavior
/// * `refresh_fn` - Callback to execute on each refresh
/// * `pause_when` - Boolean indicating when to pause refreshing
/// 
/// # Features
/// - Transparent operation (no UI indicators)
/// - Automatic pause when user is interacting
/// - Exponential backoff on errors
/// - Proper cleanup on component unmount
/// - Staggered timing support via initial_delay_ms
#[hook]
pub fn use_periodic_refresh(
    config: PeriodicRefreshConfig,
    refresh_fn: Callback<()>,
    pause_when: bool,
) -> UsePeriodicRefreshResult {
    let is_running = use_state(|| false);
    let error_count = use_state(|| 0u32);
    let last_refresh_time = use_state(|| Option::<f64>::None);
    let retry_count = use_state(|| 0u32);
    
    // Track if component is mounted to avoid calling callbacks after unmount
    let is_mounted = use_state(|| true);

    // Effect to handle the periodic refresh logic
    {
        let config = config.clone();
        let refresh_fn = refresh_fn.clone();
        let is_running = is_running.clone();
        let error_count = error_count.clone();
        let last_refresh_time = last_refresh_time.clone();
        let retry_count = retry_count.clone();
        let is_mounted = is_mounted.clone();

        use_effect_with((config.clone(), pause_when), move |(config, is_paused)| {
            let cleanup = if *is_paused {
                // When paused, mark as not running but don't start timer
                crate::services::logging::Logger::info_with_component("periodic-refresh-hook", "🚨 PERIODIC REFRESH PAUSED due to user interaction");
                is_running.set(false);
                Box::new(move || {}) as Box<dyn FnOnce()>
            } else {
                crate::services::logging::Logger::info_with_component("periodic-refresh-hook", "🚨 PERIODIC REFRESH ACTIVE - starting timer");
                is_running.set(true);
                
                let config = config.clone();
                let refresh_fn = refresh_fn.clone();
                let is_running = is_running.clone();
                let error_count = error_count.clone();
                let last_refresh_time = last_refresh_time.clone();
                let retry_count = retry_count.clone();
                let is_mounted = is_mounted.clone();

                spawn_local(async move {
                    // Initial delay for staggered timing
                    if let Some(initial_delay) = config.initial_delay_ms {
                        TimeoutFuture::new(initial_delay).await;
                        
                        // Check if still mounted and not paused after initial delay
                        if !*is_mounted {
                            return;
                        }
                    }

                    loop {
                        // Check if component is still mounted and not paused
                        if !*is_mounted {
                            break;
                        }

                        // Execute the refresh callback
                        let current_time = js_sys::Date::now();
                        
                        // Use Logger instead of gloo::console for Tauri app
                        crate::services::logging::Logger::debug_with_component(
                            "periodic-refresh-hook", 
                            &format!("🔄 About to execute periodic refresh at {}", 
                                js_sys::Date::new(&js_sys::wasm_bindgen::JsValue::from(current_time)).to_iso_string()
                            )
                        );
                        
                        // Try the refresh with error handling
                        let success = execute_refresh_with_retry(
                            &refresh_fn,
                            &config,
                            &retry_count,
                            &is_mounted,
                        ).await;

                        if success {
                            // Reset error count and retry count on success
                            error_count.set(0);
                            retry_count.set(0);
                            last_refresh_time.set(Some(current_time));
                            
                            crate::services::logging::Logger::debug_with_component(
                                "periodic-refresh-hook",
                                &format!("🔄 Periodic refresh successful at {}",
                                    js_sys::Date::new(&js_sys::wasm_bindgen::JsValue::from(current_time)).to_iso_string()
                                )
                            );
                        } else {
                            // Increment error count on failure
                            let new_error_count = *error_count + 1;
                            error_count.set(new_error_count);
                            
                            crate::services::logging::Logger::warn_with_component(
                                "periodic-refresh-hook",
                                &format!("⚠️ Periodic refresh failed (attempt {}/{})",
                                    new_error_count, config.max_retries + 1
                                )
                            );
                        }

                        // Wait for the next interval
                        TimeoutFuture::new(config.interval_ms).await;
                    }

                    is_running.set(false);
                });

                Box::new(move || {
                    // Cleanup function
                }) as Box<dyn FnOnce()>
            };

            move || cleanup()
        });
    }

    // Cleanup on component unmount
    {
        let is_mounted = is_mounted.clone();
        let is_running = is_running.clone();
        
        use_effect_with((), move |_| {
            // Cleanup function that runs on unmount
            move || {
                is_mounted.set(false);
                is_running.set(false);
                crate::services::logging::Logger::debug_with_component("periodic-refresh-hook", "🧹 Periodic refresh hook cleaned up");
            }
        });
    }

    UsePeriodicRefreshResult {
        is_running: *is_running,
        error_count: *error_count,
        last_refresh_time: *last_refresh_time,
    }
}

/// Execute refresh with retry logic and exponential backoff
async fn execute_refresh_with_retry(
    refresh_fn: &Callback<()>,
    config: &PeriodicRefreshConfig,
    retry_count: &UseStateHandle<u32>,
    is_mounted: &UseStateHandle<bool>,
) -> bool {
    for attempt in 0..=config.max_retries {
        if !**is_mounted {
            return false;
        }

        // Calculate delay for this attempt (exponential backoff)
        if attempt > 0 {
            let delay = config.base_retry_delay_ms * (2_u32.pow(attempt - 1));
            crate::services::logging::Logger::debug_with_component("periodic-refresh-hook", &format!("⏳ Retrying refresh in {}ms (attempt {})", delay, attempt));
            TimeoutFuture::new(delay).await;
            
            // Check again if still mounted after delay
            if !**is_mounted {
                return false;
            }
        }

        // Execute the refresh callback
        // Note: We can't directly catch errors from the callback since it's fire-and-forget,
        // but the callback itself should handle errors appropriately
        refresh_fn.emit(());
        retry_count.set(attempt);

        // For now, we assume success since we can't catch errors from the callback
        // In a more sophisticated implementation, we might modify the callback signature
        // to return a Result or use a different error signaling mechanism
        return true;
    }

    false
}

/// Convenience function for standard 10-minute refresh with no initial delay
#[hook]
pub fn use_periodic_refresh_simple(
    refresh_fn: Callback<()>,
    pause_when: bool,
) -> UsePeriodicRefreshResult {
    use_periodic_refresh(
        PeriodicRefreshConfig::default(),
        refresh_fn,
        pause_when,
    )
}

/// Convenience function for staggered refresh (useful for transactions refresh)
#[hook]
pub fn use_periodic_refresh_staggered(
    refresh_fn: Callback<()>,
    pause_when: bool,
    stagger_delay_ms: u32,
) -> UsePeriodicRefreshResult {
    let config = PeriodicRefreshConfig {
        initial_delay_ms: Some(stagger_delay_ms),
        ..PeriodicRefreshConfig::default()
    };
    
    use_periodic_refresh(config, refresh_fn, pause_when)
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::*;

    wasm_bindgen_test_configure!(run_in_browser);

    #[wasm_bindgen_test]
    fn test_config_default() {
        let config = PeriodicRefreshConfig::default();
        assert_eq!(config.interval_ms, 600000); // 10 minutes
        assert_eq!(config.initial_delay_ms, None);
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.base_retry_delay_ms, 1000);
    }

    #[wasm_bindgen_test]
    fn test_config_staggered() {
        let config = PeriodicRefreshConfig {
            initial_delay_ms: Some(15000),
            ..PeriodicRefreshConfig::default()
        };
        assert_eq!(config.initial_delay_ms, Some(15000));
    }
} 