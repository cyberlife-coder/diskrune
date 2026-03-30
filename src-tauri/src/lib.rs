pub mod node;
pub mod scanner;

mod commands;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            commands::start_scan,
            commands::open_path
        ])
        .run(tauri::generate_context!())
        .expect("diskrune failed to start");
}
