use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Layout, Margin, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, BorderType, Clear, Padding, Paragraph, Row, Sparkline, Table, Widget, Wrap,
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::app::{App, ClickAction, Modal, Page};
use crate::logs::LogSource;
use crate::palette;
use crate::process::ProcessSort;
use crate::services::ServiceAction;
use crate::system::{format_bytes, format_duration};

const BG: Color = Color::Rgb(17, 17, 17);
const SURFACE: Color = Color::Rgb(29, 29, 29);
const SELECTED: Color = Color::Rgb(229, 229, 229);
const TEXT: Color = Color::Rgb(222, 222, 222);
const MUTED: Color = Color::Rgb(137, 137, 137);
const DIM: Color = Color::Rgb(82, 82, 82);
const BORDER: Color = Color::Rgb(58, 58, 58);
const TRACK: Color = Color::Rgb(48, 48, 48);
const ERROR: Color = Color::Rgb(255, 155, 155);
const CONTENT_MAX: u16 = 112;

fn muted(text: impl Into<String>) -> Span<'static> {
    Span::styled(text.into(), Style::default().fg(MUTED))
}

fn label(text: impl Into<String>) -> Span<'static> {
    Span::styled(text.into(), Style::default().fg(TEXT))
}

fn dim(text: impl Into<String>) -> Span<'static> {
    Span::styled(text.into(), Style::default().fg(DIM))
}

fn cap(text: &str) -> Span<'static> {
    Span::styled(format!(" {text} "), Style::default().fg(TEXT).bg(SURFACE))
}

fn bright_cap(text: &str) -> Span<'static> {
    Span::styled(
        format!(" {text} "),
        Style::default().fg(BG).bg(SELECTED).bold(),
    )
}

fn text_width(text: &str) -> u16 {
    UnicodeWidthStr::width(text) as u16
}

fn shorten(text: &str, max: u16) -> String {
    if text_width(text) <= max {
        return text.to_owned();
    }
    let limit = max.saturating_sub(1);
    let mut output = String::new();
    let mut used = 0;
    for character in text.chars() {
        let width = UnicodeWidthChar::width(character).unwrap_or(0) as u16;
        if used + width > limit {
            break;
        }
        output.push(character);
        used += width;
    }
    output.push('…');
    output
}

fn content_area(area: Rect) -> Rect {
    let width = area.width.saturating_sub(4).min(CONTENT_MAX);
    Rect::new(
        area.x + (area.width - width) / 2,
        area.y,
        width,
        area.height,
    )
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let vertical = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(height.min(area.height)),
        Constraint::Fill(1),
    ])
    .split(area);
    Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Length(width.min(area.width)),
        Constraint::Fill(1),
    ])
    .split(vertical[1])[1]
}

fn hit(app: &mut App, area: Rect, action: ClickAction) {
    if area.width > 0 && area.height > 0 {
        app.hits.push((area, action));
    }
}

fn framed(title: &str) -> Block<'static> {
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER))
        .padding(Padding::horizontal(2));
    if title.is_empty() {
        block
    } else {
        block.title(Line::from(vec![
            Span::raw(" "),
            muted(title.to_owned()),
            Span::raw(" "),
        ]))
    }
}

fn page_index(page: Page) -> &'static str {
    match page {
        Page::Overview => "01",
        Page::Files => "02",
        Page::Processes => "03",
        Page::Services => "04",
        Page::Logs => "05",
        Page::Terminal => "06",
        Page::Settings => "07",
    }
}

fn nav_title(page: Page, compact: bool) -> &'static str {
    if compact {
        match page {
            Page::Overview => "概览",
            Page::Files => "文件",
            Page::Processes => "进程",
            Page::Services => "服务",
            Page::Logs => "日志",
            Page::Terminal => "终端",
            Page::Settings => "设置",
        }
    } else {
        page.title()
    }
}

fn page_meta(app: &App) -> Option<String> {
    match app.page {
        Page::Overview => Some(format!("自动刷新 {} 秒", app.refresh_interval.as_secs())),
        Page::Files => Some(format!(
            "{} 项  ·  {}",
            app.files.entries().len(),
            app.files.relative_cwd()
        )),
        Page::Processes => Some(format!(
            "{} 进程  ·  按{}",
            app.process_rows.len(),
            sort_label(app.processes.sort)
        )),
        Page::Services => Some(if app.services.loading {
            "正在读取服务…".to_owned()
        } else {
            format!("{} 个服务", app.services.visible().len())
        }),
        Page::Logs => Some(format!("{} 行", app.logs.visible().len())),
        Page::Terminal => Some(format!(
            "{} 个标签  ·  {}",
            app.terminals.len(),
            app.terminals.index() + 1
        )),
        Page::Settings => Some(format!("v{}", env!("CARGO_PKG_VERSION"))),
    }
}

fn sort_label(sort: ProcessSort) -> &'static str {
    match sort {
        ProcessSort::Cpu => "CPU",
        ProcessSort::Memory => "内存",
        ProcessSort::Name => "名称",
        ProcessSort::Pid => "PID",
    }
}

fn action_label(action: ServiceAction) -> &'static str {
    match action {
        ServiceAction::Start => "启动",
        ServiceAction::Stop => "停止",
        ServiceAction::Restart => "重启",
    }
}

fn status_label(status: &str) -> &str {
    match status {
        "Running" | "Runnable" => "运行",
        "Sleeping" | "Sleep" => "休眠",
        "Idle" => "空闲",
        "Zombie" => "僵尸",
        "Stopped" => "停止",
        "Tracing" => "跟踪",
        "Dead" => "结束",
        "Wakekill" | "Waking" => "唤醒",
        "Parked" => "挂起",
        "LockBlocked" => "锁等待",
        "UninterruptibleDiskSleep" => "磁盘等待",
        other => other,
    }
}

struct Dim;

impl Widget for Dim {
    fn render(self, area: Rect, buf: &mut Buffer) {
        for y in area.top()..area.bottom() {
            for x in area.left()..area.right() {
                let cell = &mut buf[(x, y)];
                cell.modifier.insert(Modifier::DIM);
                cell.fg = half(cell.fg);
                cell.bg = half(cell.bg);
            }
        }
    }
}

fn half(color: Color) -> Color {
    match color {
        Color::Rgb(red, green, blue) => Color::Rgb(red / 2, green / 2, blue / 2),
        other => other,
    }
}

pub fn draw(frame: &mut Frame, app: &mut App) {
    app.hits.clear();
    app.terminal_area = Rect::default();
    let area = frame.area();
    frame.render_widget(Block::default().style(Style::default().bg(BG)), area);
    if area.width < 64 || area.height < 18 {
        draw_fallback(frame, area);
        return;
    }
    let padded = u16::from(area.height >= 26);
    let body = Rect::new(
        area.x,
        area.y + padded,
        area.width,
        area.height.saturating_sub(padded * 2),
    );
    let content = content_area(body);
    let rows = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(10),
        Constraint::Length(2),
    ])
    .split(content);
    draw_header(frame, app, rows[0]);
    draw_page(frame, app, rows[1]);
    draw_footer(frame, app, rows[2]);
    draw_toast(frame, app, body);
    if app.modal.is_some() {
        draw_modal(frame, app);
    }
}

fn draw_fallback(frame: &mut Frame, area: Rect) {
    let card = centered(area, 46, 7);
    frame.render_widget(Clear, card);
    frame.render_widget(
        Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(BORDER))
            .style(Style::default().bg(SURFACE)),
        card,
    );
    let inner = card.inner(Margin::new(2, 1));
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(label("APKEZ").bold()),
            Line::from(""),
            Line::from(muted("终端空间不足")),
            Line::from(label("扩大终端以继续 · 至少 64 × 18")),
            Line::from(vec![cap("Ctrl+Q"), muted(" 退出")]),
        ])
        .alignment(Alignment::Center),
        inner,
    );
}

