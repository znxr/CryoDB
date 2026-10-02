use crate::constants::{TRANSFER_PROGRESS_MIN_INTERVAL_MS, TRANSFER_PROGRESS_MIN_STEP};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TransferStage {
    Configure,
    Running,
    Completed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InnoDbVersion {
    Default,
    V57,
    V80,
}

impl InnoDbVersion {
    pub(crate) const ALL: [InnoDbVersion; 3] = [
        InnoDbVersion::Default,
        InnoDbVersion::V57,
        InnoDbVersion::V80,
    ];
}

impl std::fmt::Display for InnoDbVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InnoDbVersion::Default => f.write_str("Default"),
            InnoDbVersion::V57 => f.write_str("5.7"),
            InnoDbVersion::V80 => f.write_str("8.0"),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) enum TransferError {
    Cancelled,
    Failed(String),
}

#[derive(Debug, Clone)]
pub(crate) struct ImportSummary {
    pub(crate) statements: usize,
    pub(crate) duration: Duration,
}

#[derive(Debug, Clone)]
pub(crate) struct ExportSummary {
    pub(crate) tables: usize,
    pub(crate) rows: usize,
    pub(crate) duration: Duration,
    pub(crate) bytes_written: u64,
}

#[derive(Debug, Clone)]
pub(crate) struct ImportOptions {
    pub(crate) create_database: bool,
    pub(crate) drop_existing: bool,
    pub(crate) disable_foreign_keys: bool,
    pub(crate) use_transaction: bool,
    pub(crate) innodb_version: InnoDbVersion,
}

#[derive(Debug, Clone)]
pub(crate) struct ExportOptions {
    pub(crate) include_drop: bool,
    pub(crate) include_create: bool,
    pub(crate) include_inserts: bool,
    pub(crate) use_values: bool,
    pub(crate) include_routines: bool,
}

pub(crate) struct ProgressThrottle {
    pub(crate) last_progress: f32,
    pub(crate) last_emit: Instant,
}

impl ProgressThrottle {
    pub(crate) fn new() -> Self {
        Self {
            last_progress: 0.0,
            last_emit: Instant::now(),
        }
    }

    pub(crate) fn should_emit(&mut self, progress: f32) -> bool {
        let progress = progress.clamp(0.0, 1.0);
        let elapsed = self.last_emit.elapsed().as_millis();
        if (progress - self.last_progress).abs() >= TRANSFER_PROGRESS_MIN_STEP
            || elapsed >= TRANSFER_PROGRESS_MIN_INTERVAL_MS
        {
            self.last_progress = progress;
            self.last_emit = Instant::now();
            true
        } else {
            false
        }
    }
}
