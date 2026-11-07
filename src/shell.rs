use std::io::Write;
use std::num::NonZeroU16;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::thread;

use anyhow::{Context, Result};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

pub struct TerminalSession {
    parser: vt100::Parser,
    pub cwd: PathBuf,
    master: Box<dyn MasterPty>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn Child>,
    receiver: Receiver<Vec<u8>>,
    alive: bool,
    restart: bool,
    rows: u16,
    cols: u16,
}

impl TerminalSession {
    pub fn new(cwd: PathBuf) -> Result<Self> {
        Self::with_size(cwd, 30, 100)
    }

    fn with_size(cwd: PathBuf, rows: u16, cols: u16) -> Result<Self> {
        let rows = rows.max(3);
        let cols = cols.max(20);
        let pair = native_pty_system().openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_owned());
        let mut command = CommandBuilder::new(shell);
        command.cwd(&cwd);
        command.env("TERM", "xterm-256color");
        command.env("COLORTERM", "truecolor");
        let child = pair
            .slave
            .spawn_command(command)
            .context("无法启动交互式 Shell")?;
        drop(pair.slave);
        let mut reader = pair.master.try_clone_reader()?;
        let writer = pair.master.take_writer()?;
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let mut buffer = [0u8; 16 * 1024];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) | Err(_) => break,
                    Ok(count) => {
                        if sender.send(buffer[..count].to_vec()).is_err() {
                            break;
                        }
                    }
                }
            }
        });
        let mut parser = vt100::Parser::new(
            NonZeroU16::new(rows).unwrap(),
            NonZeroU16::new(cols).unwrap(),
            1000,
        );
        parser.process(b"\x1b[2J\x1b[H");
        Ok(Self {
            parser,
            cwd,
            master: pair.master,
            writer,
            child,
            receiver,
            alive: true,
            restart: false,
            rows,
            cols,
        })
    }

    pub fn poll(&mut self) -> Result<()> {
        while let Ok(bytes) = self.receiver.try_recv() {
            self.parser.process(&bytes);
        }
        if self.alive {
            if let Some(status) = self.child.try_wait()? {
                self.alive = false;
                self.parser
                    .process(format!("\r\n[Shell 已退出: {status}]\r\n").as_bytes());
            }
        } else if self.restart {
            self.restart = false;
            self.restart_shell()?;
        }
        Ok(())
    }

    pub fn resize(&mut self, rows: u16, cols: u16) -> Result<()> {
        let rows = rows.max(3);
        let cols = cols.max(20);
        if self.rows == rows && self.cols == cols {
            return Ok(());
        }
        self.rows = rows;
        self.cols = cols;
        self.master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;
        self.parser.set_size(
            NonZeroU16::new(rows).unwrap(),
            NonZeroU16::new(cols).unwrap(),
        );
        Ok(())
    }

    pub fn write_bytes(&mut self, bytes: &[u8]) -> Result<()> {
        self.writer.write_all(bytes)?;
        self.writer.flush()?;
        if self.alive && bytes == b"\r" {
            let last = self.parser.screen().rows(0, 0).last().unwrap_or_default();
            if let Some(path) = last.strip_prefix("__APKEZ_PWD__") {
                let path = path.trim();
                if std::path::Path::new(path).is_dir() {
                    self.cwd = PathBuf::from(path);
                }
            }
        }
        Ok(())
    }

    pub fn paste(&mut self, text: &str) -> Result<()> {
        if !self.alive || text.is_empty() {
            return Ok(());
        }
        self.write_bytes(text.as_bytes())
    }

    pub fn report_error(&mut self, error: String) {
        self.parser
            .process(format!("\r\n[终端错误: {error}]\r\n").as_bytes());
    }

    pub fn scroll(&mut self, rows: isize) {
        let screen = self.parser.screen_mut();
        let current = screen.scrollback() as isize;
        screen.set_scrollback(current.saturating_add(rows).max(0) as usize);
    }

    pub fn key(&mut self, key: KeyEvent) -> Result<()> {
        if !self.alive {
            if key.code == KeyCode::Enter {
                self.restart = true;
            }
            return Ok(());
        }
        let application = self.parser.screen().application_cursor();
        let modifiers = key.modifiers;
        let control = modifiers.contains(KeyModifiers::CONTROL);
        let alt = modifiers.contains(KeyModifiers::ALT);
        let mut bytes = match key.code {
            KeyCode::Enter => b"\r".to_vec(),
            KeyCode::Tab if modifiers.contains(KeyModifiers::SHIFT) => b"\x1b[Z".to_vec(),
            KeyCode::Tab => b"\t".to_vec(),
            KeyCode::BackTab => b"\x1b[Z".to_vec(),
            KeyCode::Backspace => b"\x7f".to_vec(),
            KeyCode::Esc => b"\x1b".to_vec(),
            KeyCode::Up => Self::arrow(b'A', application, modifiers),
            KeyCode::Down => Self::arrow(b'B', application, modifiers),
            KeyCode::Right => Self::arrow(b'C', application, modifiers),
            KeyCode::Left => Self::arrow(b'D', application, modifiers),
            KeyCode::Home => b"\x1b[H".to_vec(),
            KeyCode::End => b"\x1b[F".to_vec(),
            KeyCode::PageUp => b"\x1b[5~".to_vec(),
            KeyCode::PageDown => b"\x1b[6~".to_vec(),
            KeyCode::Insert => b"\x1b[2~".to_vec(),
            KeyCode::Delete => b"\x1b[3~".to_vec(),
            KeyCode::F(number @ 1..=4) => vec![27, b'O', b'P' + number - 1],
            KeyCode::F(number @ 5..=12) => {
                let suffix = [15, 17, 18, 19, 20, 21, 23, 24][number as usize - 5];
                format!("\x1b[{suffix}~").into_bytes()
            }
            KeyCode::Char(character) if control => {
                let control_byte = match character {
                    'a'..='z' => Some(character as u8 - b'a' + 1),
                    'A'..='Z' => Some(character as u8 - b'A' + 1),
                    ' ' | '@' => Some(0),
                    '[' => Some(27),
                    '\\' => Some(28),
                    ']' => Some(29),
                    '^' => Some(30),
                    '_' => Some(31),
                    '?' => Some(127),
                    _ => None,
                };
                control_byte.map_or_else(Vec::new, |byte| vec![byte])
            }
            KeyCode::Char(character) => {
                let mut buffer = [0u8; 4];
                character.encode_utf8(&mut buffer).as_bytes().to_vec()
            }
            _ => Vec::new(),
        };
        if alt && !bytes.is_empty() {
            bytes.insert(0, 27);
        }
        self.write_bytes(&bytes)
    }

    fn arrow(code: u8, application: bool, modifiers: KeyModifiers) -> Vec<u8> {
        if modifiers.contains(KeyModifiers::CONTROL) {
            vec![27, b'[', b'1', b';', b'5', code]
        } else if application {
            vec![27, b'O', code]
        } else {
            vec![27, b'[', code]
        }
    }

    pub fn mouse(&mut self, event: MouseEvent, area: Rect) -> Result<bool> {
        let mouse_enabled =
            self.parser.screen().mouse_protocol_mode() != vt100::MouseProtocolMode::None;
        if self.alive
            && !mouse_enabled
            && matches!(
                event.kind,
                MouseEventKind::ScrollUp | MouseEventKind::ScrollDown
            )
        {
            let rows = self.parser.screen().scrollback_len();
            if rows > 0 {
                let offset = self.parser.screen().scrollback();
                let delta = if matches!(event.kind, MouseEventKind::ScrollUp) {
                    3
                } else {
                    -3
                };
                let next = (offset as isize + delta).clamp(0, rows as isize) as usize;
                self.parser.screen_mut().set_scrollback(next);
                return Ok(true);
            }
        }
        if !self.alive || !mouse_enabled {
            return Ok(false);
        }
        let column = event.column.saturating_sub(area.x) + 1;
        let row = event.row.saturating_sub(area.y) + 1;
        if column > area.width || row > area.height {
            return Ok(false);
        }
        let button = match event.kind {
            MouseEventKind::Down(crossterm::event::MouseButton::Left) => 0,
            MouseEventKind::Down(crossterm::event::MouseButton::Middle) => 1,
            MouseEventKind::Down(crossterm::event::MouseButton::Right) => 2,
            MouseEventKind::Up(_) => 0,
            MouseEventKind::ScrollUp => 64,
            MouseEventKind::ScrollDown => 65,
            _ => return Ok(false),
        };
        let release = matches!(event.kind, MouseEventKind::Up(_));
        if self.parser.screen().mouse_protocol_encoding() == vt100::MouseProtocolEncoding::Default
            && (column > 223 || row > 223)
        {
            return Ok(true);
        }
        self.write_bytes(
            format!(
                "\x1b[<{button};{column};{row}{}",
                if release { 'm' } else { 'M' }
            )
            .as_bytes(),
        )?;
        Ok(true)
    }

    pub fn lines(&self) -> Vec<Line<'static>> {
        let (rows, cols) = self.parser.screen().size();
        (0..rows.get())
            .map(|row| {
                let mut spans = Vec::new();
                let mut text = String::new();
                let mut style = Style::default();
                for col in 0..cols.get() {
                    let Some(cell) = self.parser.screen().cell(row, col) else {
                        continue;
                    };
                    if cell.is_wide_continuation() {
                        continue;
                    }
                    let next = Self::style(cell);
                    if next != style && !text.is_empty() {
                        spans.push(Span::styled(std::mem::take(&mut text), style));
                        style = next;
                    }
                    if cell.contents().is_empty() {
                        text.push(' ');
                    } else {
                        text.push_str(cell.contents());
                    }
                }
                if !text.is_empty() {
                    spans.push(Span::styled(text, style));
                }
                Line::from(spans)
            })
            .collect()
    }

    fn style(cell: &vt100::Cell) -> Style {
        let mut style = Style::default()
            .fg(Self::color(cell.fgcolor(), true))
            .bg(Self::color(cell.bgcolor(), false));
        if cell.bold() {
            style = style.add_modifier(Modifier::BOLD);
        }
        if cell.italic() {
            style = style.add_modifier(Modifier::ITALIC);
        }
        if cell.underline() {
            style = style.add_modifier(Modifier::UNDERLINED);
        }
        if cell.inverse() {
            style = style.add_modifier(Modifier::REVERSED);
        }
        style
    }

    fn color(color: vt100::Color, foreground: bool) -> Color {
        match color {
            vt100::Color::Default if foreground => Color::Rgb(215, 215, 215),
            vt100::Color::Default => Color::Rgb(17, 17, 17),
            vt100::Color::Idx(index) => Color::Indexed(index),
            vt100::Color::Rgb(red, green, blue) => Color::Rgb(red, green, blue),
        }
    }

    pub fn cursor(&self) -> Option<(u16, u16)> {
        if self.parser.screen().hide_cursor() {
            None
        } else {
            let (row, col) = self.parser.screen().cursor_position();
            Some((col, row))
        }
    }

    pub fn is_alive(&self) -> bool {
        self.alive
    }

    fn restart_shell(&mut self) -> Result<()> {
        let current_dir = self.cwd.clone();
        let rows = self.rows;
        let cols = self.cols;
        *self = Self::with_size(current_dir, rows, cols)?;
        Ok(())
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        if self.alive {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}