fn draw_header(frame: &mut Frame, app: &mut App, area: Rect) {
    let brand_width = 12;
    frame.render_widget(
        label("apkez").bold(),
        Rect::new(area.x, area.y, brand_width, 1),
    );
    frame.render_widget(
        muted("终端工作区"),
        Rect::new(area.x, area.y + 1, brand_width, 1),
    );

    let help_width = 7;
    let help_x = area.right().saturating_sub(help_width);
    frame.render_widget(
        Paragraph::new(muted("? 帮助")).alignment(Alignment::Right),
        Rect::new(help_x, area.y, help_width, 1),
    );
    hit(
        app,
        Rect::new(help_x, area.y, help_width, 1),
        ClickAction::Help,
    );

    let palette_width = 13;
    let palette_x = help_x.saturating_sub(palette_width + 2);
    let nav_x = area.x + brand_width + 2;
    let palette_visible = palette_x > nav_x + 40;
    if palette_visible {
        frame.render_widget(
            Line::from(vec![
                Span::styled(" Ctrl+K ", Style::default().fg(TEXT).bg(SURFACE)),
                Span::raw(" "),
                muted("命令"),
            ]),
            Rect::new(palette_x, area.y, palette_width, 1),
        );
        hit(
            app,
            Rect::new(palette_x, area.y, palette_width, 1),
            ClickAction::Palette,
        );
    }

    let available = if palette_visible {
        palette_x.saturating_sub(nav_x).saturating_sub(2)
    } else {
        help_x.saturating_sub(nav_x).saturating_sub(2)
    };
    let compact = full_nav_width(2) > available;
    let gap = if compact { 1 } else { 2 };
    let labels: Vec<String> = Page::ALL
        .iter()
        .map(|page| format!(" {} ", nav_title(*page, compact)))
        .collect();
    let widths: Vec<u16> = labels.iter().map(|label| text_width(label)).collect();
    let total = widths.iter().sum::<u16>() + gap * (Page::ALL.len() as u16 - 1);
    if total <= available {
        let mut x = nav_x;
        for (index, page) in Page::ALL.iter().enumerate() {
            let item = Rect::new(x, area.y, widths[index], 1);
            let style = if app.page == *page {
                Style::default().fg(BG).bg(SELECTED).bold()
            } else {
                Style::default().fg(MUTED)
            };
            frame.render_widget(Paragraph::new(labels[index].clone()).style(style), item);
            hit(app, item, ClickAction::Navigate(*page));
            x += widths[index] + gap;
        }
    }

    if area.width >= 100 {
        frame.render_widget(
            Paragraph::new(muted(format!("v{}", env!("CARGO_PKG_VERSION"))))
                .alignment(Alignment::Right),
            Rect::new(palette_x.saturating_sub(13), area.y, 11, 1),
        );
    }

    frame.render_widget(
        Paragraph::new("─".repeat(area.width as usize)).fg(BORDER),
        Rect::new(area.x, area.y + 2, area.width, 1),
    );
}

fn full_nav_width(gap: u16) -> u16 {
    Page::ALL
        .iter()
        .map(|page| text_width(page.title()) + 2)
        .sum::<u16>()
        + gap * (Page::ALL.len() as u16 - 1)
}

fn draw_page(frame: &mut Frame, app: &mut App, area: Rect) {
    let gap = u16::from(area.height >= 24);
    frame.render_widget(
        Line::from(vec![
            dim(page_index(app.page)),
            Span::raw("  "),
            label(app.page.title()).bold(),
        ]),
        Rect::new(area.x, area.y, area.width.saturating_sub(34), 1),
    );
    let meta_width = area.width.min(34);
    let meta = if area.width >= 80 {
        page_meta(app)
    } else {
        None
    };
    if let Some(meta) = meta {
        frame.render_widget(
            Paragraph::new(muted(meta)).alignment(Alignment::Right),
            Rect::new(
                area.right().saturating_sub(meta_width),
                area.y + 1,
                meta_width,
                1,
            ),
        );
    }
    frame.render_widget(
        muted(app.page.subtitle()),
        Rect::new(
            area.x,
            area.y + 1,
            area.width
                .saturating_sub(if area.width >= 80 { 36 } else { 0 }),
            1,
        ),
    );
    let content = Rect::new(
        area.x,
        area.y + 2 + gap,
        area.width,
        area.height.saturating_sub(2 + gap),
    );
    match app.page {
        Page::Overview => draw_overview(frame, app, content),
        Page::Files => draw_files(frame, app, content),
        Page::Processes => draw_processes(frame, app, content),
        Page::Services => draw_services(frame, app, content),
        Page::Logs => draw_logs(frame, app, content),
        Page::Terminal => draw_terminal(frame, app, content),
        Page::Settings => draw_settings(frame, app, content),
    }
}

struct Metric {
    label: &'static str,
    value: String,
    detail: String,
    ratio: f32,
}

fn metrics(app: &App) -> [Metric; 4] {
    let cores = app.system.cpu_count.max(1) as f32;
    let load = (app.system.load.0 as f32 / cores * 100.0).clamp(0.0, 100.0);
    [
        Metric {
            label: "CPU",
            value: format!("{:.1}%", app.system.cpu),
            detail: format!("{} 核", app.system.cpu_count),
            ratio: app.system.cpu,
        },
        Metric {
            label: "内存",
            value: format!("{:.1}%", app.system.memory_percent()),
            detail: format!(
                "{} / {}",
                format_bytes(app.system.memory_used),
                format_bytes(app.system.memory_total)
            ),
            ratio: app.system.memory_percent(),
        },
        Metric {
            label: "磁盘",
            value: format!("{:.1}%", app.system.disk_percent()),
            detail: format!(
                "{} / {}",
                format_bytes(app.system.disk_used),
                format_bytes(app.system.disk_total)
            ),
            ratio: app.system.disk_percent(),
        },
        Metric {
            label: "进程",
            value: app.system.system.processes().len().to_string(),
            detail: format!("负载 {:.2}", app.system.load.0),
            ratio: load,
        },
    ]
}

fn draw_overview(frame: &mut Frame, app: &mut App, area: Rect) {
    if area.height < 22 {
        draw_overview_compact(frame, app, area);
        return;
    }
    let wide_metrics = area.width >= 88;
    let metrics_height = if wide_metrics { 6 } else { 10 };
    let rows = Layout::vertical([
        Constraint::Length(metrics_height),
        Constraint::Length(1),
        Constraint::Min(8),
        Constraint::Length(1),
        Constraint::Length(2),
    ])
    .split(area);
    draw_metrics(frame, app, rows[0], wide_metrics);
    draw_activity(frame, app, rows[2], area.width >= 88);
    draw_quick_entries(frame, app, rows[4]);
}

fn draw_metrics(frame: &mut Frame, app: &App, area: Rect, wide: bool) {
    let block = framed("资源");
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let metrics = metrics(app);
    if wide {
        let cell_width = inner.width.saturating_sub(3) / 4;
        for (index, metric) in metrics.iter().enumerate() {
            let x = inner.x + index as u16 * (cell_width + 1);
            let cell = Rect::new(x, inner.y, cell_width, inner.height);
            draw_metric_cell(frame, cell, metric, true);
            if index < 3 {
                vertical_rule(frame, x + cell_width, inner.y, inner.height);
            }
        }
    } else {
        let column_width = inner.width.saturating_sub(1) / 2;
        let first = Rect::new(inner.x, inner.y, column_width, 4);
        let second = Rect::new(inner.x + column_width + 1, inner.y, column_width, 4);
        let third = Rect::new(inner.x, inner.y + 4, column_width, 4);
        let fourth = Rect::new(inner.x + column_width + 1, inner.y + 4, column_width, 4);
        draw_metric_cell(frame, first, &metrics[0], true);
        draw_metric_cell(frame, second, &metrics[1], true);
        draw_metric_cell(frame, third, &metrics[2], true);
        draw_metric_cell(frame, fourth, &metrics[3], true);
        vertical_rule(frame, inner.x + column_width, inner.y, inner.height);
    }
}

fn draw_metric_cell(frame: &mut Frame, area: Rect, metric: &Metric, show_bar: bool) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    frame.render_widget(
        Paragraph::new(Span::styled(metric.label, Style::default().fg(MUTED))),
        Rect::new(area.x, area.y, area.width, 1),
    );
    if area.height > 1 {
        frame.render_widget(
            Paragraph::new(Span::styled(
                metric.value.clone(),
                Style::default().fg(TEXT).bold(),
            )),
            Rect::new(area.x, area.y + 1, area.width, 1),
        );
    }
    if show_bar && area.height > 2 {
        let bar_width = area.width.saturating_sub(2);
        if bar_width > 0 {
            let filled =
                (metric.ratio.clamp(0.0, 100.0) / 100.0 * bar_width as f32).round() as usize;
            let remaining = bar_width as usize - filled;
            let bar = Line::from(vec![
                Span::styled("█".repeat(filled), Style::default().fg(TEXT)),
                Span::styled("░".repeat(remaining), Style::default().fg(TRACK)),
            ]);
            frame.render_widget(bar, Rect::new(area.x, area.y + 2, bar_width, 1));
        }
    }
    if area.height > 3 {
        frame.render_widget(
            Paragraph::new(muted(shorten(&metric.detail, area.width))),
            Rect::new(area.x, area.y + 3, area.width, 1),
        );
    }
}

