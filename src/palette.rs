use crate::app::Page;

#[derive(Clone)]
pub struct Command {
    pub name: &'static str,
    pub description: &'static str,
    pub action: CommandAction,
}

#[derive(Clone, Copy)]
pub enum CommandAction {
    Navigate(Page),
    NewTerminal,
    CloseTerminal,
    NextTerminal,
    PreviousTerminal,
    RefreshServices,
    ToggleMotion,
    Help,
}

pub fn all() -> Vec<Command> {
    vec![
        Command {
            name: "系统概览",
            description: "CPU、内存、磁盘和系统状态",
            action: CommandAction::Navigate(Page::Overview),
        },
        Command {
            name: "文件管理",
            description: "浏览、预览和管理工作区文件",
            action: CommandAction::Navigate(Page::Files),
        },
        Command {
            name: "进程管理",
            description: "查看和终止系统进程",
            action: CommandAction::Navigate(Page::Processes),
        },
        Command {
            name: "服务管理",
            description: "查看服务，使用 sudo 启停服务",
            action: CommandAction::Navigate(Page::Services),
        },
        Command {
            name: "日志查看",
            description: "系统日志实时跟随与筛选",
            action: CommandAction::Navigate(Page::Logs),
        },
        Command {
            name: "打开终端",
            description: "进入当前交互式 Shell",
            action: CommandAction::Navigate(Page::Terminal),
        },
        Command {
            name: "新建终端",
            description: "创建一个新的 Shell 标签",
            action: CommandAction::NewTerminal,
        },
        Command {
            name: "关闭终端",
            description: "关闭当前 Shell 标签",
            action: CommandAction::CloseTerminal,
        },
        Command {
            name: "下一个终端",
            description: "切换到下一个 Shell 标签",
            action: CommandAction::NextTerminal,
        },
        Command {
            name: "上一个终端",
            description: "切换到上一个 Shell 标签",
            action: CommandAction::PreviousTerminal,
        },
        Command {
            name: "偏好设置",
            description: "配置动态曲线和查看快捷键",
            action: CommandAction::Navigate(Page::Settings),
        },
        Command {
            name: "刷新服务",
            description: "重新读取 systemd 服务列表",
            action: CommandAction::RefreshServices,
        },
        Command {
            name: "暂停动态曲线",
            description: "切换系统活动的动态采样",
            action: CommandAction::ToggleMotion,
        },
        Command {
            name: "帮助",
            description: "查看全部快捷键",
            action: CommandAction::Help,
        },
    ]
}

pub fn search(query: &str) -> Vec<Command> {
    let query = query.to_lowercase();
    all()
        .into_iter()
        .filter(|command| {
            query.is_empty()
                || command.name.to_lowercase().contains(&query)
                || command.description.to_lowercase().contains(&query)
                || command.name.contains(&query)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn filters_actions() {
        let result = super::search("终端");
        assert!(result.len() >= 3);
        assert!(result.iter().any(|command| command.name == "新建终端"));
    }
}
