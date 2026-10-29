mod apps;

use apps::processes::SystemState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(SystemState::default())
        .invoke_handler(tauri::generate_handler![
            apps::processes::processes_list,
            apps::my_pc::my_pc_info,
            apps::my_network::network_adapters,
            apps::my_disks::disks_list,
            apps::my_files::files_home,
            apps::my_files::files_list,
            apps::my_files::files_create_file,
            apps::my_files::files_create_dir,
            apps::my_files::files_rename,
            apps::my_files::files_delete,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
