use std::path::PathBuf;

/// Application launch options parsed from command line arguments.
#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub struct LaunchArgs {
    pub files_to_open: Vec<PathBuf>,
    pub folder_to_open: Option<PathBuf>,
    pub ksy_to_load: Option<PathBuf>,
    pub diff: Option<(PathBuf, PathBuf)>,
    pub panel: Option<String>,
    pub sidebar: Option<bool>,
}

/// Backwards compatibility alias.
pub type CliArgs = LaunchArgs;

impl LaunchArgs {
    /// Parses command line arguments from the current process environment.
    pub fn parse() -> Self {
        Self::parse_from_args(std::env::args().skip(1))
    }

    /// Parses command line arguments from an arbitrary sequence of strings.
    pub fn parse_from_args<I, T>(args: I) -> Self
    where
        I: IntoIterator<Item = T>,
        T: Into<String>,
    {
        let mut launch_args = Self::default();
        let mut iter = args.into_iter().map(Into::into).peekable();

        while let Some(arg) = iter.next() {
            match arg.as_str() {
                "--ksy" => {
                    if let Some(path) = iter.next() {
                        launch_args.ksy_to_load = Some(PathBuf::from(path));
                    }
                }
                "--diff" => {
                    if let (Some(left), Some(right)) = (iter.next(), iter.next()) {
                        launch_args.diff = Some((PathBuf::from(left), PathBuf::from(right)));
                    }
                }
                "--folder" => {
                    if let Some(folder) = iter.next() {
                        launch_args.folder_to_open = Some(PathBuf::from(folder));
                    }
                }
                "--panel" => {
                    if let Some(panel) = iter.next() {
                        launch_args.panel = Some(panel);
                    }
                }
                "--no-sidebar" => {
                    launch_args.sidebar = Some(false);
                }
                "--sidebar" => {
                    launch_args.sidebar = Some(true);
                }
                other => {
                    if other.starts_with("--") {
                        eprintln!("xvw: warning: unknown option '{other}'");
                        continue;
                    }

                    let path = PathBuf::from(other);
                    if path.is_dir() {
                        launch_args.folder_to_open = Some(path);
                    } else {
                        launch_args.files_to_open.push(path);
                    }
                }
            }
        }

        launch_args
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_flags() {
        let args = LaunchArgs::parse_from_args(["--sidebar", "--panel", "inspector"]);
        assert_eq!(args.sidebar, Some(true));
        assert_eq!(args.panel.as_deref(), Some("inspector"));
    }

    #[test]
    fn test_parse_diff() {
        let args = LaunchArgs::parse_from_args(["--diff", "file_a.bin", "file_b.bin"]);
        assert_eq!(args.diff, Some((PathBuf::from("file_a.bin"), PathBuf::from("file_b.bin"))));
    }

    #[test]
    fn test_parse_ksy() {
        let args = LaunchArgs::parse_from_args(["--ksy", "format.ksy", "sample.bin"]);
        assert_eq!(args.ksy_to_load, Some(PathBuf::from("format.ksy")));
        assert_eq!(args.files_to_open, vec![PathBuf::from("sample.bin")]);
    }

    #[test]
    fn test_unknown_option_ignored_instead_of_treated_as_file() {
        let args = LaunchArgs::parse_from_args(["--unknown-option", "real_file.bin"]);
        assert_eq!(args.files_to_open, vec![PathBuf::from("real_file.bin")]);
    }
}
