# 开发日志 · UI 重构与服务器日常功能

## 一、本次范围

在既有 UI 重构基础上，为服务器日常运维补齐四项能力：

| 功能 | 入口 | 说明 |
| --- | --- | --- |
| 多终端标签 | `F6` / `Ctrl+T` / 命令面板 | 同一工作区内并行运行多个 Shell，可切换、关闭 |
| 命令面板 | `Ctrl+K` | 模糊搜索页面、终端操作与常用命令，回车执行 |
| 服务管理 | `F4` | 列出 systemd 服务，启动/停止/重启，查看状态与日志 |
| 日志查看 | `F5` | journalctl 或日志文件，支持实时跟随、搜索/筛选 |

页面顺序调整为 `F1` 概览、`F2` 文件、`F3` 进程、`F4` 服务、`F5` 日志、`F6` 终端、`F7` 设置。

权限策略按需求执行：**所有需要 root 的操作都写入内置终端执行 `sudo`，密码由用户在终端中输入**，应用本身不收集、不缓存凭据。

## 二、实现依据（可溯源）

- `journalctl`：`-n <行数>`、`--no-pager`、`-o short-iso`、`-u <unit>`、`-f`，均取自 `man journalctl`。
- `systemctl`：`list-units --type=service --all --no-pager --no-legend --plain --full` 与 `start/stop/restart -- <unit>`，取自 `man systemctl`。
- 终端复用：沿用 `portable-pty` 的 `native_pty_system().openpty()`、`slave.spawn_command()`、`master.try_clone_reader()`，未自造 PTY 生命周期。
- 终端屏幕解析：`vt100::Parser` 的 `screen().contents()`、`cursor_position()`、`resize()`。
- 界面组件：`ratatui 0.30` 的 `Table`、`Block`、`Clear`、`Paragraph`、`Line`、`Layout`。

## 三、代码改动

### 1. 终端多标签

- 新增 `src/workspace.rs` 中的 `TerminalTabs`：
  - `sessions: Vec<TerminalSession>`、`active: usize`
  - `create/close/select/next/previous/active/active_mut/len/index/send_command`
- `App` 字段由单个 `terminal` 改为 `terminals: TerminalTabs`。
- 关闭最后一个标签时保留一个空会话，避免界面失去终端区域。
- 标签栏绘制于终端页第二行，左侧为编号胶囊，右侧固定显示 `+ Ctrl+T  × Ctrl+W`；标签过多时按可用宽度截断，不覆盖右侧控件。

### 2. 命令面板

- 新增 `src/palette.rs`：`Command { name, description, action }`、`CommandAction`、`all()`、`search()`。
- `Modal::Palette { query, selected }`，按键 ↑↓ 移动、Enter 执行、Esc 取消、Backspace 删除。
- 输入为空时显示占位提示“输入以搜索页面、操作或命令…”。
- 结果行与鼠标点击命中区一一对应（`ClickAction::PaletteRow(index)`）。
- 头部右上角提供 `Ctrl+K 命令` 入口，点击打开同一弹窗。

### 3. 服务管理

- 新增 `src/services.rs`：
  - `ServiceManager::{reload, poll, visible, selected, move_by, select, command}`
  - 后台线程执行 `systemctl list-units`，`mpsc` 回传，`poll()` 在 tick 中消费。
- 输出解析：`split_whitespace` 取 unit 名、load、active、sub，剩余字段合并为描述，兼容 `●`/`×`/`*` 前缀。
- 启动/停止/重启先弹确认框（`Modal::ConfirmService`），确认后：
  ```rust
  let command = ServiceManager::command(&name, action, false); // 生成 sudo systemctl <action> -- '<unit>'
  self.navigate(Page::Terminal);
  self.terminals.send_command(&command);
  ```
- 单元名经 `shell_quote` 单引号转义，防止命令注入。
- `Enter`/`l` 跳转日志页并 `journalctl -u <unit> -f`。

### 4. 日志查看

- 新增 `src/logs.rs`：`LogViewer` 管理 journalctl 子进程或文件读取线程。
- `LogReader` 枚举同时支持 `BufReader<File>` 与 `BufReader<ChildStdout>`，解决两种 reader 类型不匹配。
- 文件模式检测截断（`metadata.len() < stream_position()` 时 rewind），避免日志轮转后读到旧偏移。
- 实时跟随：journalctl 使用 `-f`；文件模式由 `following` 标记控制尾部滚动。
- 支持 `f` 跟随、`o` 打开日志文件、`J` 读取 journalctl、`/` 筛选、`Esc` 清除筛选。

### 5. 界面同步调整

- `page_index/page_meta/nav_title` 覆盖 7 个页面；日志页 meta 只显示行数，避免与“已暂停/实时跟随”胶囊重复。
- 文件页在宽度 ≥ 90 时拆分为列表 + 28 列详情面板（目录 / 选中 / 操作）。
- 服务页表格列宽 `[21, 30, Min(20)]`，状态列格式 `● active / running`，行点击按 `inner.y + 1 + offset` 命中。
- 帮助弹窗、页脚键帽提示、命令面板尺寸同步更新。

