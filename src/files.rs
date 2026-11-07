use std::cmp::Ordering;
use std::fs;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};

#[derive(Debug, Clone)]
pub struct FileEntry {
    pub name: String,
    pub path: PathBuf,
    pub directory: bool,
    pub size: u64,
}

#[derive(Debug)]
pub struct FileManager {
    root: PathBuf,
    cwd: PathBuf,
    entries: Vec<FileEntry>,
    pub selected: usize,
    pub offset: usize,
    pub filter: String,
    pub show_hidden: bool,
}

impl FileManager {
    pub fn new(root: PathBuf) -> Result<Self> {
        let root = root.canonicalize().context("工作目录不存在或无法访问")?;
        if !root.is_dir() {
            bail!("工作目录必须是一个目录");
        }
        let mut manager = Self {
            cwd: root.clone(),
            root,
            entries: Vec::new(),
            selected: 0,
            offset: 0,
            filter: String::new(),
            show_hidden: false,
        };
        manager.reload()?;
        Ok(manager)
    }

    pub fn cwd(&self) -> &Path {
        &self.cwd
    }

    pub fn entries(&self) -> &[FileEntry] {
        &self.entries
    }

    pub fn relative_cwd(&self) -> String {
        match self.cwd.strip_prefix(&self.root) {
            Ok(path) if path.as_os_str().is_empty() => "/".to_owned(),
            Ok(path) => format!("/{}", path.display()),
            Err(_) => "/".to_owned(),
        }
    }

    pub fn selected_entry(&self) -> Option<&FileEntry> {
        self.entries.get(self.selected)
    }

    pub fn next(&mut self) {
        if !self.entries.is_empty() {
            self.selected = (self.selected + 1).min(self.entries.len() - 1);
        }
    }

