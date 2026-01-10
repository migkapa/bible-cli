use std::time::Duration;

use indicatif::{ProgressBar, ProgressStyle};

pub struct ThinkingIndicator {
    spinner: ProgressBar,
}

impl ThinkingIndicator {
    pub fn new() -> Self {
        let spinner = ProgressBar::new_spinner();
        spinner.set_style(
            ProgressStyle::default_spinner()
                .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏")
                .template("{spinner:.dim} {msg:.dim}")
                .unwrap(),
        );
        spinner.set_message("Thinking...");
        Self { spinner }
    }

    pub fn start(&self) {
        self.spinner.enable_steady_tick(Duration::from_millis(80));
    }

    pub fn finish(&self) {
        self.spinner.finish_and_clear();
    }
}

impl Default for ThinkingIndicator {
    fn default() -> Self {
        Self::new()
    }
}