fn draw_activity(frame: &mut Frame, app: &App, area: Rect, wide: bool) {
    let total = area.width;
    let left_width = if wide {
        total.saturating_mul(2) / 3
    } else {
        total.saturating_sub(2) / 2
    };
    let left = Rect::new(area.x, area.y, left_width, area.height);
    let right = Rect::new(
        area.x + left_width + 2,
        area.y,
        total.saturating_sub(left_width + 2),
        area.height,
    );
    draw_cpu_panel(frame, app, left);
    draw_system_panel(frame, app, right);
}

fn draw_cpu_panel(frame: &mut Frame, app: &App, area: Rect) {
    let block = framed("CPU 活动");
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height == 0 || inner.width == 0 {
        return;
    }
    let status_text = if app.system.reduced_motion {
        "已暂停"
    } else if app.system.history.len() < 6 {
        "采样中"
    } else {
        "实时"
    };
    frame.render_widget(
        Line::from(vec![
            muted(format!("最近 {} 次采样 · 0–100%", app.system.history.len())),
            Span::raw("   "),
            Span::styled("●", Style::default().fg(TEXT)),
            muted(format!(" {status_text}")),
        ]),
        Rect::new(inner.x, inner.y, inner.width, 1),
    );
    if inner.height < 4 || inner.width < 12 {
        return;
    }
    let axis_width = 4;
    let chart = Rect::new(
        inner.x + axis_width + 1,
        inner.y + 2,
        inner.width.saturating_sub(axis_width + 1),
        inner.height.saturating_sub(3),
    );
    if chart.height == 0 {
        return;
    }
    for (row, text) in [
        (0, "100"),
        (chart.height / 2, "50"),
        (chart.height - 1, "0"),
    ] {
        frame.render_widget(
            Paragraph::new(muted(text)).alignment(Alignment::Right),
            Rect::new(inner.x, chart.y + row, axis_width, 1),
        );
    }
    for row in [0, chart.height / 2, chart.height - 1] {
        frame.render_widget(
            Paragraph::new("─".repeat(chart.width as usize)).fg(TRACK),
            Rect::new(chart.x, chart.y + row, chart.width, 1),
        );
    }
    frame.render_widget(
        Paragraph::new(muted("60 秒前")),
        Rect::new(inner.x, inner.bottom().saturating_sub(1), 12, 1),
    );
    frame.render_widget(
        Paragraph::new(muted("现在")).alignment(Alignment::Right),
        Rect::new(
            inner.right().saturating_sub(8),
            inner.bottom().saturating_sub(1),
            8,
            1,
        ),
    );
    let notice = Rect::new(
        chart.x,
        chart.y + (chart.height / 4).min(chart.height.saturating_sub(1)),
        chart.width,
        1,
    );
    if app.system.reduced_motion {
        frame.render_widget(
            Paragraph::new(muted("动态曲线已暂停")).alignment(Alignment::Center),
            notice,
        );
        return;
    }
    if app.system.history.len() < 6 {
        frame.render_widget(
            Paragraph::new(muted("正在采集趋势…")).alignment(Alignment::Center),
            notice,
        );
        return;
    }
    let take = chart.width as usize;
    let history = &app.system.history;
    let start = history.len().saturating_sub(take);
    let data: Vec<u64> = history
        .iter()
        .skip(start)
        .map(|value| value.clamp(0.0, 100.0) as u64)
        .collect();
    frame.render_widget(
        Sparkline::default()
            .data(&data)
            .max(100)
            .style(Style::default().fg(TEXT)),
        chart,
    );
}

fn draw_system_panel(frame: &mut Frame, app: &App, area: Rect) {
    let block = framed("系统");
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let mut lines = vec![
        info_line("主机", app.system.hostname.clone()),
        info_line("系统", app.system.os.clone()),
        info_line("内核", app.system.kernel.clone()),
        info_line(
            "负载",
            format!(
                "{:.2}  {:.2}  {:.2}",
                app.system.load.0, app.system.load.1, app.system.load.2
            ),
        ),
        info_line("在线", format_duration(app.system.uptime)),
    ];
    if inner.width >= 30 {
        lines.push(info_line(
            "网络",
            format!(
                "↓ {}  ↑ {}",
                format_bytes(app.system.network_rx),
                format_bytes(app.system.network_tx)
            ),
        ));
    } else {
        lines.push(info_line("网络↓", format_bytes(app.system.network_rx)));
        lines.push(info_line("网络↑", format_bytes(app.system.network_tx)));
    }
    lines.push(info_line(
        "交换",
        format!(
            "{} / {}",
            format_bytes(app.system.swap_used),
            format_bytes(app.system.swap_total)
        ),
    ));
    lines.push(info_line(
        "磁盘",
        format!(
            "{} / {}",
            format_bytes(app.system.disk_used),
            format_bytes(app.system.disk_total)
        ),
    ));
    lines.push(info_line(
        "进程",
        format!(
            "{} 个 · {} 核",
            app.system.system.processes().len(),
            app.system.cpu_count
        ),
    ));
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), inner);
}

fn info_line(name: &str, value: String) -> Line<'static> {
    Line::from(vec![muted(format!("{name}  ")), label(shorten(&value, 30))])
}

fn draw_overview_compact(frame: &mut Frame, app: &mut App, area: Rect) {
    let rows = Layout::vertical([
        Constraint::Length(4),
        Constraint::Length(1),
        Constraint::Min(4),
    ])
    .split(area);
    let block = framed("资源");
    let inner = block.inner(rows[0]);
    frame.render_widget(block, rows[0]);
    let metrics = metrics(app);
    let cell_width = inner.width.saturating_sub(3) / 4;
    for (index, metric) in metrics.iter().enumerate() {
        let x = inner.x + index as u16 * (cell_width + 1);
        let cell = Rect::new(x, inner.y, cell_width, inner.height);
        draw_metric_cell(frame, cell, metric, false);
        if index < 3 {
            vertical_rule(frame, x + cell_width, inner.y, inner.height);
        }
    }
    let block = framed("系统");
    let inner = block.inner(rows[2]);
    frame.render_widget(block, rows[2]);
    frame.render_widget(
        Paragraph::new(vec![
            info_line("主机", app.system.hostname.clone()),
            info_line(
                "负载",
                format!(
                    "{:.2}  {:.2}  {:.2}",
                    app.system.load.0, app.system.load.1, app.system.load.2
                ),
            ),
            info_line("在线", format_duration(app.system.uptime)),
        ]),
        inner,
    );
}

fn draw_quick_entries(frame: &mut Frame, app: &mut App, area: Rect) {
    frame.render_widget(
        Paragraph::new("─".repeat(area.width as usize)).fg(BORDER),
        Rect::new(area.x, area.y, area.width, 1),
    );
    let wide = area.width >= 72;
    let entries = [
        ("F2", Page::Files, "文件管理", "文件"),
        ("F3", Page::Processes, "进程管理", "进程"),
        ("F4", Page::Terminal, "交互式终端", "终端"),
    ];
    let mut x = area.x;
    let gap = if wide { 4 } else { 2 };
    for (key, page, long, short) in entries {
        let title = if wide { long } else { short };
        let item_width = text_width(key) + 2 + 1 + text_width(title);
        if x + item_width > area.right() {
            break;
        }
        frame.render_widget(
            Line::from(vec![cap(key), Span::raw(" "), label(title)]),
            Rect::new(x, area.y + 1, item_width, 1),
        );
        hit(
            app,
            Rect::new(x, area.y + 1, item_width, 1),
            ClickAction::Navigate(page),
        );
        x += item_width + gap;
    }
}

