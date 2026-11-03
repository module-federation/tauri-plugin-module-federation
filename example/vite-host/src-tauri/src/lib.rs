#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut builder = tauri::Builder::default().plugin(tauri_plugin_module_federation::init(None));

    // Smoke-test reporter; inactive unless MF_E2E is set.
    if let Some(e2e) = example_tauri_e2e::plugin() {
        builder = builder
            .plugin(e2e)
            .invoke_handler(example_tauri_e2e::handler());
    }

    builder
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
