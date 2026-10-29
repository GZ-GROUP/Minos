//! GPU names. There is no portable API for this, so each OS has its own source.
//! Every implementation degrades to an empty list instead of failing.

#[cfg(target_os = "windows")]
pub fn gpu_names() -> Vec<String> {
    use winreg::{RegKey, enums::HKEY_LOCAL_MACHINE};

    // DEVICEMAP\VIDEO is volatile: it only lists adapters active in the current boot.
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let Ok(video_map) = hklm.open_subkey(r"HARDWARE\DEVICEMAP\VIDEO") else {
        return Vec::new();
    };

    let names = video_map
        .enum_values()
        .flatten()
        .filter(|(name, _)| name.starts_with(r"\Device\Video"))
        .filter_map(|(name, _)| video_map.get_value::<String, _>(name).ok())
        // Values look like "\Registry\Machine\System\CurrentControlSet\Control\Video\{GUID}\0000".
        .filter_map(|path| {
            const PREFIX: &str = r"\registry\machine\";
            path.get(..PREFIX.len())
                .filter(|head| head.eq_ignore_ascii_case(PREFIX))
                .map(|_| path[PREFIX.len()..].to_string())
        })
        .filter_map(|subkey| hklm.open_subkey(subkey).ok())
        .filter_map(|key| key.get_value::<String, _>("DriverDesc").ok());

    dedup(names)
}

#[cfg(target_os = "linux")]
pub fn gpu_names() -> Vec<String> {
    let names = lspci();
    if names.is_empty() {
        drm_vendors()
    } else {
        names
    }
}

/// `lspci -mm` lines: `00:02.0 "VGA compatible controller" "Intel Corporation" "UHD Graphics 620" ...`
#[cfg(target_os = "linux")]
fn lspci() -> Vec<String> {
    let Ok(output) = std::process::Command::new("lspci").arg("-mm").output() else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }

    let names = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let fields: Vec<&str> = line.split('"').collect();
            let (class, vendor, device) = (fields.get(1)?, fields.get(3)?, fields.get(5)?);
            let is_gpu = ["VGA", "3D", "Display"].iter().any(|k| class.contains(k));
            is_gpu.then(|| format!("{vendor} {device}"))
        })
        .collect::<Vec<_>>();
    dedup(names)
}

/// Fallback when `lspci` (pciutils) is not installed: vendor from the DRM sysfs entries.
#[cfg(target_os = "linux")]
fn drm_vendors() -> Vec<String> {
    let Ok(entries) = std::fs::read_dir("/sys/class/drm") else {
        return Vec::new();
    };

    let names = entries
        .flatten()
        .filter(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            name.starts_with("card") && !name.contains('-')
        })
        .filter_map(|entry| {
            let device = entry.path().join("device");
            let vendor = std::fs::read_to_string(device.join("vendor")).ok()?;
            let id = std::fs::read_to_string(device.join("device")).unwrap_or_default();
            let vendor = match vendor.trim() {
                "0x8086" => "Intel",
                "0x10de" => "NVIDIA",
                "0x1002" => "AMD",
                other => other,
            }
            .to_string();
            Some(format!("{vendor} GPU ({})", id.trim()))
        })
        .collect::<Vec<_>>();
    dedup(names)
}

#[cfg(target_os = "macos")]
pub fn gpu_names() -> Vec<String> {
    let Ok(output) = std::process::Command::new("system_profiler")
        .arg("SPDisplaysDataType")
        .output()
    else {
        return Vec::new();
    };

    let names = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.trim().strip_prefix("Chipset Model:"))
        .map(|name| name.trim().to_string())
        .collect::<Vec<_>>();
    dedup(names)
}

#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
pub fn gpu_names() -> Vec<String> {
    Vec::new()
}

/// Removes duplicates (one adapter can appear once per output) while keeping order.
#[allow(dead_code)]
fn dedup(names: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut unique: Vec<String> = Vec::new();
    for name in names {
        let name = name.trim().to_string();
        if !name.is_empty() && !unique.contains(&name) {
            unique.push(name);
        }
    }
    unique
}
