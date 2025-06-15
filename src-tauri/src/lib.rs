mod backend;

use backend::create_app_state;
use tokio::runtime::Runtime;
use std::thread;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  tauri::Builder::default()
    .setup(|app| {
      if cfg!(debug_assertions) {
        app.handle().plugin(
          tauri_plugin_log::Builder::default()
            .level(log::LevelFilter::Info)
            .build(),
        )?;
      }
      
      // Start embedded backend server
      start_embedded_server();
      
      Ok(())
    })
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}

fn start_embedded_server() {
    thread::spawn(|| {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            // Initialize the app state
            let app_state = create_app_state().await.unwrap();
            
            // Create the Axum app
            let app = backend::create_router(app_state);
            
            // Start the server on localhost:3000
            let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
                .await
                .unwrap();
            
            println!("Embedded server starting on http://127.0.0.1:3000");
            
            axum::serve(listener, app)
                .await
                .unwrap();
        });
    });
}
