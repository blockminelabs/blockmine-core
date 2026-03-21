use anyhow::Result;

use crate::engine::gpu;

pub fn run() -> Result<()> {
    let devices = gpu::list_devices()?;

    if devices.is_empty() {
        println!("No OpenCL devices found.");
        return Ok(());
    }

    for device in devices {
        println!("platform_index={}", device.platform_index);
        println!("device_index={}", device.device_index);
        println!("platform_name={}", device.platform_name);
        println!("device_name={}", device.device_name);
        println!("vendor={}", device.vendor);
        println!("version={}", device.version);
        println!("global_memory_bytes={}", device.global_memory_bytes);
        println!("max_compute_units={}", device.max_compute_units);
        println!("max_work_group_size={}", device.max_work_group_size);
        println!();
    }

    Ok(())
}