    pub fn previous(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    pub fn move_by(&mut self, delta: isize) {
        self.selected = self
            .selected
            .saturating_add_signed(delta)
            .min(self.entries.len().saturating_sub(1));
    }

    pub fn select(&mut self, index: usize) {
        if index < self.entries.len() {
            self.selected = index;
        }
    }

    pub fn page(&mut self, index: usize) {
        self.selected = index.min(self.entries.len().saturating_sub(1));
    }

    pub fn reload(&mut self) -> Result<()> {
        let mut entries = Vec::new();
        for item in fs::read_dir(&self.cwd)
            .with_context(|| format!("无法读取目录 {}", self.cwd.display()))?
        {
            let item = item?;
            let path = item.path();
            let metadata = fs::symlink_metadata(&path)?;
            let name = item.file_name().to_string_lossy().into_owned();
            if !self.show_hidden && name.starts_with('.') {
                continue;
            }
            if !self.filter.is_empty() && !name.to_lowercase().contains(&self.filter.to_lowercase())
            {
                continue;
            }
            entries.push(FileEntry {
                name,
                path,
                directory: metadata.is_dir(),
                size: if metadata.is_file() {
                    metadata.len()
                } else {
                    0
                },
            });
        }
        entries.sort_by(|a, b| match b.directory.cmp(&a.directory) {
            Ordering::Equal => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            other => other,
        });
        self.entries = entries;
        self.selected = self.selected.min(self.entries.len().saturating_sub(1));
        Ok(())
    }

    fn safe_current_name(&self, name: &str) -> Result<PathBuf> {
        let path = Path::new(name);
        if path.components().count() != 1
            || !matches!(path.components().next(), Some(Component::Normal(_)))
        {
            bail!("名称必须是当前目录中的单个文件名");
        }
        Ok(self.cwd.join(path))
    }

    pub fn create_file(&mut self, name: &str) -> Result<PathBuf> {
        let path = self.safe_current_name(name)?;
        if path.exists() {
            bail!("文件或目录已存在");
        }
        fs::write(&path, []).context("无法创建文件")?;
        self.reload()?;
        Ok(path)
    }

    pub fn create_directory(&mut self, name: &str) -> Result<PathBuf> {
        let path = self.safe_current_name(name)?;
        if path.exists() {
            bail!("文件或目录已存在");
        }
        fs::create_dir(&path).context("无法创建目录")?;
        self.reload()?;
        Ok(path)
    }

    pub fn rename_selected(&mut self, name: &str) -> Result<()> {
        let target = self.safe_current_name(name)?;
        let entry = self
            .selected_entry()
            .ok_or_else(|| anyhow!("请先选择文件"))?;
        if target.exists() {
            bail!("目标名称已存在");
        }
        fs::rename(&entry.path, target).context("重命名失败")?;
        self.reload()
    }

    pub fn delete_selected(&mut self) -> Result<()> {
        let entry = self
            .selected_entry()
            .ok_or_else(|| anyhow!("请先选择文件"))?;
        if entry.directory {
            fs::remove_dir(&entry.path).context("删除目录失败，只能删除空目录")?;
        } else {
            fs::remove_file(&entry.path).context("删除文件失败")?;
        }
        self.reload()
    }

    pub fn read_selected(&self) -> Result<String> {
        let entry = self
            .selected_entry()
            .ok_or_else(|| anyhow!("请先选择文件"))?;
        if entry.directory {
            bail!("目录无法作为文本打开");
        }
        let resolved = entry.path.canonicalize().context("无法打开文件")?;
        if !resolved.starts_with(&self.root) {
            bail!("文件链接指向工作目录之外，已阻止访问");
        }
        if entry.size > 1_048_576 {
            bail!("为保持界面流畅，仅支持预览 1 MiB 以内的文件");
        }
        let bytes = fs::read(&resolved).context("无法读取文件")?;
        if bytes.contains(&0) {
            bail!("这是二进制文件，无法作为文本预览");
        }
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }

    pub fn enter_selected(&mut self) -> Result<()> {
        let Some(entry) = self.selected_entry() else {
            return Ok(());
        };
        if !entry.directory {
            return Ok(());
        }
        let target = entry.path.canonicalize().context("无法打开目录")?;
        if !target.starts_with(&self.root) {
            bail!("目录链接指向工作目录之外，已阻止访问");
        }
        self.cwd = target;
        self.selected = 0;
        self.offset = 0;
        self.filter.clear();
        self.reload()
    }

    pub fn up(&mut self) -> Result<()> {
        if self.cwd == self.root {
            return Ok(());
        }
        let previous = self.cwd.clone();
        if let Some(parent) = self.cwd.parent() {
            self.cwd = parent.to_path_buf();
            self.filter.clear();
            self.reload()?;
            if let Some(index) = self.entries.iter().position(|entry| entry.path == previous) {
                self.selected = index;
            }
        }
        Ok(())
    }

    pub fn toggle_hidden(&mut self) -> Result<()> {
        self.show_hidden = !self.show_hidden;
        self.selected = 0;
        self.reload()
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::FileManager;
    use crate::test_support::Workspace;

    #[test]
    fn creates_and_navigates_inside_workspace() {
        let workspace = Workspace::new("files");
        let mut files = FileManager::new(workspace.path()).unwrap();
        files.create_directory("project").unwrap();
        files.create_file("note.txt").unwrap();
        assert_eq!(files.entries().len(), 2);
        assert_eq!(files.selected_entry().unwrap().name, "project");

        files.enter_selected().unwrap();
        assert_eq!(files.relative_cwd(), "/project");
        files.create_file("inside.txt").unwrap();
        files.up().unwrap();
        assert_eq!(files.relative_cwd(), "/");
        assert_eq!(files.selected_entry().unwrap().name, "project");
    }

    #[test]
    fn rejects_unsafe_names() {
        let workspace = Workspace::new("unsafe");
        let mut files = FileManager::new(workspace.path()).unwrap();
        assert!(files.create_file("../outside").is_err());
        assert!(files.create_file("/tmp/outside").is_err());
        assert!(files.create_file("").is_err());
        assert!(files.create_directory("nested/child").is_err());
    }

    #[test]
    fn filters_and_toggles_hidden_files() {
        let workspace = Workspace::new("filter");
        let root = workspace.path();
        fs::write(root.join("visible.txt"), "visible").unwrap();
        fs::write(root.join(".secret"), "hidden").unwrap();
        let mut files = FileManager::new(root).unwrap();
        assert_eq!(files.entries().len(), 1);

        files.toggle_hidden().unwrap();
        assert_eq!(files.entries().len(), 2);
        files.filter = "secret".to_owned();
        files.reload().unwrap();
        assert_eq!(files.entries()[0].name, ".secret");
    }

    #[test]
    fn previews_text_and_blocks_external_symlink() {
        let workspace = Workspace::new("preview");
        let root = workspace.path();
        fs::write(root.join("hello.txt"), "你好，终端").unwrap();
        let mut files = FileManager::new(root.clone()).unwrap();
        assert_eq!(files.read_selected().unwrap(), "你好，终端");

        #[cfg(unix)]
        {
            std::os::unix::fs::symlink("/etc/hostname", root.join("external")).unwrap();
            files.reload().unwrap();
            let index = files
                .entries()
                .iter()
                .position(|entry| entry.name == "external")
                .unwrap();
            files.select(index);
            assert!(files.read_selected().is_err());
        }
    }

    #[test]
    fn deletes_only_empty_directories() {
        let workspace = Workspace::new("delete");
        let root = workspace.path();
        fs::create_dir(root.join("folder")).unwrap();
        fs::write(root.join("folder/file"), "data").unwrap();
        let mut files = FileManager::new(root).unwrap();
        assert!(files.delete_selected().is_err());
        fs::remove_file(files.selected_entry().unwrap().path.join("file")).unwrap();
        files.delete_selected().unwrap();
        assert!(files.entries().is_empty());
    }
}
