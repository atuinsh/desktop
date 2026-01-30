use std::collections::HashMap;

/// Escape a value for safe inclusion in a POSIX single-quoted string.
///
/// Replaces each `'` with `'\''` (end quote, escaped literal quote, start new quote)
/// and wraps the result in single quotes. POSIX single-quoted strings treat all
/// characters as literal, so this handles `$`, backticks, newlines, etc.
fn shell_escape_value(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len() + 2);
    escaped.push('\'');
    for ch in value.chars() {
        if ch == '\'' {
            escaped.push_str("'\\''");
        } else {
            escaped.push(ch);
        }
    }
    escaped.push('\'');
    escaped
}

/// Build `export K='V'\n` lines for all env vars.
///
/// Returns an empty string if the map is empty.
pub fn build_env_exports(env_vars: &HashMap<String, String>) -> String {
    if env_vars.is_empty() {
        return String::new();
    }

    let mut out = String::new();
    for (key, value) in env_vars {
        out.push_str("export ");
        out.push_str(key);
        out.push('=');
        out.push_str(&shell_escape_value(value));
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shell_escape_simple_value() {
        assert_eq!(shell_escape_value("hello"), "'hello'");
    }

    #[test]
    fn test_shell_escape_empty_value() {
        assert_eq!(shell_escape_value(""), "''");
    }

    #[test]
    fn test_shell_escape_value_with_single_quotes() {
        assert_eq!(shell_escape_value("it's"), "'it'\\''s'");
    }

    #[test]
    fn test_shell_escape_value_with_special_chars() {
        assert_eq!(shell_escape_value("$HOME"), "'$HOME'");
        assert_eq!(shell_escape_value("`cmd`"), "'`cmd`'");
        assert_eq!(shell_escape_value("a\nb"), "'a\nb'");
        assert_eq!(shell_escape_value("a b"), "'a b'");
        assert_eq!(shell_escape_value("a\"b"), "'a\"b'");
    }

    #[test]
    fn test_build_env_exports_empty_map() {
        let map = HashMap::new();
        assert_eq!(build_env_exports(&map), "");
    }

    #[test]
    fn test_build_env_exports_single_var() {
        let mut map = HashMap::new();
        map.insert("FOO".to_string(), "bar".to_string());
        assert_eq!(build_env_exports(&map), "export FOO='bar'\n");
    }

    #[test]
    fn test_build_env_exports_special_value() {
        let mut map = HashMap::new();
        map.insert("VAR".to_string(), "it's $complex".to_string());
        assert_eq!(build_env_exports(&map), "export VAR='it'\\''s $complex'\n");
    }

    #[test]
    fn test_build_env_exports_multiple_vars() {
        let mut map = HashMap::new();
        map.insert("A".to_string(), "1".to_string());
        map.insert("B".to_string(), "2".to_string());

        let result = build_env_exports(&map);
        // HashMap order is not guaranteed, so check both lines are present
        assert!(result.contains("export A='1'\n"));
        assert!(result.contains("export B='2'\n"));
        // Should have exactly 2 lines
        assert_eq!(result.lines().count(), 2);
    }
}
