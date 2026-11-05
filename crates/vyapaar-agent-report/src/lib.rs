//! Embedded Typst / Markdown quantitative research tearsheet generator.
//!
//! A [`TearsheetReport`] is a title, a set of named metrics, and free-form
//! sections. It renders to Markdown (for chat/CLI) and Typst (for PDF
//! compilation), and can be built directly from a
//! [`vyapaar_agent_analytics::PerformanceReport`].

use vyapaar_agent_analytics::PerformanceReport;

/// A named metric shown in the report header.
#[derive(Debug, Clone, PartialEq)]
pub struct Metric {
    /// Metric label (e.g. "Sharpe").
    pub name: String,
    /// Formatted value (e.g. "1.42").
    pub value: String,
}

impl Metric {
    /// Create a new metric.
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
        }
    }
}

/// A free-form report section.
#[derive(Debug, Clone, PartialEq)]
pub struct ReportSection {
    /// Section heading.
    pub heading: String,
    /// Section body (plain text or Markdown).
    pub body: String,
}

impl ReportSection {
    /// Create a new section.
    pub fn new(heading: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            heading: heading.into(),
            body: body.into(),
        }
    }
}

/// A quantitative research tearsheet.
#[derive(Debug, Clone, PartialEq)]
pub struct TearsheetReport {
    /// Report title.
    pub title: String,
    /// Header metrics.
    pub metrics: Vec<Metric>,
    /// Body sections.
    pub sections: Vec<ReportSection>,
}

impl TearsheetReport {
    /// Create an empty report with the given title.
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            metrics: Vec::new(),
            sections: Vec::new(),
        }
    }

    /// Add a header metric.
    pub fn add_metric(&mut self, metric: Metric) {
        self.metrics.push(metric);
    }

    /// Add a body section.
    pub fn add_section(&mut self, section: ReportSection) {
        self.sections.push(section);
    }

    /// Render the report as Markdown.
    pub fn to_markdown(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("# {}\n\n", self.title));
        if !self.metrics.is_empty() {
            out.push_str("| Metric | Value |\n| :--- | :--- |\n");
            for metric in &self.metrics {
                out.push_str(&format!("| {} | {} |\n", metric.name, metric.value));
            }
            out.push('\n');
        }
        for section in &self.sections {
            out.push_str(&format!("## {}\n\n{}\n\n", section.heading, section.body));
        }
        out
    }

    /// Render the report as Typst source.
    pub fn to_typst(&self) -> String {
        let mut out = String::new();
        out.push_str("#set page(paper: \"a4\", margin: 2cm)\n#set text(size: 10pt)\n\n");
        out.push_str(&format!(
            "#align(center)[#text(size: 18pt)[{}]]\n\n",
            self.title
        ));
        if !self.metrics.is_empty() {
            out.push_str("#table(\n  columns: 2,\n  [*Metric*], [*Value*],\n");
            for metric in &self.metrics {
                out.push_str(&format!("  [{}], [{}],\n", metric.name, metric.value));
            }
            out.push_str(")\n\n");
        }
        for section in &self.sections {
            out.push_str(&format!("== {}\n\n{}\n\n", section.heading, section.body));
        }
        out
    }

    /// Build a tearsheet from an analytics performance report.
    pub fn from_performance_report(title: impl Into<String>, report: &PerformanceReport) -> Self {
        let mut tearsheet = Self::new(title);
        let trades = &report.trades;
        let equity = &report.equity;

        tearsheet.add_metric(Metric::new("Trades", trades.n_trades.to_string()));
        tearsheet.add_metric(Metric::new(
            "Win rate",
            format!("{:.2}%", trades.win_rate * 100.0),
        ));
        tearsheet.add_metric(Metric::new(
            "Profit factor",
            trades
                .profit_factor
                .map(|pf| format!("{pf:.2}"))
                .unwrap_or_else(|| "n/a".into()),
        ));
        tearsheet.add_metric(Metric::new("Net PnL", format!("{:.2}", trades.total_pnl)));
        tearsheet.add_metric(Metric::new(
            "Total return",
            format!("{:.2}%", equity.total_return * 100.0),
        ));
        tearsheet.add_metric(Metric::new(
            "Sharpe",
            equity
                .sharpe
                .map(|s| format!("{s:.2}"))
                .unwrap_or_else(|| "n/a".into()),
        ));
        tearsheet.add_metric(Metric::new(
            "Max drawdown",
            format!("{:.2}%", equity.max_drawdown_pct * 100.0),
        ));

        tearsheet.add_section(ReportSection::new(
            "Trade statistics",
            format!(
                "{} trades: {} wins, {} losses, {} flat. Expectancy {:.2} per trade, total fees {:.2}.",
                trades.n_trades,
                trades.n_wins,
                trades.n_losses,
                trades.n_flat,
                trades.expectancy,
                trades.total_fees
            ),
        ));
        tearsheet.add_section(ReportSection::new(
            "Equity statistics",
            format!(
                "{} periods, annualized return {:.2}%, annualized volatility {:.2}%, Calmar {}.",
                equity.n_periods,
                equity.annualized_return * 100.0,
                equity.annualized_volatility * 100.0,
                equity
                    .calmar
                    .map(|c| format!("{c:.2}"))
                    .unwrap_or_else(|| "n/a".into())
            ),
        ));

        tearsheet
    }
}
