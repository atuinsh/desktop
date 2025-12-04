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

/// Parses variable content from a string supporting two formats:
/// 1. Simple format: `key=value` (one per line)
/// 2. Heredoc format: `key<<DELIMITER` followed by lines until `DELIMITER`
///
/// The heredoc format allows multiline values:
/// ```text
/// myvar<<EOF
/// line 1
/// line 2
/// EOF
/// ```
///
/// Returns a HashMap of the parsed variables.
pub fn parse_vars(content: &str) -> HashMap<String, String> {
    let mut vars = HashMap::new();
    let lines: Vec<&str> = content.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];

        // Check for heredoc syntax: KEY<<DELIMITER
        // Only treat as heredoc if << appears before any = sign
        // This prevents "KEY=VALUE<<TEXT" from being parsed as heredoc
        if let Some(heredoc_pos) = line.find("<<") {
            let equals_pos = line.find('=');

            // Only treat as heredoc if either:
            // 1. No = at all, OR
            // 2. << comes before =
            let is_heredoc = match equals_pos {
                None => true,
                Some(eq_pos) => heredoc_pos < eq_pos,
            };

            if is_heredoc {
                let key = line[..heredoc_pos].trim();
                let delimiter = line[heredoc_pos + 2..].trim();

                if !key.is_empty() && !delimiter.is_empty() {
                    // Collect lines until we find the delimiter
                    let mut value_lines = Vec::new();
                    i += 1;

                    while i < lines.len() {
                        if lines[i].trim() == delimiter {
                            // Found the closing delimiter
                            break;
                        }
                        value_lines.push(lines[i]);
                        i += 1;
                    }

                    // Join the collected lines with newlines
                    let value = value_lines.join("\n");
                    vars.insert(key.to_string(), value);
                    i += 1; // Move past the delimiter line
                    continue;
                }
            }
        }

        // Fall back to simple KEY=VALUE parsing
        let parts = line.splitn(2, '=').collect::<Vec<&str>>();
        if parts.len() == 2 {
            vars.insert(parts[0].to_string(), parts[1].to_string());
        }

        i += 1;
    }

    vars
}