fn draw_files(frame: &mut Frame, app: &mut App, area: Rect) {
    let rows = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(6),
        Constraint::Length(2),
    ])
    .split(area);
    let mut spans = vec![cap("←"), Span::raw(" "), label("上级")];
    let parent_width = text_width("←") + 2 + 1 + text_width("上级");
    spans.extend([
        Span::raw("    "),
        muted("工作区"),
        Span::raw("  "),
        label(app.files.relative_cwd()),
    ]);
    if !app.files.filter.is_empty() {
        spans.extend([
            Span::raw("    "),
            muted(format!("筛选 “{}”", app.files.filter)),
        ]);
    }
    frame.render_widget(
        Line::from(spans),
        Rect::new(rows[0].x, rows[0].y, rows[0].width, 1),
    );
    hit(
        app,
        Rect::new(rows[0].x, rows[0].y, parent_width, 1),
        ClickAction::FileParent,
    );
    if app.files.show_hidden {
        frame.render_widget(
            Paragraph::new(muted("隐藏项已显示")).alignment(Alignment::Right),
            Rect::new(rows[0].x, rows[0].y, rows[0].width, 1),
        );
    }

    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER))
        .title(Line::from(vec![
            Span::raw(" "),
            muted("文件列表"),
            Span::raw(" "),
        ]))
        .padding(Padding::horizontal(1));
    let wide = rows[1].width >= 90;
    let columns = Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Length(2),
        Constraint::Length(28),
    ])
    .split(rows[1]);
    let list_area = if wide { columns[0] } else { rows[1] };
    if wide {
        draw_file_details(frame, app, columns[2]);
    }
    let inner = block.inner(list_area);
    let visible = inner.height.saturating_sub(1) as usize;
    scroll_selection(app.files.selected, &mut app.files.offset, visible.max(1));
    let header = Row::new(vec![
        Line::from(""),
        Line::from("名称"),
        Line::from("大小").alignment(Alignment::Right),
        Line::from("类型").alignment(Alignment::Right),
    ])
    .style(Style::default().fg(MUTED));
    let entries: Vec<Row> = app
        .files
        .entries()
        .iter()
        .enumerate()
        .skip(app.files.offset)
        .take(visible)
        .map(|(index, entry)| {
            let selected = index == app.files.selected;
            let style = if selected {
                Style::default().fg(BG).bg(SELECTED)
            } else {
                Style::default().fg(TEXT)
            };
            Row::new(vec![
                Line::from(if entry.directory { "▸" } else { " " }),
                Line::from(entry.name.clone()),
                Line::from(if entry.directory {
                    "—".to_owned()
                } else {
                    format_bytes(entry.size)
                })
                .alignment(Alignment::Right),
                Line::from(if entry.directory { "目录" } else { "文件" })
                    .alignment(Alignment::Right),
            ])
            .style(style)
        })
        .collect();
    frame.render_widget(
        Table::new(
            entries,
            [
                Constraint::Length(2),
                Constraint::Min(8),
                Constraint::Length(10),
                Constraint::Length(6),
            ],
        )
        .header(header)
        .column_spacing(2)
        .block(block),
        list_area,
    );
    for index in app.files.offset..(app.files.offset + visible).min(app.files.entries().len()) {
        hit(
            app,
            Rect::new(
                inner.x,
                inner.y + 1 + (index - app.files.offset) as u16,
                inner.width,
                1,
            ),
            ClickAction::FileRow(index),
        );
    }
    if app.files.entries().is_empty() && inner.height > 1 {
        frame.render_widget(
            Paragraph::new(muted("空目录 · n 新建文件 · N 新建目录")).alignment(Alignment::Center),
            Rect::new(inner.x, inner.y + 1, inner.width, 1),
        );
    }
    draw_actions(
        frame,
        app,
        Rect::new(rows[2].x, rows[2].y + 1, rows[2].width, 1),
        &[
            ("n", "新建文件", "新建", ClickAction::NewFile),
            ("N", "新建目录", "目录", ClickAction::NewDirectory),
            ("r", "重命名", "改名", ClickAction::Rename),
            ("d", "删除", "删除", ClickAction::Delete),
            (".", "隐藏项", "隐藏", ClickAction::Hidden),
        ],
    );
}

fn draw_file_details(frame: &mut Frame, app: &App, area: Rect) {
    let block = framed("详情");
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height < 6 || inner.width < 10 {
        return;
    }
    let filter = if app.files.filter.is_empty() {
        "未启用".to_owned()
    } else {
        format!("“{}”", app.files.filter)
    };
    let hidden = if app.files.show_hidden {
        "显示"
    } else {
        "不显示"
    };
    let mut lines = vec![
        Line::from(label("目录").bold()),
        detail_line("路径", app.files.relative_cwd()),
        detail_line("项目", format!("{} 项", app.files.entries().len())),
        detail_line("筛选", filter),
        detail_line("隐藏项", hidden),
        Line::from(""),
        Line::from(label("选中").bold()),
    ];
    match app.files.selected_entry() {
        Some(entry) => {
            lines.push(detail_line("名称", entry.name.clone()));
            lines.push(detail_line(
                "类型",
                if entry.directory { "目录" } else { "文件" },
            ));
            lines.push(detail_line(
                "大小",
                if entry.directory {
                    "—".to_owned()
                } else {
                    format_bytes(entry.size)
                },
            ));
        }
        None => lines.push(Line::from(muted("暂无选中项"))),
    }
    lines.push(Line::from(""));
    lines.push(Line::from(label("操作").bold()));
    lines.push(Line::from(vec![
        cap("Enter"),
        Span::raw(" "),
        muted("打开"),
    ]));
    lines.push(Line::from(vec![
        cap("n"),
        Span::raw(" "),
        muted("新建文件"),
    ]));
    lines.push(Line::from(vec![
        cap("N"),
        Span::raw(" "),
        muted("新建目录"),
    ]));
    lines.push(Line::from(vec![cap("r"), Span::raw(" "), muted("重命名")]));
    lines.push(Line::from(vec![cap("d"), Span::raw(" "), muted("删除")]));
    lines.push(Line::from(vec![cap("H"), Span::raw(" "), muted("隐藏项")]));
    frame.render_widget(Paragraph::new(lines), inner);
}

fn detail_line(name: &str, value: impl Into<String>) -> Line<'static> {
    let value = value.into();
    Line::from(vec![muted(format!("{name}  ")), label(shorten(&value, 16))])
}

fn draw_processes(frame: &mut Frame, app: &mut App, area: Rect) {
    let rows = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(6),
        Constraint::Length(2),
    ])
    .split(area);
    let mut spans = vec![
        label(format!("{} 进程", app.process_rows.len())),
        Span::raw("    "),
        muted(format!("CPU {:.1}%", app.system.cpu)),
    ];
    if !app.processes.filter.is_empty() {
        spans.extend([
            Span::raw("    "),
            muted(format!("筛选 “{}”", app.processes.filter)),
        ]);
    }
    frame.render_widget(
        Line::from(spans),
        Rect::new(rows[0].x, rows[0].y, rows[0].width, 1),
    );
    frame.render_widget(
        Paragraph::new(muted(format!("排序 {}", sort_label(app.processes.sort))))
            .alignment(Alignment::Right),
        Rect::new(rows[0].x, rows[0].y, rows[0].width, 1),
    );

    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER))
        .padding(Padding::horizontal(1));
    let inner = block.inner(rows[1]);
    let visible = inner.height.saturating_sub(1) as usize;
    scroll_selection(
        app.processes.selected(),
        &mut app.processes.offset,
        visible.max(1),
    );
    let entries: Vec<Row> = app
        .process_rows
        .iter()
        .enumerate()
        .skip(app.processes.offset)
        .take(visible)
        .map(|(index, process)| {
            let selected = index == app.processes.selected();
            Row::new(vec![
                Line::from(process.pid.to_string()),
                Line::from(process.name.clone()),
                Line::from(process.user.clone()),
                Line::from(format!("{:.1}%", process.cpu)).alignment(Alignment::Right),
                Line::from(format_bytes(process.memory)).alignment(Alignment::Right),
                Line::from(status_label(&process.status)).alignment(Alignment::Right),
            ])
            .style(if selected {
                Style::default().fg(BG).bg(SELECTED)
            } else {
                Style::default().fg(TEXT)
            })
        })
        .collect();
    let header = Row::new(vec![
        Line::from("PID"),
        Line::from("进程"),
        Line::from("用户"),
        Line::from("CPU").alignment(Alignment::Right),
        Line::from("内存").alignment(Alignment::Right),
        Line::from("状态").alignment(Alignment::Right),
    ])
    .style(Style::default().fg(MUTED));
    frame.render_widget(
        Table::new(
            entries,
            [
                Constraint::Length(7),
                Constraint::Min(10),
                Constraint::Length(6),
                Constraint::Length(6),
                Constraint::Length(9),
                Constraint::Length(8),
            ],
        )
        .header(header)
        .column_spacing(1)
        .block(block),
        rows[1],
    );
    for index in app.processes.offset..(app.processes.offset + visible).min(app.process_rows.len())
    {
        hit(
            app,
            Rect::new(
                inner.x,
                inner.y + 1 + (index - app.processes.offset) as u16,
                inner.width,
                1,
            ),
            ClickAction::ProcessRow(index),
        );
    }
    draw_actions(
        frame,
        app,
        Rect::new(rows[2].x, rows[2].y + 1, rows[2].width, 1),
        &[
            ("s", "切换排序", "排序", ClickAction::Sort),
            ("x", "终止进程", "终止", ClickAction::Kill),
            ("r", "刷新", "刷新", ClickAction::Refresh),
        ],
    );
}

