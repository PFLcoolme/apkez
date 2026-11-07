use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use crate::files::FileManager;
use crate::logs::{LogSource, LogViewer};
use crate::palette::{self, CommandAction};
use crate::process::{ProcessInfo, ProcessManager};
use crate::services::{ServiceAction, ServiceManager};
use crate::system::SystemInfo;
use crate::workspace::TerminalTabs;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Overview,
    Files,
    Processes,
    Services,
    Logs,
    Terminal,
    Settings,
}

impl Page {
    pub const ALL: [Page; 7] = [
        Self::Overview,
        Self::Files,
        Self::Processes,
        Self::Services,
        Self::Logs,
        Self::Terminal,
        Self::Settings,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Self::Overview => "系统概览",
            Self::Files => "文件管理",
            Self::Processes => "进程管理",
            Self::Services => "服务管理",
            Self::Logs => "日志查看",
            Self::Terminal => "终端",
            Self::Settings => "偏好设置",
        }
    }

    pub fn subtitle(self) -> &'static str {
        match self {
            Self::Overview => "一目了然，掌控你的服务器",
            Self::Files => "轻量、直观的本地文件工作区",
            Self::Processes => "观察系统负载，管理运行中的任务",
            Self::Services => "查看服务状态，按需通过 sudo 管理",
            Self::Logs => "跟随系统日志，快速定位问题",
            Self::Terminal => "多个交互式 Shell，共享一个工作区",
            Self::Settings => "打造属于你的终端空间",
        }
    }
}

#[derive(Debug, Clone)]
pub enum InputPurpose {
    NewFile,
    NewDirectory,
    Rename,
    FilterFiles,
    FilterProcesses,
    FilterServices,
    FilterLogs,
    LogPath,
}

#[derive(Debug, Clone)]
pub enum Modal {
    Help,
    Palette {
        query: String,
        selected: usize,
    },
    Input {
        title: String,
        value: String,
        purpose: InputPurpose,
    },
    ConfirmDelete,
    ConfirmKill(u32, String),
    ConfirmService(String, ServiceAction),
    Preview {
        name: String,
        content: String,
        offset: usize,
    },
}

#[derive(Debug, Clone, Copy)]
pub enum ClickAction {
    Navigate(Page),
    Help,
    CloseModal,
    Palette,
    PaletteRow(usize),
    FileRow(usize),
    FileParent,
    NewFile,
    NewDirectory,
    Rename,
    Delete,
    Hidden,
    ProcessRow(usize),
    Kill,
    Sort,
    TerminalTab(usize),
    NewTerminal,
    CloseTerminal,
    RestartShell,
    ServiceRow(usize),
    ServiceAction(ServiceAction),
    RefreshServices,
    OpenLogPath,
    ToggleLogFollow,
    RefreshLogs,
    ToggleMotion,
    Refresh,
}

pub struct App {
    pub running: bool,
    pub page: Page,
    pub system: SystemInfo,
    pub files: FileManager,
    pub processes: ProcessManager,
    pub process_rows: Vec<ProcessInfo>,
    pub terminals: TerminalTabs,
    pub services: ServiceManager,
    pub logs: LogViewer,
    pub modal: Option<Modal>,
    pub toast: Option<(String, Instant, bool)>,
    pub refresh_interval: Duration,
    last_refresh: Instant,
    last_click: Option<(Instant, usize)>,
    pub hits: Vec<(Rect, ClickAction)>,
    pub terminal_area: Rect,
}

impl App {
    pub fn new(root: PathBuf, refresh_interval: Duration) -> Result<Self> {
        let files = FileManager::new(root)?;
        let terminals = TerminalTabs::new(files.cwd().to_path_buf())?;
        let mut system = SystemInfo::new();
        let mut processes = ProcessManager::new();
        let process_rows = processes.reload(&mut system.system);
        let mut logs = LogViewer::new();
        logs.start_journal(None, false);
        Ok(Self {
            running: true,
            page: Page::Overview,
            system,
            files,
            processes,
            process_rows,
            terminals,
            services: ServiceManager::new(),
            logs,
            modal: None,
            toast: None,
            refresh_interval,
            last_refresh: Instant::now(),
            last_click: None,
            hits: Vec::new(),
            terminal_area: Rect::default(),
        })
    }

