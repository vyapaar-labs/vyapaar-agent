//! 5-field cron expression matcher (minute hour day-of-month month day-of-week),
//! evaluated in Indian Standard Time (IST = UTC+05:30).
//!
//! Supported field syntax: `*`, `*/N`, `a-b`, `a-b/N`, `a,b,c`, and a single integer.
//! Day-of-week uses `0 = Sunday`, matching conventional cron.

use chrono::Datelike;
use chrono::Timelike;
use chrono::{DateTime, FixedOffset, Utc};
use thiserror::Error;

/// Indian Standard Time offset (UTC+05:30).
pub(crate) const IST: FixedOffset =
    FixedOffset::east_opt(5 * 3600 + 30 * 60).expect("IST offset is valid");

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CronError {
    #[error("invalid cron expression: {0}")]
    Invalid(String),
    #[error("field out of range: {0}")]
    OutOfRange(String),
}

/// Allowed values for a single cron field.
#[derive(Debug, Clone, PartialEq, Eq)]
struct FieldSpec {
    min: i64,
    max: i64,
    /// `allowed[(value - min) as usize]` indicates whether `value` matches.
    allowed: Vec<bool>,
}

impl FieldSpec {
    fn contains(&self, value: i64) -> bool {
        if value < self.min || value > self.max {
            return false;
        }
        self.allowed[(value - self.min) as usize]
    }
}

fn parse_field(s: &str, min: i64, max: i64) -> Result<FieldSpec, CronError> {
    let mut allowed = vec![false; (max - min + 1) as usize];
    for part in s.split(',') {
        let part = part.trim();
        if part.is_empty() {
            return Err(CronError::Invalid("empty cron field segment".to_string()));
        }
        let (range, step) = match part.split_once('/') {
            Some((r, st)) => (
                r,
                st.parse::<i64>()
                    .map_err(|_| CronError::Invalid(format!("invalid step '{st}'")))?,
            ),
            None => (part, 1),
        };
        if step == 0 {
            return Err(CronError::Invalid("step cannot be zero".to_string()));
        }
        let (lo, hi) = if range == "*" {
            (min, max)
        } else if let Some((a, b)) = range.split_once('-') {
            (
                a.parse::<i64>()
                    .map_err(|_| CronError::OutOfRange(range.to_string()))?,
                b.parse::<i64>()
                    .map_err(|_| CronError::OutOfRange(range.to_string()))?,
            )
        } else {
            let v = range
                .parse::<i64>()
                .map_err(|_| CronError::OutOfRange(range.to_string()))?;
            // A bare value with a step (e.g. `10/3`) walks from `v` to `max`.
            (v, if step > 1 { max } else { v })
        };
        if lo < min || hi > max || lo > hi {
            return Err(CronError::OutOfRange(range.to_string()));
        }
        let mut v = lo;
        while v <= hi {
            allowed[(v - min) as usize] = true;
            v += step;
        }
    }
    Ok(FieldSpec { min, max, allowed })
}

/// A compiled 5-field cron expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CronExpr {
    minute: FieldSpec,
    hour: FieldSpec,
    day_of_month: FieldSpec,
    month: FieldSpec,
    day_of_week: FieldSpec,
}

impl CronExpr {
    /// Parse a standard 5-field cron expression.
    pub fn parse(s: &str) -> Result<Self, CronError> {
        let fields: Vec<&str> = s.split_whitespace().collect();
        if fields.len() != 5 {
            return Err(CronError::Invalid(format!(
                "expected 5 fields, got {}",
                fields.len()
            )));
        }
        Ok(Self {
            minute: parse_field(fields[0], 0, 59)?,
            hour: parse_field(fields[1], 0, 23)?,
            day_of_month: parse_field(fields[2], 1, 31)?,
            month: parse_field(fields[3], 1, 12)?,
            day_of_week: parse_field(fields[4], 0, 6)?,
        })
    }

    /// Return `true` if the expression matches `dt` (evaluated in IST).
    pub fn is_due(&self, dt: DateTime<Utc>) -> bool {
        let ist = dt.with_timezone(&IST);
        self.minute.contains(ist.minute() as i64)
            && self.hour.contains(ist.hour() as i64)
            && self.day_of_month.contains(ist.day() as i64)
            && self.month.contains(ist.month() as i64)
            && self
                .day_of_week
                .contains(ist.weekday().num_days_from_sunday() as i64)
    }

    /// Return the next scheduled time strictly after `after`, or `None` if none
    /// occurs within 366 days. Evaluated in IST.
    pub fn next_after(&self, after: DateTime<Utc>) -> Option<DateTime<Utc>> {
        let mut cur = after + chrono::Duration::minutes(1);
        let cap = after + chrono::Duration::days(366);
        while cur <= cap {
            if self.is_due(cur) {
                return Some(cur);
            }
            cur += chrono::Duration::minutes(1);
        }
        None
    }
}