fn draw_services(frame: &mut Frame, app: &mut App, area: Rect) {
    let rows = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(6),
        Constraint::Length(2),
    ])
    .split(area);
    let mut spans = vec![label("systemd 服务")];
    if app.services.loading {
        spans.extend([Span::raw("    "), muted("正在读取…")]);
    }
    if !app.services.filter.is_empty() {
        spans.extend([
            Span::raw("    "),
            muted(format!("筛选 “{}”", app.services.filter)),
        ]);
    }
    frame.render_widget(
        Line::from(spans),
        Rect::new(rows[0].x, rows[0].y, rows[0].width, 1),
    );
    frame.render_widget(
        Paragraph::new(muted("s 启动 · x 停止 · r 重启 · Enter 查看日志"))
            .alignment(Alignment::Right),
        Rect::new(rows[0].x, rows[0].y, rows[0].width, 1),
    );

    let block = framed("服务列表");
    let inner = block.inner(rows[1]);
    let visible = inner.height.saturating_sub(1) as usize;
    scroll_selection(
        app.services.selected,
        &mut app.services.offset,
        visible.max(1),
    );
    let header = Row::new(vec![
        Line::from("状态"),
        Line::from("名称"),
        Line::from("描述"),
    ])
    .style(Style::default().fg(MUTED));
    let indices = app.services.visible();
    let entries: Vec<Row> = indices
        .iter()
        .skip(app.services.offset)
        .take(visible)
        .enumerate()
        .map(|(offset, index)| {
            let position = app.services.offset + offset;
            let selected = position == app.services.selected;
            let style = if selected {
                Style::default().fg(BG).bg(SELECTED)
            } else {
                Style::default().fg(TEXT)
            };
            let service = &app.services.services[*index];
            let state = format!(
                "{} {} / {}",
                if service.active == "active" {
                    "●"
                } else {
                    "○"
                },
                service.active,
                service.sub
            );
            Row::new(vec![
                Line::from(shorten(&state, 19)),
                Line::from(shorten(&service.name, 28)),
                Line::from(shorten(&service.description, 60)),
            ])
            .style(style)
        })
        .collect();
    frame.render_widget(
        Table::new(
            entries,
            [
                Constraint::Length(21),
                Constraint::Length(30),
                Constraint::Min(20),
            ],
        )
        .header(header)
        .column_spacing(2)
        .block(block),
        rows[1],
    );
    for offset in 0..visible.min(indices.len().saturating_sub(app.services.offset)) {
        hit(
            app,
            Rect::new(inner.x, inner.y + 1 + offset as u16, inner.width, 1),
            ClickAction::ServiceRow(app.services.offset + offset),
        );
    }
    if indices.is_empty() && !app.services.loading && inner.height > 1 {
        frame.render_widget(
            Paragraph::new(muted("未发现服务 · R 重新读取")).alignment(Alignment::Center),
            Rect::new(inner.x, inner.y + 1, inner.width, 1),
        );
    }
    draw_actions(
        frame,
        app,
        Rect::new(rows[2].x, rows[2].y + 1, rows[2].width, 1),
        &[
            (
                "s",
                "启动服务",
                "启动",
                ClickAction::ServiceAction(ServiceAction::Start),
            ),
            (
                "x",
                "停止服务",
                "停止",
                ClickAction::ServiceAction(ServiceAction::Stop),
            ),
            (
                "r",
                "重启服务",
                "重启",
                ClickAction::ServiceAction(ServiceAction::Restart),
            ),
            ("R", "刷新列表", "刷新", ClickAction::RefreshServices),
        ],
    );
}

fn draw_logs(frame: &mut Frame, app: &mut App, area: Rect) {
    let rows = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(6),
        Constraint::Length(2),
    ])
    .split(area);
    let source = match app.logs.source {
        LogSource::Journal => {
            if app.logs.service.is_empty() {
                "journalctl · 全部日志".to_owned()
            } else {
                format!("journalctl · {}", app.logs.service)
            }
        }
        LogSource::File => app
            .logs
            .path
            .as_ref()
            .map_or_else(|| "日志文件".to_owned(), |path| path.display().to_string()),
    };
    let mut spans = vec![label(shorten(&source, 60))];
    if !app.logs.filter.is_empty() {
        spans.extend([
            Span::raw("    "),
            muted(format!("筛选 “{}”", app.logs.filter)),
        ]);
    }
    frame.render_widget(
        Line::from(spans),
        Rect::new(rows[0].x, rows[0].y, rows[0].width, 1),
    );
    let follow = if app.logs.following {
        bright_cap("实时跟随")
    } else {
        Span::styled(" 已暂停 ", Style::default().fg(MUTED).bg(SURFACE))
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![follow])).alignment(Alignment::Right),
        Rect::new(rows[0].x, rows[0].y, rows[0].width, 1),
    );
    hit(
        app,
        Rect::new(rows[0].right().saturating_sub(12), rows[0].y, 12, 1),
        ClickAction::ToggleLogFollow,
    );

    let block = framed("日志输出");
    let inner = block.inner(rows[1]);
    frame.render_widget(block, rows[1]);
    if let Some(error) = &app.logs.error {
        frame.render_widget(
            Paragraph::new(Span::styled(error.clone(), Style::default().fg(ERROR)))
                .wrap(Wrap { trim: true }),
            inner,
        );
        return;
    }
    let lines = app.logs.visible();
    if lines.is_empty() {
        frame.render_widget(
            Paragraph::new(muted("暂无日志 · o 打开文件 · J 读取 journalctl")),
            inner,
        );
        return;
    }
    let height = inner.height as usize;
    let start = app
        .logs
        .offset
        .min(lines.len().saturating_sub(1))
        .saturating_sub(height.saturating_sub(1));
    let end = (start + height).min(lines.len());
    let body: Vec<Line> = lines[start..end]
        .iter()
        .map(|line| Line::from(Span::styled((*line).to_owned(), Style::default().fg(TEXT))))
        .collect();
    frame.render_widget(Paragraph::new(body), inner);
    draw_actions(
        frame,
        app,
        Rect::new(rows[2].x, rows[2].y + 1, rows[2].width, 1),
        &[
            ("f", "跟随日志", "跟随", ClickAction::ToggleLogFollow),
            ("o", "打开文件", "打开", ClickAction::OpenLogPath),
            ("r", "刷新日志", "刷新", ClickAction::RefreshLogs),
        ],
    );
}

fn draw_terminal(frame: &mut Frame, app: &mut App, area: Rect) {
    let alive = app.terminals.active().is_alive();
    let status_area = Rect::new(area.x, area.y, area.width, 1);
    frame.render_widget(
        Line::from(vec![
            Span::styled("●", Style::default().fg(if alive { TEXT } else { ERROR })),
            label(if alive {
                "  Shell 会话运行中"
            } else {
                "  Shell 已退出"
            }),
            muted(if alive {
                "  ·  所有按键发送至终端"
            } else {
                "  ·  按 Enter 重启"
            }),
        ]),
        status_area,
    );
    hit(app, status_area, ClickAction::RestartShell);
    frame.render_widget(
        Paragraph::new(muted("Ctrl+] 返回桌面")).alignment(Alignment::Right),
        Rect::new(area.right().saturating_sub(18), area.y, 18, 1),
    );
    draw_terminal_tabs(frame, app, Rect::new(area.x, area.y + 1, area.width, 1));
    frame.render_widget(
        Paragraph::new("─".repeat(area.width as usize)).fg(BORDER),
        Rect::new(area.x, area.y + 2, area.width, 1),
    );
    let terminal_area = Rect::new(
        area.x,
        area.y + 3,
        area.width,
        area.height.saturating_sub(3),
    );
    if terminal_area.height == 0 || terminal_area.width == 0 {
        return;
    }
    if let Err(error) = app
        .terminals
        .active_mut()
        .resize(terminal_area.height, terminal_area.width)
    {
        frame.render_widget(muted(format!("终端尺寸调整失败: {error}")), terminal_area);
        return;
    }
    frame.render_widget(
        Block::default().style(Style::default().bg(BG)),
        terminal_area,
    );
    let lines = app.terminals.active().lines();
    for (index, line) in lines
        .into_iter()
        .enumerate()
        .take(terminal_area.height as usize)
    {
        frame.render_widget(
            Paragraph::new(line),
            Rect::new(
                terminal_area.x,
                terminal_area.y + index as u16,
                terminal_area.width,
                1,
            ),
        );
    }
    if let Some((col, row)) = app
        .terminals
        .active()
        .cursor()
        .filter(|(col, row)| *col < terminal_area.width && *row < terminal_area.height)
    {
        frame.set_cursor_position((terminal_area.x + col, terminal_area.y + row));
    }
    app.terminal_area = terminal_area;
}

