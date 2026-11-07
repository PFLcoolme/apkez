use std::sync::mpsc::{self, Receiver};
use std::thread;

#[derive(Debug, Clone)]
pub struct Service {
    pub name: String,
    pub active: String,
    pub sub: String,
    pub description: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceAction {
    Start,
    Stop,
    Restart,
}

impl ServiceAction {
    pub fn command(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Stop => "stop",
            Self::Restart => "restart",
        }
    }
}

pub struct ServiceManager {
    pub services: Vec<Service>,
    pub selected: usize,
    pub offset: usize,
    pub filter: String,
    receiver: Option<Receiver<Result<Vec<Service>, String>>>,
    pub loading: bool,
}

impl ServiceManager {
    pub fn new() -> Self {
        Self {
            services: Vec::new(),
            selected: 0,
            offset: 0,
            filter: String::new(),
            receiver: None,
            loading: false,
        }
    }

    pub fn reload(&mut self) {
        let (sender, receiver) = mpsc::channel();
        self.receiver = Some(receiver);
        self.loading = true;
        thread::spawn(move || {
            let result = list_services();
            let _ = sender.send(result);
        });
    }

    pub fn poll(&mut self) -> Option<Result<(), String>> {
        let Some(receiver) = &self.receiver else {
            return None;
        };
        match receiver.try_recv() {
            Ok(result) => {
                self.loading = false;
                self.receiver = None;
                match result {
                    Ok(services) => {
                        self.services = services;
                        self.selected = self.selected.min(self.services.len().saturating_sub(1));
                        Some(Ok(()))
                    }
                    Err(error) => Some(Err(error)),
                }
            }
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                self.loading = false;
                self.receiver = None;
                Some(Err("服务列表读取中断".to_owned()))
            }
        }
    }

    pub fn visible(&self) -> Vec<usize> {
        let query = self.filter.to_lowercase();
        self.services
            .iter()
            .enumerate()
            .filter(|(_, service)| {
                query.is_empty()
                    || service.name.to_lowercase().contains(&query)
                    || service.description.to_lowercase().contains(&query)
            })
            .map(|(index, _)| index)
            .collect()
    }

    pub fn selected(&self) -> Option<&Service> {
        self.visible()
            .get(self.selected)
            .and_then(|index| self.services.get(*index))
    }

    pub fn move_by(&mut self, amount: isize) {
        let length = self.visible().len();
        self.selected = self
            .selected
            .saturating_add_signed(amount)
            .min(length.saturating_sub(1));
    }

    pub fn select(&mut self, index: usize) {
        self.selected = index.min(self.visible().len().saturating_sub(1));
    }

    pub fn command(service: &str, action: ServiceAction, user: bool) -> String {
        let scope = if user { "--user " } else { "" };
        let elevation = if user { "" } else { "sudo " };
        format!(
            "{elevation}systemctl {scope}{} -- {}",
            action.command(),
            shell_quote(service)
        )
    }
}

fn list_services() -> Result<Vec<Service>, String> {
    let output = std::process::Command::new("systemctl")
        .args([
            "list-units",
            "--type=service",
            "--all",
            "--no-pager",
            "--no-legend",
            "--plain",
            "--full",
        ])
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|error| format!("无法运行 systemctl: {error}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned());
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let services = parse_services(&stdout);
    Ok(services)
}

fn parse_services(output: &str) -> Vec<Service> {
    output
        .lines()
        .filter_map(|line| {
            let line = line.trim_start_matches(['●', '×', '*', ' ']);
            let mut fields = line.split_whitespace();
            let name = fields.next()?;
            if !name.ends_with(".service") {
                return None;
            }
            fields.next()?;
            Some(Service {
                name: name.to_owned(),
                active: fields.next().unwrap_or("—").to_owned(),
                sub: fields.next().unwrap_or("—").to_owned(),
                description: fields.collect::<Vec<_>>().join(" "),
            })
        })
        .collect()
}

pub fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::{ServiceManager, parse_services, shell_quote};

    #[test]
    fn parses_systemctl_units() {
        let services = parse_services(
            "ssh.service loaded active running OpenSSH server\n● failed.service loaded failed failed Broken service\n",
        );
        assert_eq!(services.len(), 2);
        assert_eq!(services[0].name, "ssh.service");
        assert_eq!(services[1].active, "failed");
        assert_eq!(services[1].description, "Broken service");
    }

    #[test]
    fn commands_are_shell_quoted_and_use_sudo() {
        let command = ServiceManager::command("a;bad", super::ServiceAction::Restart, false);
        assert_eq!(command, "sudo systemctl restart -- 'a;bad'");
        assert_eq!(shell_quote("a'b"), "'a'\\''b'");
    }
}
