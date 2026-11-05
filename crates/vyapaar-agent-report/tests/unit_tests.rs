//! Unit tests for `vyapaar-agent-report`.

use vyapaar_agent_analytics::{EquityStats, PerformanceReport, TradeStats};
use vyapaar_agent_report::{Metric, ReportSection, TearsheetReport};

fn sample_report() -> PerformanceReport {
    PerformanceReport {
        trades: TradeStats {
            n_trades: 10,
            n_wins: 6,
            n_losses: 3,
            n_flat: 1,
            win_rate: 0.6,
            gross_profit: 1200.0,
            gross_loss: 300.0,
            profit_factor: Some(4.0),
            avg_win: Some(200.0),
            avg_loss: Some(-100.0),
            total_pnl: 900.0,
            expectancy: 90.0,
            total_fees: 50.0,
        },
        equity: EquityStats {
            n_periods: 252,
            total_return: 0.25,
            annualized_return: 0.25,
            annualized_volatility: 0.15,
            sharpe: Some(1.5),
            sortino: Some(2.0),
            max_drawdown: 500.0,
            max_drawdown_pct: 0.08,
            calmar: Some(3.125),
        },
    }
}

#[test]
fn markdown_renders_title_metrics_and_sections() {
    let mut report = TearsheetReport::new("Momentum Alpha");
    report.add_metric(Metric::new("Sharpe", "1.50"));
    report.add_section(ReportSection::new("Regime", "Trending."));

    let md = report.to_markdown();
    assert!(md.starts_with("# Momentum Alpha"));
    assert!(md.contains("| Sharpe | 1.50 |"));
    assert!(md.contains("## Regime"));
    assert!(md.contains("Trending."));
}

#[test]
fn typst_renders_title_metrics_and_sections() {
    let mut report = TearsheetReport::new("Momentum Alpha");
    report.add_metric(Metric::new("Sharpe", "1.50"));
    report.add_section(ReportSection::new("Regime", "Trending."));

    let typst = report.to_typst();
    assert!(typst.contains("Momentum Alpha"));
    assert!(typst.contains("[Sharpe], [1.50]"));
    assert!(typst.contains("== Regime"));
}

#[test]
fn from_performance_report_builds_metrics_and_sections() {
    let report = TearsheetReport::from_performance_report("Backtest", &sample_report());
    assert_eq!(report.title, "Backtest");
    assert_eq!(report.metrics.len(), 7);
    assert!(
        report
            .metrics
            .iter()
            .any(|m| m.name == "Trades" && m.value == "10")
    );
    assert!(
        report
            .metrics
            .iter()
            .any(|m| m.name == "Sharpe" && m.value == "1.50")
    );
    assert!(
        report
            .metrics
            .iter()
            .any(|m| m.name == "Win rate" && m.value == "60.00%")
    );
    assert_eq!(report.sections.len(), 2);
    assert!(
        report.sections[0]
            .body
            .contains("10 trades: 6 wins, 3 losses, 1 flat")
    );
}

#[test]
fn empty_report_renders_title_only() {
    let report = TearsheetReport::new("Empty");
    assert_eq!(report.to_markdown(), "# Empty\n\n");
    assert!(report.to_typst().contains("Empty"));
}