fn draw_terminal_tabs(frame: &mut Frame, app: &mut App, area: Rect) {
    let controls_width = 24;
    let controls_x = area.right().saturating_sub(controls_width);
    let mut x = area.x;
    for index in 0..app.terminals.len() {
        let title = format!(" {} ", index + 1);
        let width = text_width(&title);
        if x + width > controls_x.saturating_sub(2) {
            break;
        }
        let active = index == app.terminals.index();
        let style = if active {
            Style::default().fg(BG).bg(SELECTED).bold()
        } else {
            Style::default().fg(MUTED)
        };
        frame.render_widget(
            Paragraph::new(title).style(style),
            Rect::new(x, area.y, width, 1),
        );
        hit(
            app,
            Rect::new(x, area.y, width, 1),
            ClickAction::TerminalTab(index),
        );
        x += width + 1;
    }
    let can_close = app.terminals.len() > 1;
    let mut controls = vec![cap("+"), Span::raw(" "), muted("Ctrl+T")];
    if can_close {
        controls.extend([Span::raw("  "), cap("×"), Span::raw(" "), muted("Ctrl+W")]);
    }
    frame.render_widget(
        Line::from(controls),
        Rect::new(controls_x, area.y, controls_width, 1),
    );
    hit(
        app,
        Rect::new(controls_x, area.y, 11, 1),
        ClickAction::NewTerminal,
    );
    if can_close {
        hit(
            app,
            Rect::new(controls_x.saturating_add(11), area.y, 12, 1),
            ClickAction::CloseTerminal,
        );
    }
}

fn draw_settings(frame: &mut Frame, app: &mut App, area: Rect) {
    if area.height < 24 || area.width < 80 {
        draw_settings_compact(frame, app, area);
        return;
    }
    let stack_height = area.height.min(27);
    let stack = Rect::new(
        area.x,
        area.y + area.height.saturating_sub(stack_height) / 2,
        area.width,
        stack_height,
    );
    let rows = Layout::vertical([
        Constraint::Length(10),
        Constraint::Length(1),
        Constraint::Length(8),
        Constraint::Length(1),
        Constraint::Min(5),
    ])
    .split(stack);
    let columns = Layout::horizontal([
        Constraint::Percentage(62),
        Constraint::Length(2),
        Constraint::Fill(1),
    ])
    .split(rows[0]);
    draw_appearance(frame, app, columns[0]);
    draw_runtime(frame, app, columns[2]);
    draw_about(frame, app, rows[2]);
    draw_shortcuts(frame, rows[4]);
}

fn draw_appearance(frame: &mut Frame, app: &mut App, area: Rect) {
    let block = framed("外观");
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height < 3 || inner.width < 20 {
        return;
    }
    let status = if app.system.reduced_motion {
        "暂停"
    } else {
        "开启"
    };
    let value = if app.system.reduced_motion {
        Span::styled(
            format!(" {status} "),
            Style::default().fg(MUTED).bg(SURFACE),
        )
    } else {
        bright_cap(status)
    };
    frame.render_widget(
        Line::from(vec![
            label("动态曲线").bold(),
            Span::raw("    "),
            value,
            Span::raw("   "),
            muted("m 切换"),
        ]),
        Rect::new(inner.x, inner.y, inner.width, 1),
    );
    frame.render_widget(
        muted("低带宽环境可暂停历史曲线"),
        Rect::new(inner.x, inner.y + 2, inner.width, 1),
    );
    if inner.height >= 7 {
        frame.render_widget(
            Line::from(vec![
                label("配色").bold(),
                Span::raw("    "),
                muted("黑白灰"),
            ]),
            Rect::new(inner.x, inner.y + 4, inner.width, 1),
        );
        frame.render_widget(
            muted("固定单色 · 终端程序保留自身配色"),
            Rect::new(inner.x, inner.y + 6, inner.width, 1),
        );
    }
    hit(
        app,
        Rect::new(inner.x, inner.y, inner.width, 1),
        ClickAction::ToggleMotion,
    );
}

fn draw_runtime(frame: &mut Frame, app: &App, area: Rect) {
    let block = framed("运行时");
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height == 0 || inner.width == 0 {
        return;
    }
    let value_width = inner.width.saturating_sub(8);
    let lines = vec![
        info_line("版本", format!("v{}", env!("CARGO_PKG_VERSION"))),
        info_line("刷新", format!("{} 秒", app.refresh_interval.as_secs())),
        info_line(
            "工作区",
            shorten(&app.files.cwd().display().to_string(), value_width),
        ),
        info_line("平台", shorten(&app.system.os, value_width)),
        info_line("内核", shorten(&app.system.kernel, value_width)),
        info_line("主机", shorten(&app.system.hostname, value_width)),
    ];
    frame.render_widget(Paragraph::new(lines), inner);
}

fn draw_about(frame: &mut Frame, app: &mut App, area: Rect) {
    let block = framed("关于");
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height == 0 || inner.width == 0 {
        return;
    }
    let lines = vec![
        Line::from(vec![
            label("apkez").bold(),
            muted("  ·  为 Linux 服务器设计的终端工作区"),
        ]),
        Line::from(muted("Rust · 真彩色 · 无需 GPU · 支持 SSH")),
        Line::from(""),
        Line::from(muted("文件操作限制在 --root 工作区内")),
        Line::from(muted("终端会话拥有当前用户的系统权限")),
        Line::from(vec![cap("r"), Span::raw(" "), label("刷新系统信息")]),
    ];
    let content_height = inner.height.min(lines.len() as u16);
    frame.render_widget(
        Paragraph::new(lines),
        Rect::new(inner.x, inner.y, inner.width, content_height),
    );
    frame.render_widget(
        Paragraph::new(muted(format!("v{}", env!("CARGO_PKG_VERSION"))))
            .alignment(Alignment::Right),
        Rect::new(
            inner.x,
            inner.y + content_height.saturating_sub(1),
            inner.width,
            1,
        ),
    );
    if inner.height >= 6 {
        hit(
            app,
            Rect::new(inner.x, inner.y + 5, inner.width.min(24), 1),
            ClickAction::Refresh,
        );
    }
}

fn draw_shortcuts(frame: &mut Frame, area: Rect) {
    let block = framed("快捷操作");
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height == 0 || inner.width < 24 {
        return;
    }
    let rows = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(inner);
    let columns = Layout::horizontal([
        Constraint::Percentage(50),
        Constraint::Length(1),
        Constraint::Fill(1),
    ])
    .split(rows[0]);
    let left = vec![
        Line::from(vec![cap("F1–F5"), Span::raw(" "), muted("切换页面")]),
        Line::from(vec![cap("?"), Span::raw(" "), muted("打开帮助")]),
        Line::from(vec![cap("Esc"), Span::raw(" "), muted("关闭弹窗")]),
        Line::from(vec![cap("Ctrl+Q"), Span::raw(" "), muted("退出程序")]),
    ];
    let right = vec![
        Line::from(vec![cap("↑↓"), Span::raw(" "), muted("移动选择")]),
        Line::from(vec![cap("Enter"), Span::raw(" "), muted("打开 / 确认")]),
        Line::from(vec![cap("/"), Span::raw(" "), muted("筛选列表")]),
        Line::from(vec![cap("鼠标"), Span::raw(" "), muted("点击 / 滚轮")]),
    ];
    frame.render_widget(
        Paragraph::new(left),
        Rect::new(
            columns[0].x,
            columns[0].y,
            columns[0].width,
            columns[0].height,
        ),
    );
    frame.render_widget(
        Paragraph::new(right),
        Rect::new(
            columns[2].x,
            columns[2].y,
            columns[2].width,
            columns[2].height,
        ),
    );
    vertical_rule(frame, columns[1].x, rows[0].y, rows[0].height);
    frame.render_widget(
        muted("终端内按键直接发送至 Shell · 完整说明按 ?"),
        Rect::new(rows[1].x, rows[1].y, rows[1].width, 1),
    );
}

