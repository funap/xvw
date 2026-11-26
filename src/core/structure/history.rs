use crate::core::recent_history::RecentPathHistory;

/// Stores the most recently used Kaitai structure definition paths.
pub type DefinitionHistory = RecentPathHistory;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::recent_history::MAX_RECENT_PATHS;
    use std::path::PathBuf;

    #[test]
    fn keeps_five_unique_definition_paths_in_most_recent_order() {
        let mut history = DefinitionHistory::default();
        for index in 0..=MAX_RECENT_PATHS {
            history.record(PathBuf::from(format!("definition-{index}.ksy")));
        }

        assert_eq!(history.paths().len(), MAX_RECENT_PATHS);
        assert_eq!(history.paths().first(), Some(&PathBuf::from("definition-5.ksy")));
        assert!(!history.paths().contains(&PathBuf::from("definition-0.ksy")));
    }
}
