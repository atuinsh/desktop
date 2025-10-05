use serde::Deserialize;
use tauri::ipc::Response;
use tempfile::NamedTempFile;
use tokio::{fs, process::Command};
use ts_rs::TS;

#[allow(dead_code)]
#[derive(TS, Deserialize)]
#[ts(export)]
struct ShellCheckOutput {
    comments: Vec<Comment>,
}

/// ShellCheck output format
///
/// Reference https://github.com/koalaman/shellcheck/blob/master/src/ShellCheck/Formatter/JSON.hs
///
/// ShellCheck uses the arbitrary precision Integer type in Haskell instead of
/// the 32/64 bit fixed width Int for their integers, but u32 should be enough.
#[allow(dead_code)]
#[derive(TS, Deserialize)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
struct Comment {
    file: String,
    line: u32,
    end_line: u32,
    column: u32,
    end_column: u32,
    level: Severity,
    code: u32,
    message: String,
    fix: Option<Fix>,
}

#[allow(dead_code)]
#[derive(TS, Deserialize)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
enum Severity {
    Error,
    Warning,
    Info,
    Style,
}

#[allow(dead_code)]
#[derive(TS, Deserialize)]
#[ts(export)]
struct Fix {
    replacements: Vec<Replacement>,
}

#[allow(dead_code)]
#[derive(TS, Deserialize)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
struct Replacement {
    line: u32,
    end_line: u32,
    column: u32,
    end_column: u32,
    insertion_point: InsertionPoint,
    precedence: u32,
    replacement: String,
}

#[allow(dead_code)]
#[derive(TS, Deserialize)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
enum InsertionPoint {
    BeforeStart,
    AfterEnd,
}

#[tauri::command]
pub async fn shellcheck(arg0: String, shell: String, script: String) -> Result<Response, String> {
    let temp_file = NamedTempFile::new().map_err(|e| e.to_string())?;
    let temp_path = temp_file.as_ref();

    fs::write(temp_path, script)
        .await
        .map_err(|e| e.to_string())?;

    let output = Command::new(&arg0)
        .args(["--format", "json1", "--shell", &shell])
        .arg(temp_path)
        .output()
        .await
        .map_err(|e| format!("Failed to run ShellCheck: {arg0} {e}"))?;

    // deserializing here just to validate the output format
    // output.stdout is passed through directly to the front end
    let _: ShellCheckOutput = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("Malformed ShellCheck output: {e}"))?;

    Ok(Response::new(output.stdout))
}
