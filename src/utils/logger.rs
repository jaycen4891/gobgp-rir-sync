use std::fs::{self, File, OpenOptions};
use std::io::{LineWriter, Write};
use std::path::Path;
use std::sync::Mutex;

use crate::config::Settings;

/// 自定义日志器 — 每次写入后立即 flush
struct FileLogger {
    writer: Mutex<LineWriter<File>>,
}

impl log::Log for FileLogger {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &log::Record) {
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
        let level = record.level();
        let msg = record.args();

        if let Ok(mut writer) = self.writer.lock() {
            let _ = writeln!(writer, "{} [{}] {}", now, level, msg);
            let _ = writer.flush();
        }
    }

    fn flush(&self) {
        if let Ok(mut writer) = self.writer.lock() {
            let _ = writer.flush();
        }
    }
}

/// 日志初始化器
pub struct Logger;

impl Logger {
    /// 初始化日志系统
    pub fn setup(settings: &Settings) -> anyhow::Result<()> {
        let log_file = &settings.log_file;

        // 确保日志目录存在
        if let Some(parent) = Path::new(log_file).parent() {
            fs::create_dir_all(parent)?;
        }

        // 以追加模式打开文件（不 truncate，保留历史日志）
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_file)
            .map_err(|e| anyhow::anyhow!("无法打开日志文件 {}: {}", log_file, e))?;

        let writer = LineWriter::new(file);

        // 注册自定义 logger
        let logger = Box::new(FileLogger {
            writer: Mutex::new(writer),
        });
        log::set_boxed_logger(logger).map_err(|e| anyhow::anyhow!("注册日志器失败: {}", e))?;
        log::set_max_level(log::LevelFilter::Info);

        log::info!("日志系统初始化完成");

        Ok(())
    }
}