    pub fn tick(&mut self) {
        self.terminals.poll();
        if let Some(Err(error)) = self.services.poll() {
            self.notify(error, true);
        }
        self.logs.poll();
        if self.last_refresh.elapsed() >= self.refresh_interval {
            self.system.refresh();
            if self.page == Page::Processes {
                self.process_rows = self.processes.reload(&mut self.system.system);
            }
            self.last_refresh = Instant::now();
        }
        if self
            .toast
            .as_ref()
            .is_some_and(|(_, time, _)| time.elapsed() > Duration::from_secs(4))
        {
            self.toast = None;
        }
    }

    fn notify(&mut self, message: impl Into<String>, error: bool) {
        self.toast = Some((message.into(), Instant::now(), error));
    }

    fn result(&mut self, result: Result<()>) {
        if let Err(error) = result {
            self.notify(error.to_string(), true);
        }
    }

    pub fn navigate(&mut self, page: Page) {
        self.page = page;
        match page {
            Page::Processes => self.process_rows = self.processes.reload(&mut self.system.system),
            Page::Services if self.services.services.is_empty() && !self.services.loading => {
                self.services.reload()
            }
            _ => {}
        }
    }

    fn input(&mut self, title: &str, value: String, purpose: InputPurpose) {
        self.modal = Some(Modal::Input {
            title: title.to_owned(),
            value,
            purpose,
        });
    }

    fn input_char(&mut self, character: char) {
        match &mut self.modal {
            Some(Modal::Input { value, .. }) => value.push(character),
            Some(Modal::Palette { query, selected }) => {
                query.push(character);
                *selected = 0;
            }
            _ => {}
        }
    }

    fn input_backspace(&mut self) {
        match &mut self.modal {
            Some(Modal::Input { value, .. }) => {
                value.pop();
            }
            Some(Modal::Palette { query, selected }) => {
                query.pop();
                *selected = 0;
            }
            _ => {}
        }
    }

    pub fn handle_paste(&mut self, text: String) {
        if self.modal.is_some() {
            let single_line = text.replace(['\r', '\n'], "");
            match &mut self.modal {
                Some(Modal::Input { value, .. }) => value.push_str(&single_line),
                Some(Modal::Palette { query, selected }) => {
                    query.push_str(&single_line);
                    *selected = 0;
                }
                _ => {}
            }
            return;
        }
        if self.page == Page::Terminal {
            let result = self.terminals.paste(&text);
            self.result(result);
        }
    }

    fn submit_input(&mut self) {
        let Some(Modal::Input { value, purpose, .. }) = self.modal.take() else {
            return;
        };
        match purpose {
            InputPurpose::NewFile => {
                let result = self.files.create_file(value.trim()).map(|_| ());
                if result.is_ok() {
                    self.notify("文件已创建", false);
                }
                self.result(result);
            }
            InputPurpose::NewDirectory => {
                let result = self.files.create_directory(value.trim()).map(|_| ());
                if result.is_ok() {
                    self.notify("目录已创建", false);
                }
                self.result(result);
            }
            InputPurpose::Rename => {
                let result = self.files.rename_selected(value.trim());
                if result.is_ok() {
                    self.notify("已重命名", false);
                }
                self.result(result);
            }
            InputPurpose::FilterFiles => {
                self.files.filter = value.trim().to_owned();
                self.files.selected = 0;
                let result = self.files.reload();
                self.result(result);
            }
            InputPurpose::FilterProcesses => {
                self.processes.filter = value.trim().to_owned();
                self.process_rows = self.processes.reload(&mut self.system.system);
            }
            InputPurpose::FilterServices => {
                self.services.filter = value.trim().to_owned();
                self.services.selected = 0;
            }
            InputPurpose::FilterLogs => {
                self.logs.filter = value.trim().to_owned();
                self.logs.follow_end();
            }
            InputPurpose::LogPath => {
                let path = PathBuf::from(value.trim());
                self.logs.start_file(path, true);
            }
        }
    }

