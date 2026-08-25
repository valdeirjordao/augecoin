mod commands;
mod machine;
mod ops;
mod state;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|_app| {
            // Re-start the node + heartbeat if already activated on this machine.
            commands::resume();
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::machine_id,
            commands::get_state,
            commands::clear_state,
            commands::activate,
            commands::get_dashboard,
            commands::node_status,
            commands::check_update,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
