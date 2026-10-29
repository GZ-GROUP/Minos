use std::sync::Mutex;

use serde::Serialize;
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};
use tauri::State;

/// Long-lived `System` so CPU usage is measured as a delta between consecutive polls.
pub struct SystemState(pub Mutex<System>);

impl Default for SystemState {
    fn default() -> Self {
        let mut system = System::new();
        // Prime the process list so the first poll from the UI already has a CPU baseline.
        refresh(&mut system);
        Self(Mutex::new(system))
    }
}

fn refresh(system: &mut System) {
    system.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().with_cpu().with_memory(),
    );
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessInfo {
    pid: u32,
    name: String,
    /// Share of total CPU capacity (0–100), like Task Manager.
    cpu_percent: f32,
    memory_bytes: u64,
}

#[tauri::command]
pub async fn processes_list(state: State<'_, SystemState>) -> Result<Vec<ProcessInfo>, String> {
    let mut system = state.0.lock().map_err(|e| e.to_string())?;
    refresh(&mut system);

    // sysinfo reports per-process CPU relative to one core; normalize to the whole machine.
    let cores = std::thread::available_parallelism().map_or(1, |n| n.get()) as f32;

    Ok(system
        .processes()
        .values()
        // PID 0 is the idle/kernel pseudo-process on Windows and macOS.
        .filter(|process| process.pid().as_u32() != 0)
        .map(|process| ProcessInfo {
            pid: process.pid().as_u32(),
            name: process.name().to_string_lossy().into_owned(),
            cpu_percent: (process.cpu_usage() / cores).clamp(0.0, 100.0),
            memory_bytes: process.memory(),
        })
        .collect())
}
