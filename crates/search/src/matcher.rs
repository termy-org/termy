use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchMatch {
    /// Line index in Alacritty coordinates (`0..rows` for viewport, negative for history).
    pub line: i32,
    /// Start column in terminal cell coordinates (end-exclusive with `end_col`).
    pub start_col: usize,
    /// End column in terminal cell coordinates (exclusive).
    pub end_col: usize,
}

impl SearchMatch {
    pub fn new(line: i32, start_col: usize, end_col: usize) -> Self {
        Self {
            line,
            start_col,
            end_col,
        }
    }

    pub fn contains(&self, line: i32, col: usize) -> bool {
        self.line == line && col >= self.start_col && col < self.end_col
    }
}

#[derive(Debug, Clone)]
pub struct SearchResults {
    matches: Vec<SearchMatch>,
    current_index: Option<usize>,
    match_ranges_by_line: HashMap<i32, Vec<(usize, usize)>>,
}

impl Default for SearchResults {
    fn default() -> Self {
        Self::new()
    }
}

impl SearchResults {
    pub fn new() -> Self {
        Self {
            matches: Vec::new(),
            current_index: None,
            match_ranges_by_line: HashMap::new(),
        }
    }

    pub fn from_matches(matches: Vec<SearchMatch>) -> Self {
        let current_index = if matches.is_empty() { None } else { Some(0) };
        let match_ranges_by_line = Self::build_match_ranges_by_line(&matches);
        Self {
            matches,
            current_index,
            match_ranges_by_line,
        }
    }

    fn build_match_ranges_by_line(matches: &[SearchMatch]) -> HashMap<i32, Vec<(usize, usize)>> {
        let mut ranges_by_line = HashMap::new();
        for m in matches {
            ranges_by_line
                .entry(m.line)
                .or_insert_with(Vec::new)
                .push((m.start_col, m.end_col));
        }
        ranges_by_line
    }

    pub fn count(&self) -> usize {
        self.matches.len()
    }

    pub fn is_empty(&self) -> bool {
        self.matches.is_empty()
    }

    pub fn matches(&self) -> &[SearchMatch] {
        &self.matches
    }

    pub fn current(&self) -> Option<&SearchMatch> {
        self.current_index.and_then(|i| self.matches.get(i))
    }

    pub fn position(&self) -> Option<(usize, usize)> {
        self.current_index.map(|i| (i + 1, self.matches.len()))
    }

    pub fn next(&mut self) -> Option<&SearchMatch> {
        if self.matches.is_empty() {
            return None;
        }
        let next_index = match self.current_index {
            Some(i) => (i + 1) % self.matches.len(),
            None => 0,
        };
        self.current_index = Some(next_index);
        self.matches.get(next_index)
    }

    pub fn previous(&mut self) -> Option<&SearchMatch> {
        if self.matches.is_empty() {
            return None;
        }
        let prev_index = match self.current_index {
            Some(i) => {
                if i == 0 {
                    self.matches.len() - 1
                } else {
                    i - 1
                }
            }
            None => self.matches.len() - 1,
        };
        self.current_index = Some(prev_index);
        self.matches.get(prev_index)
    }

    pub fn jump_to(&mut self, index: usize) -> Option<&SearchMatch> {
        if index < self.matches.len() {
            self.current_index = Some(index);
            self.matches.get(index)
        } else {
            None
        }
    }

    pub fn jump_to_first(&mut self) -> Option<&SearchMatch> {
        self.jump_to(0)
    }

    pub fn jump_to_last(&mut self) -> Option<&SearchMatch> {
        let index = self.matches.len().checked_sub(1)?;
        self.jump_to(index)
    }

    pub fn jump_to_nearest(&mut self, target_line: i32) -> Option<&SearchMatch> {
        if self.matches.is_empty() {
            return None;
        }

        let index = self
            .matches
            .iter()
            .position(|m| m.line >= target_line)
            .unwrap_or(0);

        self.current_index = Some(index);
        self.matches.get(index)
    }

    pub fn is_current_match(&self, line: i32, col: usize) -> bool {
        self.current()
            .map(|m| m.contains(line, col))
            .unwrap_or(false)
    }

    pub fn is_any_match(&self, line: i32, col: usize) -> bool {
        self.match_ranges_by_line
            .get(&line)
            .map(|ranges| {
                ranges
                    .iter()
                    .any(|(start_col, end_col)| col >= *start_col && col < *end_col)
            })
            .unwrap_or(false)
    }

    pub fn matches_in_range(&self, min_line: i32, max_line: i32) -> Vec<&SearchMatch> {
        self.matches
            .iter()
            .filter(|m| m.line >= min_line && m.line <= max_line)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_match_contains() {
        let m = SearchMatch::new(5, 10, 15);
        assert!(m.contains(5, 10));
        assert!(m.contains(5, 14));
        assert!(!m.contains(5, 15));
        assert!(!m.contains(5, 9));
        assert!(!m.contains(4, 12));
    }

    #[test]
    fn test_empty_results() {
        let results = SearchResults::new();
        assert!(results.is_empty());
        assert_eq!(results.count(), 0);
        assert!(results.current().is_none());
        assert!(results.position().is_none());
    }

    #[test]
    fn test_navigation() {
        let matches = vec![
            SearchMatch::new(0, 0, 5),
            SearchMatch::new(1, 10, 15),
            SearchMatch::new(2, 5, 10),
        ];
        let mut results = SearchResults::from_matches(matches);

        assert_eq!(results.position(), Some((1, 3)));
        assert_eq!(results.current().unwrap().line, 0);

        results.next();
        assert_eq!(results.position(), Some((2, 3)));
        assert_eq!(results.current().unwrap().line, 1);

        results.next();
        assert_eq!(results.position(), Some((3, 3)));

        results.next();
        assert_eq!(results.position(), Some((1, 3)));

        results.previous();
        assert_eq!(results.position(), Some((3, 3)));
    }

    #[test]
    fn test_jump_to_nearest() {
        let matches = vec![
            SearchMatch::new(-10, 0, 5),
            SearchMatch::new(-5, 0, 5),
            SearchMatch::new(0, 0, 5),
            SearchMatch::new(5, 0, 5),
        ];
        let mut results = SearchResults::from_matches(matches);

        results.jump_to_nearest(-7);
        assert_eq!(results.current().unwrap().line, -5);

        results.jump_to_nearest(0);
        assert_eq!(results.current().unwrap().line, 0);

        results.jump_to_nearest(100);
        assert_eq!(results.current().unwrap().line, -10);
    }

    #[test]
    fn test_jump_to_last() {
        let matches = vec![
            SearchMatch::new(-10, 0, 5),
            SearchMatch::new(-5, 0, 5),
            SearchMatch::new(0, 0, 5),
            SearchMatch::new(5, 0, 5),
        ];
        let mut results = SearchResults::from_matches(matches);

        results.jump_to_last();
        assert_eq!(results.current().unwrap().line, 5);

        results.jump_to_first();
        assert_eq!(results.current().unwrap().line, -10);
    }
}