fn draw_settings_compact(frame: &mut Frame, app: &mut App, area: Rect) {
    let block = framed("偏好设置");
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height < 4 || inner.width < 20 {
        return;
    }
    let status = if app.system.reduced_motion {
        "暂停"
    } else {
        "开启"
    };
    let value = if app.system.reduced_motion {
        Span::styled(
            format!(" {status} "),
            Style::default().fg(MUTED).bg(SURFACE),
        )
    } else {
        bright_cap(status)
    };
    frame.render_widget(
        Line::from(vec![
            label("动态曲线").bold(),
            Span::raw("    "),
            value,
            Span::raw("   "),
            muted("m 切换"),
        ]),
        Rect::new(inner.x, inner.y, inner.width, 1),
    );
    frame.render_widget(
        muted("低带宽环境可暂停历史曲线"),
        Rect::new(inner.x, inner.y + 1, inner.width, 1),
    );
    if inner.height >= 6 {
        frame.render_widget(
            Line::from(vec![
                label("配色").bold(),
                Span::raw("    "),
                muted("黑白灰"),
            ]),
            Rect::new(inner.x, inner.y + 3, inner.width, 1),
        );
    }
    if inner.height >= 7 {
        let refresh = Line::from(vec![cap("r"), Span::raw(" "), label("刷新系统信息")]);
        frame.render_widget(refresh, Rect::new(inner.x, inner.y + 6, inner.width, 1));
        hit(
            app,
            Rect::new(inner.x, inner.y + 6, inner.width.min(24), 1),
            ClickAction::Refresh,
        );
    }
    hit(
        app,
        Rect::new(inner.x, inner.y, inner.width, 1),
        ClickAction::ToggleMotion,
    );
}

fn draw_actions(
    frame: &mut Frame,
    app: &mut App,
    area: Rect,
    actions: &[(&str, &str, &str, ClickAction)],
) {
    let gap = 3;
    let long_total: u16 = actions
        .iter()
        .map(|(key, long, _, _)| text_width(key) + 2 + 1 + text_width(long))
        .sum::<u16>()
        + gap * (actions.len() as u16 - 1);
    let use_long = long_total <= area.width;
    let gap = if use_long { gap } else { 1 };
    let mut x = area.x;
    for (key, long, short, action) in actions {
        let title = if use_long { *long } else { *short };
        let item_width = text_width(key) + 2 + 1 + text_width(title);
        if x + item_width > area.right() {
            break;
        }
        frame.render_widget(
            Line::from(vec![cap(key), Span::raw(" "), label(title)]),
            Rect::new(x, area.y, item_width, 1),
        );
        hit(app, Rect::new(x, area.y, item_width, 1), *action);
        x += item_width + gap;
    }
}

fn draw_footer(frame: &mut Frame, app: &App, area: Rect) {
    frame.render_widget(
        Paragraph::new("─".repeat(area.width as usize)).fg(BORDER),
        Rect::new(area.x, area.y, area.width, 1),
    );
    let hints: &[(&str, &str)] = match app.page {
        Page::Overview => &[("F1–F7", "导航"), ("?", "帮助"), ("Ctrl+K", "命令面板")],
        Page::Files => &[
            ("↑↓", "选择"),
            ("Enter", "打开"),
            ("Backspace", "上级"),
            ("/", "筛选"),
        ],
        Page::Processes => &[("↑↓", "选择"), ("/", "筛选"), ("s", "排序"), ("x", "终止")],
        Page::Services => &[("↑↓", "选择"), ("Enter", "查看日志"), ("/", "筛选")],
        Page::Logs => &[("↑↓", "滚动"), ("/", "筛选"), ("J", "journalctl")],
        Page::Terminal => &[("Ctrl+N/P", "切换标签"), ("Ctrl+]", "返回桌面")],
        Page::Settings => &[("m", "动态曲线"), ("r", "刷新"), ("Ctrl+K", "命令面板")],
    };
    let right_width = 14;
    let left_limit = area.width.saturating_sub(right_width + 2);
    let mut x = area.x;
    let mut spans = Vec::new();
    for (key, title) in hints {
        let item_width = text_width(key) + 2 + 1 + text_width(title);
        if x + item_width > area.x + left_limit {
            break;
        }
        spans.push(cap(key));
        spans.push(Span::raw(" "));
        spans.push(muted(*title));
        spans.push(Span::raw("   "));
        x += item_width + 3;
    }
    frame.render_widget(
        Line::from(spans),
        Rect::new(area.x, area.y + 1, left_limit, 1),
    );
    let right = Line::from(vec![cap("Ctrl+Q"), Span::raw(" "), muted("退出")]);
    frame.render_widget(
        right,
        Rect::new(
            area.right().saturating_sub(right_width),
            area.y + 1,
            right_width,
            1,
        ),
    );
}

fn draw_toast(frame: &mut Frame, app: &App, area: Rect) {
    let Some((message, _, error)) = &app.toast else {
        return;
    };
    let text = format!("{}  {}", if *error { "!" } else { "✓" }, message);
    let width = (text_width(&text) + 4).min(area.width);
    if width == 0 {
        return;
    }
    let toast = Rect::new(
        area.right().saturating_sub(width),
        area.bottom().saturating_sub(3),
        width,
        1,
    );
    frame.render_widget(Clear, toast);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw(" "),
            Span::styled(text, Style::default().fg(if *error { ERROR } else { TEXT })),
            Span::raw(" "),
        ]))
        .style(Style::default().bg(SURFACE)),
        toast,
    );
}

fn draw_modal(frame: &mut Frame, app: &mut App) {
    let Some(modal) = app.modal.clone() else {
        return;
    };
    frame.render_widget(Dim, frame.area());
    let (area, title) = match &modal {
        Modal::Help => (centered(frame.area(), 78, 26), "帮助".to_owned()),
        Modal::Palette { .. } => (centered(frame.area(), 68, 22), "命令面板".to_owned()),
        Modal::Input { title, .. } => (centered(frame.area(), 54, 9), title.clone()),
        Modal::ConfirmDelete => (centered(frame.area(), 54, 10), "删除".to_owned()),
        Modal::ConfirmKill(_, _) => (centered(frame.area(), 54, 10), "终止进程".to_owned()),
        Modal::ConfirmService(name, action) => (
            centered(frame.area(), 58, 11),
            format!("{}服务 · {}", action_label(*action), name),
        ),
        Modal::Preview { name, .. } => (
            centered(frame.area(), 90, frame.area().height.saturating_sub(4)),
            name.clone(),
        ),
    };
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Rgb(105, 105, 105)))
        .style(Style::default().bg(SURFACE))
        .title(Line::from(vec![
            Span::raw(" "),
            Span::styled(title, Style::default().fg(TEXT).bold()),
            Span::raw(" "),
        ]));
    let inner = area.inner(Margin::new(2, 1));
    frame.render_widget(Clear, area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    match modal {
        Modal::Help => {
            let full = help_lines(false);
            let compact = help_lines(true);
            let lines = if inner.height as usize >= full.len() {
                full
            } else {
                compact
            };
            frame.render_widget(Paragraph::new(lines), inner);
            hit(app, inner, ClickAction::CloseModal);
        }
        Modal::Palette { query, selected } => {
            let results = palette::search(&query);
            let prompt_line = if query.is_empty() {
                Line::from(vec![
                    Span::styled("❯ ", Style::default().fg(MUTED)),
                    muted("输入以搜索页面、操作或命令…"),
                ])
            } else {
                Line::from(vec![
                    Span::styled("❯ ", Style::default().fg(MUTED)),
                    label(query.clone()),
                ])
            };
            frame.render_widget(prompt_line, Rect::new(inner.x, inner.y, inner.width, 1));
            let prompt = text_width("❯ ");
            frame.set_cursor_position((
                inner
                    .x
                    .saturating_add(prompt + text_width(&query))
                    .min(inner.right().saturating_sub(1)),
                inner.y,
            ));
            if inner.height > 3 {
                frame.render_widget(
                    Paragraph::new("─".repeat(inner.width as usize)).fg(BORDER),
                    Rect::new(inner.x, inner.y + 1, inner.width, 1),
                );
                let limit = inner.height.saturating_sub(3) as usize;
                let rows: Vec<Line> = results
                    .iter()
                    .take(limit)
                    .enumerate()
                    .map(|(index, command)| {
                        let style = if index == selected {
                            Style::default().fg(BG).bg(SELECTED)
                        } else {
                            Style::default().fg(TEXT)
                        };
                        Line::from(vec![
                            Span::styled(format!(" {:<12} ", command.name), style),
                            muted(command.description.to_owned()),
                        ])
                    })
                    .collect();
                frame.render_widget(
                    Paragraph::new(rows),
                    Rect::new(inner.x, inner.y + 2, inner.width, limit as u16),
                );
                for index in 0..results.len().min(limit) {
                    hit(
                        app,
                        Rect::new(inner.x, inner.y + 2 + index as u16, inner.width, 1),
                        ClickAction::PaletteRow(index),
                    );
                }
            }
            if inner.height >= 2 {
                frame.render_widget(
                    Line::from(vec![
                        cap("↑↓"),
                        Span::raw(" "),
                        muted("选择"),
                        Span::raw("   "),
                        cap("Enter"),
                        Span::raw(" "),
                        muted("执行"),
                        Span::raw("   "),
                        cap("Esc"),
                        Span::raw(" "),
                        muted("取消"),
                    ]),
                    Rect::new(inner.x, inner.bottom().saturating_sub(1), inner.width, 1),
                );
            }
        }
        Modal::ConfirmService(name, action) => {
            draw_confirm(
                frame,
                inner,
                &format!("{}服务 {name}？", action_label(action)),
                "操作将通过内置终端执行 sudo systemctl，需要时在终端中输入密码。",
            );
        }
        Modal::Input { value, .. } => {
            frame.render_widget(muted("名称"), Rect::new(inner.x, inner.y, inner.width, 1));
            if inner.height > 2 {
                frame.render_widget(
                    Line::from(vec![
                        Span::styled("❯ ", Style::default().fg(MUTED)),
                        label(value.clone()),
                    ]),
                    Rect::new(inner.x, inner.y + 2, inner.width, 1),
                );
                let prompt = text_width("❯ ");
                let cursor = prompt.saturating_add(text_width(&value));
                frame.set_cursor_position((
                    inner
                        .x
                        .saturating_add(cursor)
                        .min(inner.right().saturating_sub(1)),
                    inner.y + 2,
                ));
            }
            frame.render_widget(
                Line::from(vec![
                    cap("Enter"),
                    Span::raw(" "),
                    muted("确认"),
                    Span::raw("   "),
                    cap("Esc"),
                    Span::raw(" "),
                    muted("取消"),
                ]),
                Rect::new(inner.x, inner.bottom().saturating_sub(1), inner.width, 1),
            );
        }
        Modal::ConfirmDelete => {
            let name = app
                .files
                .selected_entry()
                .map(|entry| entry.name.clone())
                .unwrap_or_else(|| "选中项目".to_owned());
            draw_confirm(
                frame,
                inner,
                &format!("删除 {name}？"),
                "仅支持删除空目录，此操作无法撤销。",
            );
        }
        Modal::ConfirmKill(pid, name) => {
            draw_confirm(
                frame,
                inner,
                &format!("终止 {name}（PID {pid}）？"),
                "未保存的数据可能丢失。",
            );
        }
        Modal::Preview {
            content, offset, ..
        } => {
            frame.render_widget(
                muted("↑↓ / 滚轮滚动 · Esc 关闭"),
                Rect::new(inner.x, inner.y, inner.width, 1),
            );
            if inner.height > 2 {
                frame.render_widget(
                    Paragraph::new(content)
                        .scroll((offset as u16, 0))
                        .wrap(Wrap { trim: false }),
                    Rect::new(inner.x, inner.y + 2, inner.width, inner.height - 2),
                );
            }
        }
    }
}

