use anyhow::{anyhow, Result};
use chrono::{DateTime, Duration, Utc};
use cron::Schedule;
use log::info;
use std::str::FromStr;
use tokio::time::{sleep, Duration as TokioDuration};

/// Schedule configuration - supports both simple intervals and cron expressions
#[derive(Debug, Clone)]
pub enum ScheduleType {
    Interval(Duration),       // Simple: 30m, 3h, 1d
    Cron(Schedule),           // Cron: "0 */3 * * *"
}

impl ScheduleType {
    /// Parse schedule from config string
    pub fn parse(input: &str) -> Result<Self> {
        // Try simple interval first (e.g., "3h", "30m", "1d")
        if let Some(duration) = parse_simple_interval(input) {
            return Ok(ScheduleType::Interval(duration));
        }

        // Try cron expression
        match Schedule::from_str(input) {
            Ok(schedule) => Ok(ScheduleType::Cron(schedule)),
            Err(_) => Err(anyhow!(
                "Invalid schedule format: '{}'. Use simple interval (30m, 3h, 1d) or cron (0 */3 * * *)",
                input
            )),
        }
    }

    /// Calculate next run time from now
    pub fn next_run_time(&self) -> DateTime<Utc> {
        match self {
            ScheduleType::Interval(duration) => Utc::now() + *duration,
            ScheduleType::Cron(schedule) => {
                schedule
                    .upcoming(Utc)
                    .next()
                    .unwrap_or_else(|| Utc::now() + Duration::try_hours(1).unwrap())
            }
        }
    }

    /// Get duration until next run
    pub fn duration_until_next_run(&self) -> TokioDuration {
        let next = self.next_run_time();
        let now = Utc::now();
        let duration = (next - now).to_std().unwrap_or(TokioDuration::from_secs(60));
        duration
    }
}

/// Parse simple interval strings like "30m", "3h", "1d"
fn parse_simple_interval(input: &str) -> Option<Duration> {
    let input = input.trim();
    if input.is_empty() {
        return None;
    }

    // Parse number and unit
    let (num_str, unit) = input.split_at(input.len() - 1);
    let number: i64 = num_str.parse().ok()?;

    match unit {
        "m" => Duration::try_minutes(number),
        "h" => Duration::try_hours(number),
        "d" => Duration::try_days(number),
        _ => None,
    }
}

/// Scheduler daemon that runs collection on a schedule
pub struct Scheduler {
    schedule: ScheduleType,
}

impl Scheduler {
    pub fn new(schedule: ScheduleType) -> Self {
        Self { schedule }
    }

    /// Sleep until next scheduled run time
    pub async fn wait_for_next_run(&self) {
        let duration = self.schedule.duration_until_next_run();
        let next_time = self.schedule.next_run_time();

        info!("Next collection scheduled for: {}", next_time.format("%Y-%m-%d %H:%M:%S UTC"));
        info!("Sleeping for {} seconds...", duration.as_secs());

        sleep(duration).await;
    }

    /// Get next run time (for logging/notifications)
    pub fn next_run_time(&self) -> DateTime<Utc> {
        self.schedule.next_run_time()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_intervals() {
        assert!(parse_simple_interval("30m").is_some());
        assert!(parse_simple_interval("3h").is_some());
        assert!(parse_simple_interval("1d").is_some());
        assert!(parse_simple_interval("invalid").is_none());
    }

    #[test]
    fn test_parse_schedule() {
        // Simple intervals
        assert!(ScheduleType::parse("30m").is_ok());
        assert!(ScheduleType::parse("3h").is_ok());
        assert!(ScheduleType::parse("1d").is_ok());

        // Cron expressions
        assert!(ScheduleType::parse("0 */3 * * *").is_ok());
        assert!(ScheduleType::parse("0 9 * * *").is_ok());

        // Invalid
        assert!(ScheduleType::parse("invalid").is_err());
    }
}
