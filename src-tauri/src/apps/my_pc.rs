use serde::Serialize;
use sysinfo::{CpuRefreshKind, MemoryRefreshKind, RefreshKind, System};

use super::gpu;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PcInfo {
    /// "windows" | "linux" | "macos" | other `std::env::consts::OS` values.
    os_family: &'static str,
    os_name: String,
    kernel: String,
    hostname: String,
    username: String,
    arch: String,
    uptime_secs: u64,
    cpu_brand: String,
    cpu_physical_cores: Option<usize>,
    cpu_logical_cores: usize,
    cpu_frequency_mhz: u64,
    memory_total: u64,
    memory_used: u64,
    swap_total: u64,
    swap_used: u64,
    gpus: Vec<String>,
}

#[tauri::command]
pub async fn my_pc_info() -> Result<PcInfo, String> {
    let system = System::new_with_specifics(
        RefreshKind::nothing()
            .with_cpu(CpuRefreshKind::nothing().with_frequency())
            .with_memory(MemoryRefreshKind::everything()),
    );
    let cpus = system.cpus();

    Ok(PcInfo {
        os_family: std::env::consts::OS,
        os_name: System::long_os_version()
            .or_else(System::name)
            .unwrap_or_default(),
        kernel: System::kernel_version().unwrap_or_default(),
        hostname: System::host_name().unwrap_or_default(),
        username: std::env::var("USERNAME")
            .or_else(|_| std::env::var("USER"))
            .unwrap_or_default(),
        arch: System::cpu_arch(),
        uptime_secs: System::uptime(),
        cpu_brand: cpus
            .first()
            .map(|cpu| cpu.brand().trim().to_string())
            .unwrap_or_default(),
        cpu_physical_cores: System::physical_core_count(),
        cpu_logical_cores: cpus.len(),
        cpu_frequency_mhz: cpus.first().map_or(0, |cpu| cpu.frequency()),
        memory_total: system.total_memory(),
        memory_used: system.used_memory(),
        swap_total: system.total_swap(),
        swap_used: system.used_swap(),
        gpus: gpu::gpu_names(),
    })
}