/// Reads the variables from the temporary file and returns them as a HashMap.
/// The file supports two formats:
/// - Simple: `key=value` (one per line)
/// - Heredoc: `key<<DELIMITER` followed by lines until `DELIMITER` for multiline values
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

    #[test]
    fn test_heredoc_basic_multiline() {
        let content = "output<<EOF\nline 1\nline 2\nline 3\nEOF\n";
        let vars = parse_vars(content);

        assert_eq!(vars.len(), 1);
        assert_eq!(
            vars.get("output"),
            Some(&"line 1\nline 2\nline 3".to_string())
        );
    }

    #[test]
    fn test_heredoc_custom_delimiter() {
        let content = "myvar<<CUSTOM_DELIM\nsome content\nmore content\nCUSTOM_DELIM\n";
        let vars = parse_vars(content);

        assert_eq!(vars.len(), 1);
        assert_eq!(
            vars.get("myvar"),
            Some(&"some content\nmore content".to_string())
        );
    }

    #[test]
    fn test_heredoc_single_line() {
        let content = "var<<END\nsingle line\nEND\n";
        let vars = parse_vars(content);

        assert_eq!(vars.len(), 1);
        assert_eq!(vars.get("var"), Some(&"single line".to_string()));
    }

    #[test]
    fn test_heredoc_empty_content() {
        let content = "empty<<DELIM\nDELIM\n";
        let vars = parse_vars(content);

        assert_eq!(vars.len(), 1);
        assert_eq!(vars.get("empty"), Some(&"".to_string()));
    }

    #[test]
    fn test_heredoc_with_special_characters() {
        let content = "special<<EOF\n$VAR\n`cmd`\n\"quotes\"\n'single'\nEOF\n";
        let vars = parse_vars(content);

        assert_eq!(vars.len(), 1);
        assert_eq!(
            vars.get("special"),
            Some(&"$VAR\n`cmd`\n\"quotes\"\n'single'".to_string())
        );
    }

    #[test]
    fn test_heredoc_preserves_whitespace() {
        let content = "spaces<<END\n  indented\n\ttabbed\n  \nEND\n";
        let vars = parse_vars(content);

        assert_eq!(vars.len(), 1);
        assert_eq!(
            vars.get("spaces"),
            Some(&"  indented\n\ttabbed\n  ".to_string())
        );
    }

    #[test]
    fn test_heredoc_missing_delimiter() {
        // If delimiter is never found, should consume rest of file
        let content = "incomplete<<EOF\nline 1\nline 2\n";
        let vars = parse_vars(content);

        assert_eq!(vars.len(), 1);
        assert_eq!(vars.get("incomplete"), Some(&"line 1\nline 2".to_string()));
    }

    #[test]
    fn test_mixed_heredoc_and_simple() {
        let content = "simple=value\noutput<<EOF\nmulti\nline\nEOF\nanother=test\n";
        let vars = parse_vars(content);

        assert_eq!(vars.len(), 3);
        assert_eq!(vars.get("simple"), Some(&"value".to_string()));
        assert_eq!(vars.get("output"), Some(&"multi\nline".to_string()));
        assert_eq!(vars.get("another"), Some(&"test".to_string()));
    }

    #[test]
    fn test_heredoc_delimiter_appears_in_content() {
        // Delimiter must be on its own line (after trim)
        let content = "var<<END\nthis line has END in it\nEND\n";
        let vars = parse_vars(content);

        assert_eq!(vars.len(), 1);
        assert_eq!(
            vars.get("var"),
            Some(&"this line has END in it".to_string())
        );
    }

    #[test]
    fn test_heredoc_with_equals_in_content() {
        // Should not parse equals signs inside heredoc as new variables
        let content = "config<<DONE\nkey=value\nfoo=bar\nDONE\n";
        let vars = parse_vars(content);

        assert_eq!(vars.len(), 1);
        assert_eq!(vars.get("config"), Some(&"key=value\nfoo=bar".to_string()));
    }

    #[tokio::test]
    async fn test_finalize_with_heredoc() {
        let handle = setup().expect("setup should succeed");

        let mut file = std::fs::File::create(handle.path()).expect("should open file for writing");
        writeln!(file, "simple=test").expect("should write to file");
        writeln!(file, "multi<<EOF").expect("should write to file");
        writeln!(file, "line 1").expect("should write to file");
        writeln!(file, "line 2").expect("should write to file");
        writeln!(file, "EOF").expect("should write to file");
        drop(file);

        let vars = finalize(handle).await.expect("finalize should succeed");
        assert_eq!(vars.len(), 2);
        assert_eq!(vars.get("simple"), Some(&"test".to_string()));
        assert_eq!(vars.get("multi"), Some(&"line 1\nline 2".to_string()));
    }

    #[test]
    fn test_not_heredoc_when_equals_before_double_angle() {
        // When = appears before <<, should parse as simple KEY=VALUE
        let content = "redirect=command<<input\n";
        let vars = parse_vars(content);

        assert_eq!(vars.len(), 1);
        assert_eq!(vars.get("redirect"), Some(&"command<<input".to_string()));
    }

    #[test]
    fn test_not_heredoc_complex_value_with_angles() {
        // Real-world case: bash redirection or comparison in value
        let content = "cmd=cat file.txt <<< 'input'\nop=test 5<<10\n";
        let vars = parse_vars(content);

        assert_eq!(vars.len(), 2);
        assert_eq!(
            vars.get("cmd"),
            Some(&"cat file.txt <<< 'input'".to_string())
        );
        assert_eq!(vars.get("op"), Some(&"test 5<<10".to_string()));
    }
}
