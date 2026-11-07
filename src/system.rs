use std::collections::VecDeque;

use sysinfo::{Disks, Networks, System};

#[derive(Debug)]
pub struct SystemInfo {
    pub system: System,
    disks: Disks,
    networks: Networks,
    pub hostname: String,
    pub kernel: String,
    pub os: String,
    pub uptime: u64,
    pub cpu: f32,
    pub memory_used: u64,
    pub memory_total: u64,
    pub swap_used: u64,
    pub swap_total: u64,
    pub disk_used: u64,
    pub disk_total: u64,
    pub load: (f64, f64, f64),
    pub cpu_count: usize,
    pub network_rx: u64,
    pub network_tx: u64,
    pub history: VecDeque<f32>,
    pub reduced_motion: bool,
}

impl SystemInfo {
    pub fn new() -> Self {
        let mut system = System::new_all();
        system.refresh_all();
        let disks = Disks::new_with_refreshed_list();
        let disk = disks
            .iter()
            .filter(|disk| disk.total_space() > 0)
            .max_by_key(|disk| disk.mount_point().as_os_str().len());
        let (disk_used, disk_total) = disk
            .map(|disk| {
                (
                    disk.total_space().saturating_sub(disk.available_space()),
                    disk.total_space(),
                )
            })
            .unwrap_or_default();
        let mut history = VecDeque::with_capacity(80);
        history.push_back(system.global_cpu_usage());
        let networks = Networks::new_with_refreshed_list();
        let network_rx = networks
            .values()
            .map(|network| network.total_received())
            .sum();
        let network_tx = networks
            .values()
            .map(|network| network.total_transmitted())
            .sum();
        Self {
            hostname: System::host_name().unwrap_or_else(|| "localhost".to_owned()),
            kernel: System::kernel_version().unwrap_or_else(|| "Linux".to_owned()),
            os: System::long_os_version().unwrap_or_else(|| "Linux".to_owned()),
            uptime: System::uptime(),
            cpu: system.global_cpu_usage(),
            memory_used: system.used_memory(),
            memory_total: system.total_memory(),
            swap_used: system.used_swap(),
            swap_total: system.total_swap(),
            disk_used,
            disk_total,
            load: {
                let load = System::load_average();
                (load.one, load.five, load.fifteen)
            },
            cpu_count: system.cpus().len(),
            network_rx,
            network_tx,
            disks,
            networks,
            history,
            system,
            reduced_motion: false,
        }
    }

    pub fn refresh(&mut self) {
        self.system.refresh_cpu_usage();
        self.system.refresh_memory();
        self.cpu = self.system.global_cpu_usage();
        self.memory_used = self.system.used_memory();
        self.memory_total = self.system.total_memory();
        self.swap_used = self.system.used_swap();
        self.swap_total = self.system.total_swap();
        self.uptime = System::uptime();
        let load = System::load_average();
        self.load = (load.one, load.five, load.fifteen);
        self.disks.refresh(true);
        let disk = self
            .disks
            .iter()
            .filter(|disk| disk.total_space() > 0)
            .max_by_key(|disk| disk.mount_point().as_os_str().len());
        if let Some(disk) = disk {
            self.disk_used = disk.total_space().saturating_sub(disk.available_space());
            self.disk_total = disk.total_space();
        }
        self.networks.refresh(true);
        self.network_rx = self
            .networks
            .values()
            .map(|network| network.total_received())
            .sum();
        self.network_tx = self
            .networks
            .values()
            .map(|network| network.total_transmitted())
            .sum();
        if !self.reduced_motion {
            if self.history.len() == 80 {
                self.history.pop_front();
            }
            self.history.push_back(self.cpu);
        }
    }

    pub fn memory_percent(&self) -> f32 {
        percentage(self.memory_used, self.memory_total)
    }

    pub fn disk_percent(&self) -> f32 {
        percentage(self.disk_used, self.disk_total)
    }
}

pub fn percentage(value: u64, total: u64) -> f32 {
    if total == 0 {
        0.0
    } else {
        (value as f64 * 100.0 / total as f64).clamp(0.0, 100.0) as f32
    }
}

pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}

pub fn format_duration(seconds: u64) -> String {
    let days = seconds / 86_400;
    let hours = seconds % 86_400 / 3600;
    let minutes = seconds % 3600 / 60;
    if days > 0 {
        format!("{days} 天 {hours} 小时")
    } else {
        format!("{hours} 小时 {minutes} 分钟")
    }
}

#[cfg(test)]
mod tests {
    use super::{format_bytes, format_duration, percentage};

    #[test]
    fn formats_system_measurements() {
        assert_eq!(format_bytes(0), "0.0 B");
        assert_eq!(format_bytes(1024), "1.0 KiB");
        assert_eq!(format_bytes(1536), "1.5 KiB");
        assert_eq!(format_duration(90_061), "1 天 1 小时");
        assert_eq!(percentage(1, 2), 50.0);
        assert_eq!(percentage(1, 0), 0.0);
    }
}