    fn open_selected(&mut self) {
        let directory = self
            .files
            .selected_entry()
            .is_some_and(|entry| entry.directory);
        if directory {
            let result = self.files.enter_selected();
            self.result(result);
        } else if let Some(entry) = self.files.selected_entry() {
            let name = entry.name.clone();
            match self.files.read_selected() {
                Ok(content) => {
                    self.modal = Some(Modal::Preview {
                        name,
                        content,
                        offset: 0,
                    })
                }
                Err(error) => self.notify(error.to_string(), true),
            }
        }
    }

    fn kill_selected(&mut self) {
        if let Some(process) = self.process_rows.get(self.processes.selected()) {
            self.modal = Some(Modal::ConfirmKill(process.pid, process.name.clone()));
        }
    }

    fn service_action(&mut self, action: ServiceAction) {
        if let Some(service) = self.services.selected() {
            self.modal = Some(Modal::ConfirmService(service.name.clone(), action));
        }
    }

    fn run_service_command(&mut self, name: String, action: ServiceAction) {
        let command = ServiceManager::command(&name, action, false);
        self.navigate(Page::Terminal);
        let result = self.terminals.send_command(&command);
        if result.is_ok() {
            self.notify(format!("已在终端执行 {command}，请按系统提示操作"), false);
        }
        self.result(result);
    }

    fn show_service_logs(&mut self) {
        if let Some(service) = self.services.selected() {
            self.logs.start_journal(Some(&service.name), true);
            self.navigate(Page::Logs);
        }
    }

