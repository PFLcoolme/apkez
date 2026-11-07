use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum LogSource {
    Journal,
    File,
}

struct LogWorker {
    sender: Sender<LogEvent>,
    file: Option<File>,
    reader: Option<LogReader>,
    child: Option<std::process::Child>,
}

enum LogReader {
    File(BufReader<File>),
    Child(BufReader<std::process::ChildStdout>),
}

impl LogReader {
    fn read_line(&mut self, line: &mut String) -> std::io::Result<usize> {
        match self {
            Self::File(reader) => reader.read_line(line),
            Self::Child(reader) => reader.read_line(line),
        }
    }

    fn stream_position(&mut self) -> Option<u64> {
        match self {
            Self::File(reader) => reader.stream_position().ok(),
            Self::Child(_) => None,
        }
    }

    fn rewind(&mut self) {
        if let Self::File(reader) = self {
            reader.seek(SeekFrom::Start(0)).ok();
        }
    }
}

enum LogEvent {
    Line(String),
    Error(String),
}

impl LogWorker {
    fn read_next(&mut self) -> Option<usize> {
        let reader = self.reader.as_mut()?;
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) => Some(0),
            Ok(count) => {
                self.sender
                    .send(LogEvent::Line(
                        line.trim_end_matches(['\r', '\n']).to_owned(),
                    ))
                    .ok()?;
                Some(count)
            }
            Err(error) => {
                self.sender.send(LogEvent::Error(error.to_string())).ok()?;
                None
            }
        }
    }

    fn trim_file(&mut self) {
        let Some(position) = self.reader.as_mut().and_then(LogReader::stream_position) else {
            return;
        };
        let Some(file) = &self.file else { return };
        if file
            .metadata()
            .is_ok_and(|metadata| metadata.len() < position)
            && let Some(reader) = self.reader.as_mut()
        {
            reader.rewind();
        }
    }
}

pub struct LogViewer {
    pub source: LogSource,
    pub service: String,
    pub path: Option<std::path::PathBuf>,
    pub lines: Vec<String>,
    pub offset: usize,
    pub filter: String,
    pub following: bool,
    worker: Option<std::thread::JoinHandle<()>>,
    receiver: Option<Receiver<LogEvent>>,
    stop_sender: Option<Sender<()>>,
    pub error: Option<String>,
}

impl LogViewer {
    pub fn new() -> Self {
        Self {
            source: LogSource::Journal,
            service: String::new(),
            path: None,
            lines: Vec::new(),
            offset: 0,
            filter: String::new(),
            following: true,
            worker: None,
            receiver: None,
            stop_sender: None,
            error: None,
        }
    }

    pub fn start_journal(&mut self, service: Option<&str>, follow: bool) {
        self.stop();
        self.lines.clear();
        self.offset = 0;
        self.error = None;
        self.service = service.unwrap_or("").to_owned();
        self.source = LogSource::Journal;
        self.following = follow;
        let mut args = vec![
            "-n".to_owned(),
            "300".to_owned(),
            "--no-pager".to_owned(),
            "-o".to_owned(),
            "short-iso".to_owned(),
        ];
        if !self.service.is_empty() {
            args.extend(["-u".to_owned(), self.service.clone()]);
        }
        if follow {
            args.push("-f".to_owned());
        }
        self.spawn_journal(args);
    }

    pub fn start_file(&mut self, path: std::path::PathBuf, follow: bool) {
        self.stop();
        self.lines.clear();
        self.offset = 0;
        self.error = None;
        self.service.clear();
        self.path = Some(path);
        self.source = LogSource::File;
        self.following = follow;
        self.spawn_file();
    }

    fn spawn_journal(&mut self, args: Vec<String>) {
        let (sender, receiver) = mpsc::channel();
        let (stop_sender, stop_receiver) = mpsc::channel();
        self.receiver = Some(receiver);
        self.stop_sender = Some(stop_sender);
        self.worker = Some(thread::spawn(move || {
            let mut command = std::process::Command::new("journalctl");
            command
                .args(&args)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped());
            let mut child = match command.spawn() {
                Ok(child) => child,
                Err(error) => {
                    sender.send(LogEvent::Error(error.to_string())).ok();
                    return;
                }
            };
            if let Some(stdout) = child.stdout.take() {
                let mut worker = LogWorker {
                    sender: sender.clone(),
                    file: None,
                    reader: Some(LogReader::Child(BufReader::new(stdout))),
                    child: Some(child),
                };
                while stop_receiver.try_recv().is_err() {
                    match worker.read_next() {
                        Some(0) => thread::sleep(Duration::from_millis(100)),
                        Some(_) => {}
                        None => break,
                    }
                }
                if let Some(mut child) = worker.child {
                    let _ = child.kill();
                    let _ = child.wait();
                }
            }
        }));
    }

    fn spawn_file(&mut self) {
        let Some(path) = self.path.clone() else {
            return;
        };
        let (sender, receiver) = mpsc::channel();
        let (stop_sender, stop_receiver) = mpsc::channel();
        self.receiver = Some(receiver);
        self.stop_sender = Some(stop_sender);
        self.worker = Some(thread::spawn(move || {
            let file = match File::open(&path) {
                Ok(file) => file,
                Err(error) => {
                    sender.send(LogEvent::Error(error.to_string())).ok();
                    return;
                }
            };
            let reader = file
                .try_clone()
                .ok()
                .map(|file| LogReader::File(BufReader::new(file)));
            let mut worker = LogWorker {
                sender,
                file: Some(file),
                reader,
                child: None,
            };
            while stop_receiver.try_recv().is_err() {
                match worker.read_next() {
                    Some(0) => {
                        worker.trim_file();
                        thread::sleep(Duration::from_millis(500));
                    }
                    Some(_) => {}
                    None => break,
                }
            }
        }));
    }

    pub fn poll(&mut self) {
        let Some(receiver) = &self.receiver else {
            return;
        };
        while let Ok(event) = receiver.try_recv() {
            match event {
                LogEvent::Line(line) => self.lines.push(line),
                LogEvent::Error(error) => self.error = Some(error),
            }
        }
        if self.lines.len() > 5000 {
            self.lines.drain(..self.lines.len() - 5000);
        }
    }

    pub fn visible(&self) -> Vec<&str> {
        let query = self.filter.to_lowercase();
        self.lines
            .iter()
            .filter(|line| query.is_empty() || line.to_lowercase().contains(&query))
            .map(String::as_str)
            .collect()
    }

    pub fn scroll(&mut self, amount: isize) {
        let length = self.visible().len();
        self.offset = self
            .offset
            .saturating_add_signed(amount)
            .min(length.saturating_sub(1));
        self.following = false;
    }

    pub fn follow_end(&mut self) {
        let length = self.visible().len();
        self.offset = length.saturating_sub(1);
        self.following = true;
    }

    pub fn toggle_follow(&mut self) {
        if self.following {
            self.following = false;
        } else {
            self.follow_end();
        }
    }

    pub fn refresh(&mut self) {
        match self.source {
            LogSource::Journal => {
                let service = self.service.clone();
                self.start_journal((!service.is_empty()).then_some(service.as_str()), true);
            }
            LogSource::File => {
                if let Some(path) = self.path.clone() {
                    self.start_file(path, true);
                }
            }
        }
    }

    pub fn stop(&mut self) {
        if let Some(sender) = self.stop_sender.take() {
            sender.send(()).ok();
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        self.receiver = None;
        self.following = false;
    }
}

impl Drop for LogViewer {
    fn drop(&mut self) {
        self.stop();
    }
}
