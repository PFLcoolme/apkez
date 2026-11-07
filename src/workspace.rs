use anyhow::Result;

use crate::shell::TerminalSession;

pub struct TerminalTabs {
    sessions: Vec<TerminalSession>,
    active: usize,
}

impl TerminalTabs {
    pub fn new(cwd: std::path::PathBuf) -> Result<Self> {
        Ok(Self {
            sessions: vec![TerminalSession::new(cwd)?],
            active: 0,
        })
    }

    pub fn active(&self) -> &TerminalSession {
        &self.sessions[self.active]
    }

    pub fn active_mut(&mut self) -> &mut TerminalSession {
        &mut self.sessions[self.active]
    }

    pub fn len(&self) -> usize {
        self.sessions.len()
    }

    pub fn index(&self) -> usize {
        self.active
    }

    pub fn poll(&mut self) {
        for session in &mut self.sessions {
            if let Err(error) = session.poll() {
                session.report_error(error.to_string());
            }
        }
    }

    pub fn create(&mut self) -> Result<()> {
        if self.sessions.len() >= 8 {
            anyhow::bail!("最多支持 8 个终端标签");
        }
        let cwd = self.active().cwd.clone();
        self.sessions.push(TerminalSession::new(cwd)?);
        self.active = self.sessions.len() - 1;
        Ok(())
    }

    pub fn close(&mut self) {
        if self.sessions.len() <= 1 {
            self.active_mut().write_bytes(b"exit\r").ok();
            return;
        }
        self.sessions.remove(self.active);
        self.active = self.active.min(self.sessions.len() - 1);
    }

    pub fn next(&mut self) {
        self.active = (self.active + 1) % self.sessions.len();
    }

    pub fn previous(&mut self) {
        self.active = if self.active == 0 {
            self.sessions.len() - 1
        } else {
            self.active - 1
        };
    }

    pub fn select(&mut self, index: usize) {
        if index < self.sessions.len() {
            self.active = index;
        }
    }

    pub fn send_command(&mut self, command: &str) -> Result<()> {
        self.active_mut().write_bytes(command.as_bytes())?;
        self.active_mut().write_bytes(b"\r")
    }

    pub fn paste(&mut self, text: &str) -> Result<()> {
        self.active_mut().paste(text)
    }
}