fn help_lines(compact: bool) -> Vec<Line<'static>> {
    if compact {
        vec![
            Line::from(label("导航").bold()),
            Line::from(muted("F1–F7 页面   Ctrl+K 命令面板   Ctrl+Q 退出")),
            Line::from(""),
            Line::from(label("文件 / 进程").bold()),
            Line::from(muted("↑↓ 选择   Enter 打开   / 筛选")),
            Line::from(muted("n/N 新建   r 重命名   d 删除   x 终止")),
            Line::from(""),
            Line::from(label("服务 / 日志").bold()),
            Line::from(muted("s 启动   x 停止   r 重启   R 刷新")),
            Line::from(muted("日志：f 跟随   / 筛选   o 打开文件")),
            Line::from(""),
            Line::from(label("终端").bold()),
            Line::from(muted("Ctrl+T 新建   Ctrl+N/P 切换   Ctrl+W 关闭")),
            Line::from(muted("Ctrl+] 返回桌面   其余按键发送至 Shell")),
            Line::from(""),
            Line::from(vec![cap("Esc"), Span::raw(" "), muted("关闭")]),
        ]
    } else {
        vec![
            Line::from(label("导航").bold()),
            Line::from(muted(
                "F1–F7  切换页面      Ctrl+K  命令面板      ?  帮助      Ctrl+Q  退出",
            )),
            Line::from(""),
            Line::from(label("文件").bold()),
            Line::from(muted(
                "↑↓ / j k   移动选择      Enter  打开      Backspace  上级",
            )),
            Line::from(muted(
                "/  筛选      n / N  新建文件 / 目录      r  重命名      d  删除",
            )),
            Line::from(""),
            Line::from(label("进程").bold()),
            Line::from(muted(
                "s  切换排序      x  终止进程      /  筛选      r  刷新",
            )),
            Line::from(""),
            Line::from(label("服务").bold()),
            Line::from(muted(
                "s  启动      x  停止      r  重启      R  刷新      Enter  查看日志",
            )),
            Line::from(muted(
                "启停命令在内置终端中执行 sudo systemctl，密码在终端内输入",
            )),
            Line::from(""),
            Line::from(label("日志").bold()),
            Line::from(muted(
                "f  实时跟随      /  筛选      o  打开日志文件      J  读取 journalctl",
            )),
            Line::from(""),
            Line::from(label("终端").bold()),
            Line::from(muted(
                "Ctrl+T  新建标签      Ctrl+N / Ctrl+P  切换      Ctrl+W  关闭",
            )),
            Line::from(muted(
                "Ctrl+]  返回桌面，其余按键发送至 Shell，Shell 退出后 Enter 重启",
            )),
            Line::from(""),
            Line::from(vec![cap("Esc"), Span::raw(" "), muted("关闭")]),
        ]
    }
}

fn draw_confirm(frame: &mut Frame, area: Rect, message: &str, warning: &str) {
    frame.render_widget(
        Paragraph::new(label(message.to_owned()).bold()).wrap(Wrap { trim: true }),
        Rect::new(area.x, area.y, area.width, 2),
    );
    if area.height > 3 {
        frame.render_widget(
            Paragraph::new(muted(warning.to_owned())).wrap(Wrap { trim: true }),
            Rect::new(area.x, area.y + 3, area.width, 2),
        );
    }
    if area.height > 5 {
        frame.render_widget(
            Line::from(vec![
                cap("y"),
                Span::raw(" "),
                muted("确认"),
                Span::raw("    "),
                cap("Esc"),
                Span::raw(" "),
                muted("取消"),
            ]),
            Rect::new(area.x, area.bottom().saturating_sub(2), area.width, 1),
        );
    }
}

fn vertical_rule(frame: &mut Frame, x: u16, y: u16, height: u16) {
    for offset in 0..height {
        frame.render_widget(
            Paragraph::new(Span::styled("│", Style::default().fg(BORDER))),
            Rect::new(x, y + offset, 1, 1),
        );
    }
}

fn scroll_selection(selected: usize, offset: &mut usize, visible: usize) {
    if selected < *offset {
        *offset = selected;
    }
    if selected >= *offset + visible {
        *offset = selected + 1 - visible;
    }
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend};

    use super::draw;
    use crate::app::{App, Modal, Page};
    use crate::test_support::{Workspace, duration};

    fn rendered(app: &mut App, width: u16, height: u16) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| draw(frame, app)).unwrap();
        terminal
            .backend()
            .buffer()
            .content()
            .chunks(width as usize)
            .map(|row| {
                row.iter()
                    .filter(|cell| !cell.symbol().is_empty())
                    .map(|cell| cell.symbol())
                    .collect::<String>()
                    .chars()
                    .filter(|character| !character.is_whitespace())
                    .collect::<String>()
                    + "\n"
            })
            .collect()
    }

    #[test]
    fn renders_every_page_and_help() {
        let workspace = Workspace::new("render");
        let mut app = App::new(workspace.path(), duration()).unwrap();
        for page in Page::ALL {
            app.navigate(page);
            let output = rendered(&mut app, 120, 38);
            assert!(output.contains(page.title()), "{page:?}\n{output}");
            assert!(output.contains("apkez"));
        }
        app.modal = Some(Modal::Help);
        assert!(rendered(&mut app, 120, 38).contains("帮助"));
    }

    #[test]
    fn renders_small_terminal_fallback() {
        let workspace = Workspace::new("small");
        let mut app = App::new(workspace.path(), duration()).unwrap();
        let output = rendered(&mut app, 60, 18);
        assert!(output.contains("扩大终端"));
        assert!(output.contains("Ctrl+Q"));
    }

    #[test]
    fn renders_modal_and_file_content() {
        let workspace = Workspace::new("preview-render");
        let root = workspace.path();
        std::fs::write(root.join("note.txt"), "line one\nline two").unwrap();
        let mut app = App::new(root, duration()).unwrap();
        app.navigate(Page::Files);
        app.handle_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        ));
        let output = rendered(&mut app, 120, 38);
        assert!(output.contains("Esc关闭"));
        assert!(output.contains("lineone"), "{output}");
    }
}
