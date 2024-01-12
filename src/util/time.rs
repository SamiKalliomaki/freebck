use std::time::SystemTime;

pub fn as_unix_timestamp(time: SystemTime) -> i64 {
    match time.duration_since(SystemTime::UNIX_EPOCH) {
        Ok(duration) => duration.as_secs() as i64,
        Err(error) => -(error.duration().as_secs() as i64),
    }
}

pub fn system_time_from_unix_timestamp(time: i64) -> SystemTime {
    if time < 0 {
        SystemTime::UNIX_EPOCH - std::time::Duration::from_secs((-time) as u64)
    } else {
        SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(time as u64)
    }
}
