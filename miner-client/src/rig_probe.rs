use std::process::Command;

pub fn detect_nvidia_devices() -> Vec<String> {
    let output = Command::new("nvidia-smi")
        .args(["--query-gpu=index,name", "--format=csv,noheader"])
        .output();

    let Ok(output) = output else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| line.to_string())
        .collect()
}

pub fn summarize_nvidia_devices(devices: &[String]) -> String {
    let mut ordered: Vec<(String, usize)> = Vec::new();

    for raw in devices {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }

        let name = trimmed
            .split_once(',')
            .map(|(_, device_name)| device_name.trim())
            .unwrap_or(trimmed);

        if let Some((_, count)) = ordered.iter_mut().find(|(existing, _)| existing == name) {
            *count += 1;
        } else {
            ordered.push((name.to_string(), 1));
        }
    }

    ordered
        .into_iter()
        .map(|(name, count)| format!("{count}x {name}"))
        .collect::<Vec<_>>()
        .join(" | ")
}
