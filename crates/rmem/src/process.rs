use std::collections::HashMap;

use sysinfo::{
    ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind, Users,
};

#[derive(Debug, Clone)]
pub struct ProcessInfo {
    pub pid: u32,
    pub user: String,
    pub cpu_percent: f32,
    pub memory: u64,
    pub memory_percent: Option<f64>,
    pub memory_delta: i64,
    pub cpu_time_ms: u64,
    pub elapsed_seconds: u64,
    pub command: String,
    pub name: String,
    pub status: String,
}

pub struct MemorySummary {
    pub total_memory: u64,
    pub used_memory: u64,
    pub total_swap: u64,
    pub used_swap: u64,
}

pub struct ProcessCollector {
    system: System,
    users: Users,
    previous_memory: HashMap<u32, u64>,
}

impl ProcessCollector {
    pub fn new() -> Self {
        Self {
            system: System::new_all(),
            users: Users::new_with_refreshed_list(),
            previous_memory: HashMap::new(),
        }
    }

    pub fn memory_summary(&self) -> MemorySummary {
        MemorySummary {
            total_memory: self.system.total_memory(),
            used_memory: self.system.used_memory(),
            total_swap: self.system.total_swap(),
            used_swap: self.system.used_swap(),
        }
    }

    pub fn refresh(&mut self) -> Vec<ProcessInfo> {
        self.system.refresh_memory();
        self.system.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing()
                .with_memory()
                .with_cpu()
                .with_user(UpdateKind::OnlyIfNotSet)
                .with_cmd(UpdateKind::OnlyIfNotSet)
                .with_exe(UpdateKind::OnlyIfNotSet)
                .without_tasks(),
        );

        let total_memory = self.system.total_memory();
        let mut next_memory = HashMap::with_capacity(self.system.processes().len());

        let processes = self
            .system
            .processes()
            .values()
            .map(|process| {
                let pid = process.pid().as_u32();
                let memory = process.memory();
                let old_memory = self.previous_memory.get(&pid).copied().unwrap_or(memory);
                let memory_delta = delta(memory, old_memory);

                next_memory.insert(pid, memory);

                let user = process
                    .user_id()
                    .and_then(|uid| self.users.get_user_by_id(uid))
                    .map(|user| user.name().to_string())
                    .unwrap_or_else(|| "-".to_string());

                let name = process.name().to_string_lossy().into_owned();
                let command = command_line(process, &name);

                let memory_percent = (total_memory != 0)
                    .then(|| memory as f64 / total_memory as f64 * 100.0);

                ProcessInfo {
                    pid,
                    user,
                    cpu_percent: process.cpu_usage(),
                    memory,
                    memory_percent,
                    memory_delta,
                    cpu_time_ms: process.accumulated_cpu_time(),
                    elapsed_seconds: process.run_time(),
                    command,
                    name,
                    status: format!("{:?}", process.status()),
                }
            })
            .collect();

        self.previous_memory = next_memory;
        processes
    }
}

fn command_line(process: &sysinfo::Process, fallback_name: &str) -> String {
    let command = process
        .cmd()
        .iter()
        .map(|part| part.to_string_lossy())
        .collect::<Vec<_>>()
        .join(" ");

    let command = sanitize(&command);
    if !command.is_empty() {
        return command;
    }

    if let Some(exe) = process.exe() {
        let exe = sanitize(&exe.to_string_lossy());
        if !exe.is_empty() {
            return exe;
        }
    }

    fallback_name.to_string()
}

fn sanitize(value: &str) -> String {
    value
        .chars()
        .map(|ch| match ch {
            '\n' | '\r' | '\t' => ' ',
            ch if ch.is_control() => ' ',
            ch => ch,
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::ProcessCollector;

    #[test]
    fn refresh_reports_the_current_process() {
        let mut collector = ProcessCollector::new();
        let processes = collector.refresh();
        let current = processes
            .iter()
            .find(|process| process.pid == std::process::id())
            .expect("the current process should be visible");
        assert!(current.memory > 0);
        assert!(current.memory_percent.is_some_and(|value| value > 0.0));
        assert_eq!(current.memory_delta, 0);
        assert!(!current.command.is_empty());
    }
}

fn delta(new: u64, old: u64) -> i64 {
    if new >= old {
        (new - old).min(i64::MAX as u64) as i64
    } else {
        -((old - new).min(i64::MAX as u64) as i64)
    }
}
