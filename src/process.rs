use sysinfo::System;

#[derive(Debug, Clone)]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub user: String,
    pub cpu: f32,
    pub memory: u64,
    pub status: String,
}

#[derive(Debug, Clone, Copy)]
pub enum ProcessSort {
    Cpu,
    Memory,
    Name,
    Pid,
}

#[derive(Debug)]
pub struct ProcessManager {
    selected: usize,
    pub offset: usize,
    pub filter: String,
    pub sort: ProcessSort,
}

impl ProcessManager {
    pub fn new() -> Self {
        Self {
            selected: 0,
            offset: 0,
            filter: String::new(),
            sort: ProcessSort::Cpu,
        }
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn select(&mut self, index: usize, length: usize) {
        self.selected = index.min(length.saturating_sub(1));
    }

    pub fn move_by(&mut self, delta: isize, length: usize) {
        self.selected = self
            .selected
            .saturating_add_signed(delta)
            .min(length.saturating_sub(1));
    }

    pub fn next_sort(&mut self) {
        self.sort = match self.sort {
            ProcessSort::Cpu => ProcessSort::Memory,
            ProcessSort::Memory => ProcessSort::Name,
            ProcessSort::Name => ProcessSort::Pid,
            ProcessSort::Pid => ProcessSort::Cpu,
        };
    }

    pub fn reload(&mut self, system: &mut System) -> Vec<ProcessInfo> {
        use sysinfo::ProcessesToUpdate;
        system.refresh_processes(ProcessesToUpdate::All, true);
        let mut processes: Vec<_> = system
            .processes()
            .values()
            .map(|process| ProcessInfo {
                pid: process.pid().as_u32(),
                name: process.name().to_string_lossy().into_owned(),
                user: process
                    .user_id()
                    .map(|uid| uid.to_string())
                    .unwrap_or_else(|| "—".to_owned()),
                cpu: process.cpu_usage(),
                memory: process.memory(),
                status: process.status().to_string(),
            })
            .filter(|process| {
                self.filter.is_empty()
                    || process
                        .name
                        .to_lowercase()
                        .contains(&self.filter.to_lowercase())
                    || process.pid.to_string().contains(&self.filter)
            })
            .collect();
        processes.sort_by(|a, b| match self.sort {
            ProcessSort::Cpu => b.cpu.total_cmp(&a.cpu),
            ProcessSort::Memory => b.memory.cmp(&a.memory),
            ProcessSort::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            ProcessSort::Pid => a.pid.cmp(&b.pid),
        });
        self.selected = self.selected.min(processes.len().saturating_sub(1));
        processes
    }

    pub fn terminate(system: &System, pid: u32) -> Result<(), String> {
        let process = system
            .process(sysinfo::Pid::from_u32(pid))
            .ok_or_else(|| "进程已经结束".to_owned())?;
        if process.kill_with(sysinfo::Signal::Term).unwrap_or(false) {
            Ok(())
        } else {
            Err("无法发送终止信号，请检查进程权限".to_owned())
        }
    }
}
