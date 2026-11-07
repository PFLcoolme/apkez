use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub struct Workspace {
    path: PathBuf,
}

impl Workspace {
    pub fn new(label: &str) -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("apkez-{label}-{}-{unique}", std::process::id()));
        fs::create_dir(&path).expect("create temporary workspace");
        Self { path }
    }

    pub fn path(&self) -> PathBuf {
        self.path.clone()
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

pub fn duration() -> Duration {
    Duration::from_secs(60)
}
