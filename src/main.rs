mod app;
mod files;
mod logs;
mod palette;
mod process;
mod services;
mod shell;
mod system;
#[cfg(test)]
mod test_support;
mod ui;
mod workspace;

use std::env;
use std::io::{self, Stdout};
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use app::App;
use crossterm::event::{
    self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
    Event,
};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

struct TerminalGuard;

impl TerminalGuard {
    fn start() -> Result<Self> {
        enable_raw_mode().context("无法启用终端输入模式")?;
        execute!(
            io::stdout(),
            EnterAlternateScreen,
            EnableMouseCapture,
            EnableBracketedPaste
        )
        .context("无法初始化终端显示")?;
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(
            io::stdout(),
            LeaveAlternateScreen,
            DisableMouseCapture,
            DisableBracketedPaste
        );
    }
}

fn parse_args() -> Result<Option<(PathBuf, Duration)>> {
    let mut root = env::current_dir().context("无法获取当前目录")?;
    let mut refresh = Duration::from_secs(2);
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                println!(
                    "apkez — 为 Linux 服务器设计的终端工作区\n\n用法: apkez [选项]\n\n  --root <目录>   设置文件管理的根目录\n  --tick <秒>     系统信息刷新间隔，1–60 秒\n  -h, --help      显示帮助\n  -V, --version   显示版本\n\n快捷键: F1–F5 切换页面，? 帮助，Ctrl+Q 退出"
                );
                return Ok(None);
            }
            "-V" | "--version" => {
                println!("apkez {}", env!("CARGO_PKG_VERSION"));
                return Ok(None);
            }
            "--root" => {
                let Some(path) = args.next() else {
                    bail!("--root 需要目录参数")
                };
                root = PathBuf::from(path);
            }
            "--tick" => {
                let Some(value) = args.next() else {
                    bail!("--tick 需要秒数")
                };
                let seconds: u64 = value.parse().context("--tick 必须是整数")?;
                if !(1..=60).contains(&seconds) {
                    bail!("--tick 范围为 1–60 秒");
                }
                refresh = Duration::from_secs(seconds);
            }
            _ => bail!("未知参数: {arg}，使用 --help 查看帮助"),
        }
    }
    Ok(Some((root, refresh)))
}

fn run(terminal: &mut Terminal<CrosstermBackend<Stdout>>, app: &mut App) -> Result<()> {
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(
            io::stdout(),
            LeaveAlternateScreen,
            DisableMouseCapture,
            DisableBracketedPaste
        );
        original_hook(info);
    }));
    while app.running {
        terminal.draw(|frame| ui::draw(frame, app))?;
        if event::poll(Duration::from_millis(100)).context("读取终端事件失败")? {
            match event::read().context("读取终端事件失败")? {
                Event::Key(key) if key.kind != event::KeyEventKind::Release => app.handle_key(key),
                Event::Paste(text) => app.handle_paste(text),
                Event::Mouse(mouse) => app.handle_mouse(mouse),
                Event::Resize(_, _) => {}
                _ => {}
            }
        }
        app.tick();
    }
    Ok(())
}

fn main() -> Result<()> {
    let Some((root, refresh)) = parse_args()? else {
        return Ok(());
    };
    let mut app = App::new(root, refresh)?;
    let _guard = TerminalGuard::start()?;
    let mut terminal =
        Terminal::new(CrosstermBackend::new(io::stdout())).context("无法创建终端界面")?;
    let result = run(&mut terminal, &mut app);
    terminal.show_cursor().ok();
    result
}