    fn modal_key(&mut self, key: KeyEvent) {
        if key.code == KeyCode::Esc {
            self.modal = None;
            return;
        }
        let Some(modal) = self.modal.clone() else {
            return;
        };
        match modal {
            Modal::Palette { ref query, .. } => {
                let results = palette::search(query);
                match key.code {
                    KeyCode::Up => {
                        if let Some(Modal::Palette { selected, .. }) = &mut self.modal {
                            *selected = selected.saturating_sub(1);
                        }
                    }
                    KeyCode::Down => {
                        if let Some(Modal::Palette { selected, .. }) = &mut self.modal {
                            *selected = (*selected + 1).min(results.len().saturating_sub(1));
                        }
                    }
                    KeyCode::Enter => {
                        let selected = match &self.modal {
                            Some(Modal::Palette { selected, .. }) => *selected,
                            _ => 0,
                        };
                        if let Some(command) = results.get(selected) {
                            let action = command.action;
                            self.modal = None;
                            self.execute_command(action);
                        }
                    }
                    KeyCode::Backspace => self.input_backspace(),
                    KeyCode::Char(character) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                        self.input_char(character)
                    }
                    _ => {}
                }
            }
            Modal::Input { .. } => match key.code {
                KeyCode::Enter => self.submit_input(),
                KeyCode::Backspace => self.input_backspace(),
                KeyCode::Char(character) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.input_char(character)
                }
                _ => {}
            },
            Modal::ConfirmDelete if matches!(key.code, KeyCode::Char('y') | KeyCode::Enter) => {
                self.modal = None;
                let result = self.files.delete_selected();
                if result.is_ok() {
                    self.notify("已删除", false);
                }
                self.result(result);
            }
            Modal::ConfirmKill(pid, _)
                if matches!(key.code, KeyCode::Char('y') | KeyCode::Enter) =>
            {
                self.modal = None;
                match ProcessManager::terminate(&self.system.system, pid) {
                    Ok(()) => {
                        self.notify(format!("已向进程 {pid} 发送终止信号"), false);
                        self.process_rows = self.processes.reload(&mut self.system.system);
                    }
                    Err(error) => self.notify(error, true),
                }
            }
            Modal::ConfirmService(name, action)
                if matches!(key.code, KeyCode::Char('y') | KeyCode::Enter) =>
            {
                self.modal = None;
                self.run_service_command(name, action);
            }
            Modal::Preview { .. } => {
                if let Some(Modal::Preview { offset, .. }) = &mut self.modal {
                    match key.code {
                        KeyCode::Up => *offset = offset.saturating_sub(1),
                        KeyCode::Down => *offset = offset.saturating_add(1),
                        KeyCode::PageUp => *offset = offset.saturating_sub(10),
                        KeyCode::PageDown => *offset = offset.saturating_add(10),
                        _ => {}
                    }
                }
            }
            Modal::ConfirmDelete | Modal::ConfirmKill(_, _) | Modal::ConfirmService(_, _)
                if key.code == KeyCode::Char('n') =>
            {
                self.modal = None
            }
            _ => {}
        }
    }

    fn execute_command(&mut self, action: CommandAction) {
        match action {
            CommandAction::Navigate(page) => self.navigate(page),
            CommandAction::NewTerminal => self.new_terminal(),
            CommandAction::CloseTerminal => self.terminals.close(),
            CommandAction::NextTerminal => self.terminals.next(),
            CommandAction::PreviousTerminal => self.terminals.previous(),
            CommandAction::RefreshServices => self.services.reload(),
            CommandAction::ToggleMotion => self.system.reduced_motion = !self.system.reduced_motion,
            CommandAction::Help => self.modal = Some(Modal::Help),
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('q') {
            self.running = false;
            return;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('k') {
            self.modal = Some(Modal::Palette {
                query: String::new(),
                selected: 0,
            });
            return;
        }
        if self.modal.is_some() {
            self.modal_key(key);
            return;
        }
        match key.code {
            KeyCode::F(1) => return self.navigate(Page::Overview),
            KeyCode::F(2) => return self.navigate(Page::Files),
            KeyCode::F(3) => return self.navigate(Page::Processes),
            KeyCode::F(4) => return self.navigate(Page::Services),
            KeyCode::F(5) => return self.navigate(Page::Logs),
            KeyCode::F(6) => return self.navigate(Page::Terminal),
            KeyCode::F(7) => return self.navigate(Page::Settings),
            KeyCode::Char('?') if self.page != Page::Terminal => {
                self.modal = Some(Modal::Help);
                return;
            }
            _ => {}
        }
        match self.page {
            Page::Overview => match key.code {
                KeyCode::Tab | KeyCode::Right => self.navigate(Page::Files),
                KeyCode::Enter => self.navigate(Page::Files),
                _ => {}
            },
            Page::Files => self.files_key(key),
            Page::Processes => self.processes_key(key),
            Page::Services => self.services_key(key),
            Page::Logs => self.logs_key(key),
            Page::Terminal => self.terminal_key(key),
            Page::Settings => match key.code {
                KeyCode::Char('m') => self.system.reduced_motion = !self.system.reduced_motion,
                KeyCode::Char('r') => {
                    self.system.refresh();
                    self.notify("系统信息已刷新", false);
                }
                _ => {}
            },
        }
    }

    fn files_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.files.previous(),
            KeyCode::Down | KeyCode::Char('j') => self.files.next(),
            KeyCode::PageUp => self.files.move_by(-10),
            KeyCode::PageDown => self.files.move_by(10),
            KeyCode::Home => self.files.select(0),
            KeyCode::End => self.files.page(usize::MAX),
            KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => self.open_selected(),
            KeyCode::Backspace | KeyCode::Left | KeyCode::Char('h') => {
                let result = self.files.up();
                self.result(result);
            }
            KeyCode::Char('n') => self.input("创建文件", String::new(), InputPurpose::NewFile),
            KeyCode::Char('N') => self.input("创建目录", String::new(), InputPurpose::NewDirectory),
            KeyCode::Char('r') => {
                if let Some(entry) = self.files.selected_entry() {
                    self.input("重命名", entry.name.clone(), InputPurpose::Rename);
                }
            }
            KeyCode::Char('d') | KeyCode::Delete => {
                if self.files.selected_entry().is_some() {
                    self.modal = Some(Modal::ConfirmDelete);
                }
            }
            KeyCode::Char('.') => {
                let result = self.files.toggle_hidden();
                self.result(result);
            }
            KeyCode::Char('/') => self.input(
                "筛选文件",
                self.files.filter.clone(),
                InputPurpose::FilterFiles,
            ),
            KeyCode::Esc => {
                self.files.filter.clear();
                let result = self.files.reload();
                self.result(result);
            }
            _ => {}
        }
    }

    fn processes_key(&mut self, key: KeyEvent) {
        let length = self.process_rows.len();
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.processes.move_by(-1, length),
            KeyCode::Down | KeyCode::Char('j') => self.processes.move_by(1, length),
            KeyCode::PageUp => self.processes.move_by(-10, length),
            KeyCode::PageDown => self.processes.move_by(10, length),
            KeyCode::Home => self.processes.select(0, length),
            KeyCode::End => self.processes.select(usize::MAX, length),
            KeyCode::Char('s') => {
                self.processes.next_sort();
                self.process_rows = self.processes.reload(&mut self.system.system);
            }
            KeyCode::Char('/') => self.input(
                "筛选进程",
                self.processes.filter.clone(),
                InputPurpose::FilterProcesses,
            ),
            KeyCode::Char('x') | KeyCode::Delete => self.kill_selected(),
            KeyCode::Esc => {
                self.processes.filter.clear();
                self.process_rows = self.processes.reload(&mut self.system.system);
            }
            _ => {}
        }
    }

    fn services_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.services.move_by(-1),
            KeyCode::Down | KeyCode::Char('j') => self.services.move_by(1),
            KeyCode::PageUp => self.services.move_by(-10),
            KeyCode::PageDown => self.services.move_by(10),
            KeyCode::Home => self.services.select(0),
            KeyCode::End => self.services.select(usize::MAX),
            KeyCode::Char('s') => self.service_action(ServiceAction::Start),
            KeyCode::Char('x') => self.service_action(ServiceAction::Stop),
            KeyCode::Char('r') => self.service_action(ServiceAction::Restart),
            KeyCode::Char('R') => self.services.reload(),
            KeyCode::Char('l') | KeyCode::Enter => self.show_service_logs(),
            KeyCode::Char('/') => self.input(
                "筛选服务",
                self.services.filter.clone(),
                InputPurpose::FilterServices,
            ),
            KeyCode::Esc => {
                self.services.filter.clear();
                self.services.selected = 0;
            }
            _ => {}
        }
    }

    fn logs_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.logs.scroll(-1),
            KeyCode::Down | KeyCode::Char('j') => self.logs.scroll(1),
            KeyCode::PageUp => self.logs.scroll(-10),
            KeyCode::PageDown => self.logs.scroll(10),
            KeyCode::Home => self.logs.scroll(isize::MIN),
            KeyCode::End => self.logs.follow_end(),
            KeyCode::Char('/') => self.input(
                "筛选日志",
                self.logs.filter.clone(),
                InputPurpose::FilterLogs,
            ),
            KeyCode::Char('o') => self.input("打开日志文件", String::new(), InputPurpose::LogPath),
            KeyCode::Char('J') => self.logs.start_journal(None, true),
            KeyCode::Char('f') => {
                if self.logs.source == LogSource::Journal {
                    let service = self.logs.service.clone();
                    self.logs.start_journal(
                        (!service.is_empty()).then_some(service.as_str()),
                        !self.logs.following,
                    );
                } else {
                    self.logs.following = !self.logs.following;
                }
            }
            KeyCode::Esc => {
                self.logs.filter.clear();
            }
            _ => {}
        }
    }

    fn terminal_key(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char(']') => return self.navigate(Page::Overview),
                KeyCode::Char('t') => return self.new_terminal(),
                KeyCode::Char('w') if self.terminals.len() > 1 => {
                    self.terminals.close();
                    return;
                }
                KeyCode::Char('n') if self.terminals.len() > 1 => {
                    self.terminals.next();
                    return;
                }
                KeyCode::Char('p') if self.terminals.len() > 1 => {
                    self.terminals.previous();
                    return;
                }
                _ => {}
            }
        }
        let result = self.terminals.active_mut().key(key);
        self.result(result);
    }

    fn new_terminal(&mut self) {
        let result = self.terminals.create();
        if result.is_ok() {
            self.navigate(Page::Terminal);
        }
        self.result(result);
    }

    pub fn handle_mouse(&mut self, mouse: MouseEvent) {
        if self.modal.is_none()
            && self.page == Page::Terminal
            && self
                .terminal_area
                .contains((mouse.column, mouse.row).into())
        {
            match self.terminals.active_mut().mouse(mouse, self.terminal_area) {
                Ok(true) => return,
                Ok(false) => {}
                Err(error) => {
                    self.notify(error.to_string(), true);
                    return;
                }
            }
        }
        match mouse.kind {
            MouseEventKind::ScrollUp => {
                self.scroll(-3);
                return;
            }
            MouseEventKind::ScrollDown => {
                self.scroll(3);
                return;
            }
            MouseEventKind::Down(MouseButton::Left) => {}
            _ => return,
        }
        if self.modal.is_some() {
            let action = self
                .hits
                .iter()
                .find(|(area, _)| area.contains((mouse.column, mouse.row).into()))
                .map(|(_, action)| *action);
            match action {
                Some(action @ ClickAction::PaletteRow(_)) => self.click(action),
                Some(ClickAction::CloseModal) => self.modal = None,
                _ => {}
            }
            return;
        }
        let Some((_, action)) = self
            .hits
            .iter()
            .find(|(area, _)| area.contains((mouse.column, mouse.row).into()))
            .copied()
        else {
            return;
        };
        self.click(action);
    }

    fn click(&mut self, action: ClickAction) {
        match action {
            ClickAction::Navigate(page) => self.navigate(page),
            ClickAction::Help => self.modal = Some(Modal::Help),
            ClickAction::CloseModal => self.modal = None,
            ClickAction::Palette => {
                self.modal = Some(Modal::Palette {
                    query: String::new(),
                    selected: 0,
                })
            }
            ClickAction::PaletteRow(index) => {
                let query = match &self.modal {
                    Some(Modal::Palette { query, .. }) => query.clone(),
                    _ => return,
                };
                if let Some(command) = palette::search(&query).get(index) {
                    let action = command.action;
                    self.modal = None;
                    self.execute_command(action);
                }
            }
            ClickAction::FileRow(index) => {
                let double_click = self.last_click.is_some_and(|(time, previous)| {
                    previous == index && time.elapsed() < Duration::from_millis(500)
                });
                self.files.select(index);
                if double_click {
                    self.open_selected();
                    self.last_click = None;
                } else {
                    self.last_click = Some((Instant::now(), index));
                }
            }
            ClickAction::FileParent => {
                let result = self.files.up();
                self.result(result);
            }
            ClickAction::NewFile => self.input("创建文件", String::new(), InputPurpose::NewFile),
            ClickAction::NewDirectory => {
                self.input("创建目录", String::new(), InputPurpose::NewDirectory)
            }
            ClickAction::Rename => {
                if let Some(entry) = self.files.selected_entry() {
                    self.input("重命名", entry.name.clone(), InputPurpose::Rename);
                }
            }
            ClickAction::Delete => {
                if self.files.selected_entry().is_some() {
                    self.modal = Some(Modal::ConfirmDelete)
                }
            }
            ClickAction::Hidden => {
                let result = self.files.toggle_hidden();
                self.result(result);
            }
            ClickAction::ProcessRow(index) => self.processes.select(index, self.process_rows.len()),
            ClickAction::Kill => self.kill_selected(),
            ClickAction::Sort => {
                self.processes.next_sort();
                self.process_rows = self.processes.reload(&mut self.system.system);
            }
            ClickAction::TerminalTab(index) => self.terminals.select(index),
            ClickAction::NewTerminal => self.new_terminal(),
            ClickAction::CloseTerminal => self.terminals.close(),
            ClickAction::RestartShell => {
                let result = self
                    .terminals
                    .active_mut()
                    .key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
                self.result(result);
            }
            ClickAction::ServiceRow(index) => self.services.select(index),
            ClickAction::ServiceAction(action) => self.service_action(action),
            ClickAction::RefreshServices => self.services.reload(),
            ClickAction::OpenLogPath => {
                self.input("打开日志文件", String::new(), InputPurpose::LogPath)
            }
            ClickAction::ToggleLogFollow => self.logs.toggle_follow(),
            ClickAction::RefreshLogs => self.logs.refresh(),
            ClickAction::ToggleMotion => self.system.reduced_motion = !self.system.reduced_motion,
            ClickAction::Refresh => {
                self.system.refresh();
                self.notify("系统信息已刷新", false);
            }
        }
    }

    fn scroll(&mut self, amount: isize) {
        if let Some(Modal::Preview { offset, .. }) = &mut self.modal {
            *offset = offset.saturating_add_signed(amount);
            return;
        }
        match self.page {
            Page::Files => self.files.move_by(amount),
            Page::Processes => self.processes.move_by(amount, self.process_rows.len()),
            Page::Services => self.services.move_by(amount),
            Page::Logs => self.logs.scroll(amount),
            Page::Terminal => self.terminals.active_mut().scroll(amount),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{App, Modal, Page};
    use crate::test_support::{Workspace, duration};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    #[test]
    fn navigates_pages_and_creates_file_with_confirmation_flow() {
        let workspace = Workspace::new("app");
        let mut app = App::new(workspace.path(), duration()).unwrap();
        app.handle_key(KeyEvent::new(KeyCode::F(2), KeyModifiers::NONE));
        assert_eq!(app.page, Page::Files);
        app.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(app.files.entries()[0].name, "a");
        app.handle_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE));
        assert!(matches!(app.modal, Some(Modal::ConfirmDelete)));
        app.handle_key(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE));
        assert!(app.files.entries().is_empty());
    }

    #[test]
    fn quit_shortcut_works_from_modal() {
        let workspace = Workspace::new("quit");
        let mut app = App::new(workspace.path(), duration()).unwrap();
        app.handle_key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL));
        assert!(!app.running);
    }

    #[test]
    fn creates_and_switches_terminal_tabs() {
        let workspace = Workspace::new("tabs");
        let mut app = App::new(workspace.path(), duration()).unwrap();
        app.navigate(Page::Terminal);
        app.handle_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::CONTROL));
        assert_eq!(app.terminals.len(), 2);
        assert_eq!(app.terminals.index(), 1);
        app.handle_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL));
        assert_eq!(app.terminals.index(), 0);
        app.handle_key(KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL));
        assert_eq!(app.terminals.len(), 1);
    }

    #[test]
    fn command_palette_opens_and_filters() {
        let workspace = Workspace::new("palette");
        let mut app = App::new(workspace.path(), duration()).unwrap();
        app.handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL));
        for character in "服务".chars() {
            app.handle_key(KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE));
        }
        assert!(matches!(app.modal, Some(Modal::Palette { ref query, .. }) if query == "服务"));
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(app.page, Page::Services);
    }
}
