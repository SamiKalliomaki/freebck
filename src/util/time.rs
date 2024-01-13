use std::time::{SystemTime, Duration};

pub fn as_unix_timestamp(time: SystemTime) -> i64 {
    match time.duration_since(SystemTime::UNIX_EPOCH) {
        Ok(duration) => duration.as_secs() as i64,
        Err(error) => -(error.duration().as_secs() as i64),
    }
}

pub fn system_time_from_unix_timestamp(time: i64) -> Result<SystemTime, std::io::Error> {
    // Addition or subtraction may overflow SystemTime range and panic.
    std::panic::catch_unwind(move || {
        if time < 0 {
            SystemTime::UNIX_EPOCH - Duration::from_secs((-time) as u64)
        } else {
            SystemTime::UNIX_EPOCH + Duration::from_secs(time as u64)
        }
    }).map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("Invalid timestamp (not supported by OS): {}", time),
        )
    })
}
