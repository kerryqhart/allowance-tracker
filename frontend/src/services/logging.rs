use gloo::net::http::Request;
use serde::{Deserialize, Serialize};
use wasm_bindgen_futures::spawn_local;

#[derive(Debug, Serialize)]
struct LogRequest {
    level: String,
    message: String,
    component: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LogResponse {
    success: bool,
}

pub struct Logger;

impl Logger {
    pub fn debug_with_component(component: &str, message: &str) {
        Self::log("debug", message, Some(component.to_string()));
    }

    pub fn info_with_component(component: &str, message: &str) {
        Self::log("info", message, Some(component.to_string()));
    }

    pub fn warn_with_component(component: &str, message: &str) {
        Self::log("warn", message, Some(component.to_string()));
    }

    pub fn error_with_component(component: &str, message: &str) {
        Self::log("error", message, Some(component.to_string()));
    }

    fn log(level: &str, message: &str, component: Option<String>) {
        let request = LogRequest {
            level: level.to_string(),
            message: message.to_string(),
            component,
        };

        // Send log asynchronously without blocking
        spawn_local(async move {
            let _ = Request::post("http://localhost:3000/api/logs")
                .json(&request)
                .unwrap()
                .send()
                .await;
        });
    }
} 