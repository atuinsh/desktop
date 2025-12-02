use std::{collections::HashMap, path::Path};

use tokio::io::AsyncBufReadExt;

/// Handle returned by `setup` that tracks the temporary file for variable storage.
pub struct FsVarHandle {
    file: tempfile::NamedTempFile,
}

impl FsVarHandle {
    /// Returns the path to the temporary file where variables should be written.
    /// Variables should be written in the format `key=value`, one per line.
    pub fn path(&self) -> &Path {
        self.file.path()
    }
}

/// Sets up a temporary file for storing variables during block execution.
pub fn setup() -> Result<FsVarHandle, Box<dyn std::error::Error + Send + Sync>> {
    let file = tempfile::Builder::new()
        .prefix("atuin-desktop-vars")
        .suffix(".txt")
        .rand_bytes(8)
        .tempfile()?;
    Ok(FsVarHandle { file })
}

/// Parses variable content from a string in the format `key=value`, one per line.
/// Returns a HashMap of the parsed variables.
pub fn parse_vars(content: &str) -> HashMap<String, String> {
    let mut vars = HashMap::new();
    for line in content.lines() {
        let parts = line.splitn(2, '=').collect::<Vec<&str>>();
        if parts.len() == 2 {
            vars.insert(parts[0].to_string(), parts[1].to_string());
        }
    }
    vars
}

/// Reads the variables from the temporary file and returns them as a HashMap.
/// The file is expected to contain lines in the format `key=value`.
pub async fn finalize(
    handle: FsVarHandle,
) -> Result<HashMap<String, String>, Box<dyn std::error::Error + Send + Sync>> {
    let file = tokio::fs::File::open(handle.path()).await?;
    let reader = tokio::io::BufReader::new(file);
    let mut content = String::new();
    let mut lines = reader.lines();
    while let Some(line) = lines.next_line().await? {
        content.push_str(&line);
        content.push('\n');
    }
    Ok(parse_vars(&content))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[tokio::test]
    async fn test_setup_creates_file() {
        let handle = setup().expect("setup should succeed");
        assert!(
            handle.path().exists(),
            "temporary file should exist after setup"
        );
    }

    #[tokio::test]
    async fn test_finalize_empty_file_returns_empty_map() {
        let handle = setup().expect("setup should succeed");
        let vars = finalize(handle).await.expect("finalize should succeed");
        assert!(vars.is_empty(), "empty file should produce empty map");
    }

    #[tokio::test]
    async fn test_single_variable() {
        let handle = setup().expect("setup should succeed");

        // Write a single variable to the file
        let mut file = std::fs::File::create(handle.path()).expect("should open file for writing");
        writeln!(file, "MY_VAR=hello").expect("should write to file");
        drop(file);

        let vars = finalize(handle).await.expect("finalize should succeed");
        assert_eq!(vars.len(), 1);
        assert_eq!(vars.get("MY_VAR"), Some(&"hello".to_string()));
    }

    #[tokio::test]
    async fn test_multiple_variables() {
        let handle = setup().expect("setup should succeed");

        let mut file = std::fs::File::create(handle.path()).expect("should open file for writing");
        writeln!(file, "VAR1=value1").expect("should write to file");
        writeln!(file, "VAR2=value2").expect("should write to file");
        writeln!(file, "VAR3=value3").expect("should write to file");
        drop(file);

        let vars = finalize(handle).await.expect("finalize should succeed");
        assert_eq!(vars.len(), 3);
        assert_eq!(vars.get("VAR1"), Some(&"value1".to_string()));
        assert_eq!(vars.get("VAR2"), Some(&"value2".to_string()));
        assert_eq!(vars.get("VAR3"), Some(&"value3".to_string()));
    }

    #[tokio::test]
    async fn test_empty_value() {
        let handle = setup().expect("setup should succeed");

        let mut file = std::fs::File::create(handle.path()).expect("should open file for writing");
        writeln!(file, "EMPTY_VAR=").expect("should write to file");
        drop(file);

        let vars = finalize(handle).await.expect("finalize should succeed");
        assert_eq!(vars.len(), 1);
        assert_eq!(vars.get("EMPTY_VAR"), Some(&"".to_string()));
    }

    #[tokio::test]
    async fn test_value_with_equals_sign() {
        let handle = setup().expect("setup should succeed");

        let mut file = std::fs::File::create(handle.path()).expect("should open file for writing");
        writeln!(file, "MY_VAR=foo=bar=baz").expect("should write to file");
        drop(file);

        let vars = finalize(handle).await.expect("finalize should succeed");
        assert_eq!(vars.len(), 1);
        assert_eq!(vars.get("MY_VAR"), Some(&"foo=bar=baz".to_string()));
    }

    #[tokio::test]
    async fn test_blank_lines_are_ignored() {
        let handle = setup().expect("setup should succeed");

        let mut file = std::fs::File::create(handle.path()).expect("should open file for writing");
        writeln!(file, "VAR1=value1").expect("should write to file");
        writeln!(file).expect("should write blank line");
        writeln!(file, "VAR2=value2").expect("should write to file");
        drop(file);

        let vars = finalize(handle).await.expect("finalize should succeed");
        assert_eq!(vars.len(), 2);
        assert_eq!(vars.get("VAR1"), Some(&"value1".to_string()));
        assert_eq!(vars.get("VAR2"), Some(&"value2".to_string()));
    }

    #[tokio::test]
    async fn test_later_value_overwrites_earlier() {
        let handle = setup().expect("setup should succeed");

        let mut file = std::fs::File::create(handle.path()).expect("should open file for writing");
        writeln!(file, "MY_VAR=first").expect("should write to file");
        writeln!(file, "MY_VAR=second").expect("should write to file");
        drop(file);

        let vars = finalize(handle).await.expect("finalize should succeed");
        assert_eq!(vars.len(), 1);
        assert_eq!(vars.get("MY_VAR"), Some(&"second".to_string()));
    }
}
