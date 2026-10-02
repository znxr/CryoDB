use sqlx::types::chrono::NaiveDateTime;
use std::time::Duration;

pub(crate) fn format_duration(duration: Duration) -> String {
    let secs = duration.as_secs_f32();
    if secs < 10.0 {
        format!("{secs:.2}s")
    } else if secs < 60.0 {
        format!("{secs:.1}s")
    } else {
        let minutes = duration.as_secs() / 60;
        let seconds = duration.as_secs() % 60;
        format!("{minutes}m {seconds}s")
    }
}

pub(crate) fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = UNITS[0];
    for next_unit in &UNITS[1..] {
        if value < 1024.0 {
            break;
        }
        value /= 1024.0;
        unit = next_unit;
    }
    if unit == "B" {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {unit}")
    }
}

pub(crate) fn format_optional_datetime(value: Option<NaiveDateTime>) -> Option<String> {
    value.map(|value| value.format("%Y-%m-%d %H:%M:%S").to_string())
}

pub(crate) fn clock_label() -> String {
    sqlx::types::chrono::Local::now()
        .format("%H:%M")
        .to_string()
}
