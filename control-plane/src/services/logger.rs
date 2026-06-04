use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use tracing::info;
use tracing_subscriber::EnvFilter;

const DEFAULT_LOG_DIR: &str = "/var/log/vectoros";
const DEFAULT_LOG_FILE: &str = "control-plane.log";
const MAX_LOG_SIZE_BYTES: u64 = 50 * 1024 * 1024; // 50 MB
const LOG_RETENTION_COUNT: usize = 5;

/// Manages structured logging with file output and rotation.
pub struct Logger {
    log_dir: PathBuf,
    log_file: PathBuf,
    writer: Mutex<fs::File>,
}

impl Logger {
    /// Initialize the logger with file and stdout output.
    /// Returns a Logger instance for manual log operations.
    pub fn init(log_dir: Option<&str>, env_filter: Option<&str>) -> anyhow::Result<Self> {
        let dir = PathBuf::from(log_dir.unwrap_or(DEFAULT_LOG_DIR));
        fs::create_dir_all(&dir)?;

        let log_file = dir.join(DEFAULT_LOG_FILE);

        // Set up tracing subscriber with env filter
        let filter = env_filter
            .unwrap_or("info");
        let env_filter = EnvFilter::try_new(filter).unwrap_or_else(|_| EnvFilter::new("info"));

        tracing_subscriber::fmt()
            .with_env_filter(env_filter)
            .with_target(true)
            .with_file(true)
            .with_line_number(true)
            .init();

        // Open log file for appending
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_file)?;

        info!("Logger initialized, writing to {:?}", log_file);

        Ok(Self {
            log_dir: dir,
            log_file: log_file.clone(),
            writer: Mutex::new(file),
        })
    }

    /// Write a structured log entry to the log file.
    pub fn write_log(&self, level: &str, module: &str, message: &str) -> anyhow::Result<()> {
        let timestamp = chrono_now();
        let entry = format!("{} [{}] [{}] {}\n", timestamp, level.to_uppercase(), module, message);

        let mut writer = self.writer.lock().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
        writer.write_all(entry.as_bytes())?;
        writer.flush()?;

        // Check if rotation is needed
        drop(writer);
        self.maybe_rotate()?;

        Ok(())
    }

    /// Rotate log files when the current log exceeds MAX_LOG_SIZE_BYTES.
    fn maybe_rotate(&self) -> anyhow::Result<()> {
        let metadata = fs::metadata(&self.log_file)?;
        if metadata.len() < MAX_LOG_SIZE_BYTES {
            return Ok(());
        }

        self.rotate()
    }

    /// Rotate the current log file, creating a numbered backup.
    fn rotate(&self) -> anyhow::Result<()> {
        // Remove oldest backup if we have too many
        self.cleanup_old_backups()?;

        let timestamp = chrono_now().replace([':', ' '], "");
        let backup_name = format!("{}.{}.bak", DEFAULT_LOG_FILE, timestamp);
        let backup_path = self.log_dir.join(&backup_name);

        fs::rename(&self.log_file, &backup_path)?;

        // Create new empty log file
        let new_file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&self.log_file)?;

        let mut writer = self.writer.lock().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
        *writer = new_file;

        info!("Log rotated to {:?}", backup_path);

        Ok(())
    }

    /// Remove oldest backup files beyond the retention count.
    fn cleanup_old_backups(&self) -> anyhow::Result<()> {
        let mut backups: Vec<PathBuf> = fs::read_dir(&self.log_dir)?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .map(|n| n.to_string_lossy().ends_with(".bak"))
                    .unwrap_or(false)
            })
            .collect();

        backups.sort();

        while backups.len() >= LOG_RETENTION_COUNT {
            if let Some(oldest) = backups.first() {
                fs::remove_file(oldest)?;
                backups.remove(0);
            }
        }

        Ok(())
    }

    /// Read the last N lines from the log file.
    pub fn read_recent(&self, lines: usize) -> anyhow::Result<Vec<String>> {
        let content = fs::read_to_string(&self.log_file)?;
        let all_lines: Vec<&str> = content.lines().collect();
        let start = if all_lines.len() > lines {
            all_lines.len() - lines
        } else {
            0
        };
        Ok(all_lines[start..].iter().map(|s| s.to_string()).collect())
    }

    /// Clear the log file (rotate first).
    pub fn clear(&self) -> anyhow::Result<()> {
        self.rotate()
    }
}

/// Simple timestamp without requiring the chrono crate at module level.
fn chrono_now() -> String {
    use std::time::SystemTime;
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();

    // Convert to broken-down time (UTC)
    let days = secs / 86400;
    let remaining = secs % 86400;
    let hours = remaining / 3600;
    let minutes = (remaining % 3600) / 60;
    let seconds = remaining % 60;

    // Approximate date calculation
    let mut year = 1970;
    let mut day_of_year = days as i64;
    loop {
        let days_in_year = if is_leap(year) { 366 } else { 365 };
        if day_of_year < days_in_year as i64 {
            break;
        }
        day_of_year -= days_in_year as i64;
        year += 1;
    }

    let month_days = if is_leap(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut month = 1u32;
    let mut day = day_of_year as u32 + 1;
    for (i, &md) in month_days.iter().enumerate() {
        if day <= md {
            month = (i + 1) as u32;
            break;
        }
        day -= md;
    }

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
        year, month, day, hours, minutes, seconds
    )
}

fn is_leap(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}
