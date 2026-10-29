mod apps;

use apps::ping_pong::PingState;
use apps::processes::SystemState;
use apps::share_network::ShareState;
use apps::share_scp::ScpState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(SystemState::default())
        .manage(PingState::default())
        .manage(ShareState::default())
        .manage(ScpState::default())
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
            apps::my_files::files_permissions,
            apps::my_files::files_set_permissions,
            apps::file_io::file_info,
            apps::file_io::file_export,
            apps::file_io::report_save,
            apps::pdf_tools::pdf_info,
            apps::pdf_tools::pdf_crop,
            apps::pdf_tools::pdf_merge,
            apps::convert::convert_engine,
            apps::convert::convert_pdf_to_word,
            apps::convert::convert_word_to_pdf,
            apps::share_network::share_start,
            apps::share_network::share_status,
            apps::share_network::share_stop,
            apps::share_scp::scp_connect,
            apps::share_scp::scp_list,
            apps::share_scp::scp_upload,
            apps::share_scp::scp_download,
            apps::share_scp::scp_disconnect,
            apps::ping_pong::ping_run,
            apps::ping_pong::ping_stop,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
