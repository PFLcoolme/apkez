# apkez

`apkez` 是一个纯终端的 Linux 服务器工作区。它不是替代 Linux 内核的可启动操作系统，而是运行在 Linux 用户空间中的终端桌面层：通过 SSH 登录即可使用，不依赖 X11、Wayland、浏览器或 GPU。

界面采用黑白灰配色、留白和反白选中项，使用 Rust 与终端字符绘制。鼠标支持取决于终端客户端；键盘始终可用。

## 功能

- 系统概览：CPU、内存、存储、交换区、网络累计流量、负载和运行时间
- 文件管理：目录浏览、文本预览、筛选、隐藏文件、新建、重命名和二次确认删除
- 进程管理：按 CPU、内存、名称或 PID 排序，筛选和二次确认终止
- 交互式终端：真正的 PTY Shell，支持 `vim`、`top`、`ssh`、方向键、控制键和动态窗口大小
- 设置：低带宽动态曲线开关
- 全部页面支持 F1–F5、鼠标点击和鼠标滚轮

## 本地运行

需要 Rust 1.88 或更新版本：

```bash
cargo run -- --root "$HOME"
```

参数：

```text
--root <目录>   文件工作区根目录，默认为当前目录
--tick <秒>     系统信息刷新间隔，范围 1–60，默认为 2
-h, --help      显示帮助
-V, --version   显示版本
```

常用操作：

- `F1` 概览、`F2` 文件、`F3` 进程、`F4` 终端、`F5` 设置
- `?` 打开帮助，`Ctrl+Q` 退出
- 文件页：`Enter` 打开，`n/N` 新建文件/目录，`r` 重命名，`d` 删除，`/` 筛选
- 进程页：`s` 切换排序，`x` 终止选中进程，`/` 筛选
- 终端页：普通按键和 `Esc` 均交给 Shell，`Ctrl+]` 返回桌面，Shell 退出后按 `Enter` 重启

## 部署到服务器

最简单的方式是在目标服务器编译并通过 SSH 使用：

```bash
ssh user@server
# 在服务器上复制项目后：
cargo build --release
./target/release/apkez --root "$HOME"
```

也可以在本机编译后复制单个二进制文件：

```bash
cargo build --release
scp target/release/apkez user@server:~/.local/bin/apkez
ssh -t user@server '~/.local/bin/apkez --root "$HOME"'
```

`ssh -t` 会分配伪终端，鼠标事件是否可用由 SSH 客户端和终端决定。程序退出后不会修改服务器的登录 shell，也不会接管系统启动流程。

安装到当前用户的 `~/.local/bin`：

```bash
./install.sh
# 或安装到其他目录：
PREFIX=/usr/local sudo ./install.sh
```

也可以使用容器部署：

```bash
docker build -t apkez .
docker run --rm -it -v "$PWD:/workspace" apkez
```

交互式程序需要真实 TTY，不适合作为没有终端的后台 systemd 服务运行。

## 验证

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
python3 tests/tty_smoke.py
python3 tests/terminal_smoke.py
```

测试覆盖文件边界、文本预览、键盘交互和五个页面的离屏渲染。两个伪终端脚本检查启动、页面切换、鼠标捕获和退出恢复，以及 Shell 命令执行、`vim` 编辑保存和会话目录保持。

## 安全边界

文件管理操作默认限制在 `--root` 之内，并阻止指向根目录外的符号链接预览。终端会启动当前用户的 `$SHELL`，没有 `$SHELL` 时使用 `/bin/sh`。Shell 与 `vim`、`ssh` 等程序拥有当前用户的系统权限，不受文件管理根目录限制，也不会自动提权；请把它当作一个有交互界面的远程服务器工具，而不是安全沙箱。
