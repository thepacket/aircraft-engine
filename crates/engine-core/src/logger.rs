//! Fixed-rate data logger producing CSV, the way a flight data recorder or an
//! engine test-cell DAS would.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataLogger {
    pub rate_hz: f64,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<f64>>,
    pub enabled: bool,
    pub max_rows: usize,
    next_sample_t: f64,
}

impl DataLogger {
    pub fn new(rate_hz: f64, columns: Vec<String>) -> Self {
        DataLogger {
            rate_hz: rate_hz.max(0.1),
            columns,
            rows: Vec::new(),
            enabled: false,
            max_rows: 500_000,
            next_sample_t: 0.0,
        }
    }

    pub fn start(&mut self, t_now: f64) {
        self.enabled = true;
        self.next_sample_t = t_now;
    }

    pub fn stop(&mut self) {
        self.enabled = false;
    }

    pub fn clear(&mut self) {
        self.rows.clear();
    }

    pub fn set_rate(&mut self, rate_hz: f64) {
        self.rate_hz = rate_hz.max(0.1);
    }

    /// True when a sample would be recorded at `t_now` (cheap check so callers
    /// can skip building the row).
    pub fn due(&self, t_now: f64) -> bool {
        self.enabled && t_now + 1e-9 >= self.next_sample_t
    }

    /// Offer a sample. It is recorded only when the logger is enabled and the
    /// sample interval has elapsed.
    pub fn maybe_sample(&mut self, t_now: f64, row: impl FnOnce() -> Vec<f64>) {
        if !self.due(t_now) {
            return;
        }
        if self.rows.len() >= self.max_rows {
            self.enabled = false;
            return;
        }
        self.rows.push(row());
        self.next_sample_t += 1.0 / self.rate_hz;
        if self.next_sample_t < t_now {
            self.next_sample_t = t_now;
        }
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    pub fn to_csv(&self) -> String {
        let mut out = String::with_capacity(self.rows.len() * 160 + 512);
        out.push_str(&self.columns.join(","));
        out.push('\n');
        for r in &self.rows {
            let line: Vec<String> = r.iter().map(|v| format_num(*v)).collect();
            out.push_str(&line.join(","));
            out.push('\n');
        }
        out
    }
}

fn format_num(v: f64) -> String {
    if v == v.trunc() && v.abs() < 1e12 {
        format!("{}", v as i64)
    } else {
        format!("{:.4}", v)
    }
}
