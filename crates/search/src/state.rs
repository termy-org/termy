use crate::engine::{SearchConfig, SearchEngine, SearchMode};
use crate::matcher::SearchResults;

pub struct SearchState {
    engine: SearchEngine,
    results: SearchResults,
    results_revision: u64,
    query: String,
    is_active: bool,
    error: Option<String>,
}

impl Default for SearchState {
    fn default() -> Self {
        Self::new()
    }
}

impl SearchState {
    pub fn new() -> Self {
        Self {
            engine: SearchEngine::new(SearchConfig::default()),
            results: SearchResults::new(),
            results_revision: 0,
            query: String::new(),
            is_active: false,
            error: None,
        }
    }

    pub fn open(&mut self) {
        self.is_active = true;
    }

    pub fn close(&mut self) {
        self.is_active = false;
        self.clear();
    }

    pub fn is_active(&self) -> bool {
        self.is_active
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn set_query(&mut self, query: &str) {
        self.query = query.to_string();
        match self.engine.set_pattern(query) {
            Ok(()) => self.error = None,
            Err(e) => self.error = Some(e),
        }
    }

    pub fn clear(&mut self) {
        self.query.clear();
        let _ = self.engine.set_pattern("");
        self.results = SearchResults::new();
        self.results_revision = self.results_revision.wrapping_add(1);
        self.error = None;
    }

    pub fn results(&self) -> &SearchResults {
        &self.results
    }

    pub fn results_revision(&self) -> u64 {
        self.results_revision
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn has_valid_pattern(&self) -> bool {
        self.engine.has_pattern()
    }

    pub fn toggle_case_sensitive(&mut self) {
        let mut config = self.config();
        config.case_sensitive = !config.case_sensitive;
        self.engine.set_config(config);
    }

    pub fn toggle_regex_mode(&mut self) {
        let mut config = self.config();
        config.mode = match config.mode {
            SearchMode::Literal => SearchMode::Regex,
            SearchMode::Regex => SearchMode::Literal,
        };
        self.engine.set_config(config);
        let query = self.query.clone();
        self.set_query(&query);
    }

    pub fn config(&self) -> SearchConfig {
        SearchConfig {
            case_sensitive: self.is_case_sensitive(),
            mode: self.mode(),
        }
    }

    pub fn is_case_sensitive(&self) -> bool {
        false
    }

    pub fn mode(&self) -> SearchMode {
        SearchMode::Literal
    }

    pub fn search<F>(&mut self, start_line: i32, end_line: i32, line_provider: F)
    where
        F: Fn(i32) -> Option<String>,
    {
        self.results = self.engine.search(start_line, end_line, line_provider);
        self.results_revision = self.results_revision.wrapping_add(1);
    }

    pub fn next_match(&mut self) {
        self.results.next();
    }

    pub fn previous_match(&mut self) {
        self.results.previous();
    }

    pub fn jump_to_nearest(&mut self, line: i32) {
        self.results.jump_to_nearest(line);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn results_revision_changes_on_search_and_clear() {
        let mut state = SearchState::new();
        let baseline = state.results_revision();

        state.search(0, 2, |line| match line {
            0 => Some("alpha".to_string()),
            1 => Some("beta".to_string()),
            2 => Some("gamma".to_string()),
            _ => None,
        });
        assert_eq!(state.results_revision(), baseline.wrapping_add(1));

        state.clear();
        assert_eq!(state.results_revision(), baseline.wrapping_add(2));
    }

    #[test]
    fn results_revision_does_not_change_for_selection_navigation() {
        let mut state = SearchState::new();
        state.search(0, 3, |line| match line {
            0 => Some("match".to_string()),
            1 => Some("x".to_string()),
            2 => Some("match".to_string()),
            3 => Some("y".to_string()),
            _ => None,
        });

        let revision = state.results_revision();
        state.next_match();
        state.previous_match();
        state.jump_to_nearest(10);

        assert_eq!(state.results_revision(), revision);
    }

    #[test]
    fn close_advances_results_revision_via_clear() {
        let mut state = SearchState::new();
        state.open();
        let baseline = state.results_revision();
        state.close();
        assert_eq!(state.results_revision(), baseline.wrapping_add(1));
    }
}
