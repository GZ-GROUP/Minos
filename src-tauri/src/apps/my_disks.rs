use serde::Serialize;
use sysinfo::{DiskKind, Disks};

/// Pseudo / image file systems that are not user-facing storage.
const IGNORED_FILE_SYSTEMS: &[&str] = &["squashfs", "overlay", "tmpfs", "devtmpfs", "ramfs"];

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskInfo {
    name: String,
    mount_point: String,
    file_system: String,
    /// "ssd" | "hdd" | "unknown"
    kind: &'static str,
    removable: bool,
    total_bytes: u64,
    available_bytes: u64,
}

#[tauri::command]
pub async fn disks_list() -> Result<Vec<DiskInfo>, String> {
    let disks = Disks::new_with_refreshed_list();
    let mut seen_mounts = Vec::new();

    Ok(disks
        .list()
        .iter()
        .filter(|disk| disk.total_space() > 0)
        .filter(|disk| {
            let fs = disk.file_system().to_string_lossy().to_lowercase();
            !IGNORED_FILE_SYSTEMS.contains(&fs.as_str())
        })
        .filter_map(|disk| {
            let mount_point = disk.mount_point().to_string_lossy().into_owned();
            if seen_mounts.contains(&mount_point) {
                return None;
            }
            seen_mounts.push(mount_point.clone());

            let name = disk.name().to_string_lossy().trim().to_string();
            Some(DiskInfo {
                name: if name.is_empty() {
                    mount_point.clone()
                } else {
                    name
                },
                mount_point,
                file_system: disk.file_system().to_string_lossy().into_owned(),
                kind: match disk.kind() {
                    DiskKind::SSD => "ssd",
                    DiskKind::HDD => "hdd",
                    DiskKind::Unknown(_) => "unknown",
                },
                removable: disk.is_removable(),
                total_bytes: disk.total_space(),
                available_bytes: disk.available_space(),
            })
        })
        .collect())
}