## 四、开发过程中遇到的问题与处理

| 问题 | 原因 | 处理 |
| --- | --- | --- |
| 新增页面后编译报 17 个错误 | 枚举未穷尽匹配、`ServiceAction` 缺 `Debug`、`terminal` 字段改名 | 逐项补全匹配分支与派生 |
| 日志线程 reader 类型冲突 | `File` 与 `ChildStdout` 的 `BufReader` 是不同类型 | 引入 `LogReader` 枚举统一调用 |
| 命令面板选中项无法更新 | 克隆 `Modal` 后写入无效 | 改为对 `&mut self.modal` 内的 `selected` 赋值 |
| 日志 `j` 键不可达 | 与滚动 `j` 重复 | journalctl 快捷键改为大写 `J` |
| 刷新日志借用冲突 | 同一表达式同时借用 `logs.service` 与 `logs` | 先克隆 service 再调用 |
| 服务解析描述为空 | `splitn` 产生空字段 | 改为剩余字段合并 |
| 终端标签右侧 `Ctrl+W` 被裁剪 | 控件区按 16 列预留 | 控件区宽度改为 24，命中区拆为 11/12 列 |
| 概览 CPU 图表看似渲染错误 | 采样不足且无坐标系 | 增加 100/50/0 网格线，占位文字下移到 1/4 高度 |
| 设置页大面积空白 | 面板高度固定且内容少 | 重排为三块面板并加入快捷操作区 |

## 五、验证结果

均使用本地离线依赖（`CARGO_HOME="$PWD/.cargo-home"`）：

- `cargo fmt --all`：通过。
- `cargo clippy --all-targets --offline -- -D warnings`：0 警告。
- `cargo test --offline`：16 passed。
- `cargo build --release --offline`：成功。
- `tests/tty_smoke.py`：7 个页面切换通过。
- `tests/terminal_smoke.py`：PTY 交互、vim 保存、目录保持、Ctrl+Q 退出通过。
- 界面截图复核：概览、文件、进程、服务、日志、终端、设置、命令面板布局正常；终端标签栏 `Ctrl+W` 完整显示。

## 六、兼容说明

- 最低界面尺寸仍为 64×18，不足时显示“扩大终端”提示。
- `F1–F7`、`Ctrl+Q`、`?` 帮助、鼠标点击、终端透传行为保留；`F6` 由原终端页顺延至新位置。
- 终端页内除 `Ctrl+T/W/N/P/]` 外，所有按键原样发送给 Shell。
- 依赖未新增，仍为 ratatui / crossterm / portable-pty / vt100 / sysinfo / unicode-width。

## 七、终端输入问题排查（补充）

### 现象

用户反馈终端页“回车后不执行 / 无输出”。

### 排查过程

用伪终端驱动真实二进制逐一复现：逐字输入、回车（CR）、LF、`seq 1 100` 长输出、空闲自动刷新、Ctrl+C、`sudo` 密码提示、80×20 与 120×40 两种窗口、关闭弹窗后再输入，均正常执行并显示结果。唯一复现的输入缺陷是**括号粘贴内容被丢弃**：

- 主循环未处理 `Event::Paste`；
- 程序也从未启用 `EnableBracketedPaste`；
- 终端若以括号粘贴序列（`\x1b[200~ … \x1b[201~`）送入文本，crossterm 会解析为 `Event::Paste`，而主循环直接忽略，导致粘贴的命令既不回显也不执行。

### 修复

- `main.rs`：启动时 `EnableBracketedPaste`，退出与 panic 钩子中 `DisableBracketedPaste`；主循环新增 `Event::Paste` 处理，并把 `KeyEventKind::Repeat` 与 `Press` 同等处理（未使用终端上报 Release 的兼容路径）。
- `app.rs`：新增 `handle_paste`，弹窗输入框与命令面板接收粘贴文本（换行会被去除）；终端页把文本写入当前会话。
- `workspace.rs` / `shell.rs`：新增 `TerminalTabs::paste` 与 `TerminalSession::paste`，仅在会话存活时写入。

### 验证

- `cargo fmt`、`cargo clippy -D warnings`、`cargo test`（16 passed）、`cargo build --release` 全部通过。
- 括号粘贴 `echo pasted-ok` + 回车：文本回显且输出 `pasted-ok`。
- 逐字输入 `echo typed-ok` + 回车、弹窗关闭后输入、`tty_smoke.py`、`terminal_smoke.py` 均通过。

## 八、遗留风险与未实现项

- systemd 操作依赖本机存在 `systemctl` 且用户具备 sudo；无 sudo 时终端会提示认证失败，应用只透传输出。
- journalctl 需要当前用户在 `systemd-journal` 组或具备读权限，否则无日志输出。
- 日志跟随期间切换来源会终止旧子进程；进程退出时可能存在短暂空窗。
- 未实现（未在本次需求中选中）：网络面板、内置文本编辑器、文件复制/移动/多选、磁盘分析、Docker 管理。
