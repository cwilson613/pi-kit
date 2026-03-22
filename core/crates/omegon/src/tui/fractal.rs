//! Fractal widget stub — functionality moved to instruments.rs
//!
//! This file is kept as a stub for backward compatibility.
//! The actual fractal rendering now happens in the CIC instrument panel.

use ratatui::prelude::*;

/// Deprecated: Use InstrumentPanel from instruments.rs instead.
#[deprecated(note = "Use InstrumentPanel from instruments.rs")]
pub struct FractalWidget;

/// Deprecated: Agent modes moved to instrument panel telemetry.
#[deprecated(note = "Use InstrumentTelemetry from instruments.rs")]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AgentMode {
    Idle,
    Working,
    Thinking,
}

impl Default for AgentMode {
    fn default() -> Self { Self::Idle }
}

#[allow(dead_code)]
impl FractalWidget {
    pub fn new() -> Self { Self }
    pub fn render(&self, _area: Rect, _frame: &mut Frame, _theme: &dyn crate::tui::theme::Theme) {
        // No-op - functionality moved to InstrumentPanel
    }
}

impl Default for FractalWidget {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fractal_widget_stub_compiles() {
        let _widget = FractalWidget::new();
        let _mode = AgentMode::default();
    }
}
