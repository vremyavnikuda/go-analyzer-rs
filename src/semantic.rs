use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tower_lsp::lsp_types::{Position, Range};
use url::Url;

use crate::types::{RaceSeverity, VarId, VariableInfo};

#[derive(Clone, Debug)]
pub struct SemanticConfig {
    pub enabled: bool,
    pub helper_path: String,
    pub timeout_ms: u64,
}

impl SemanticConfig {
    pub fn from_env() -> Self {
        let enabled = match std::env::var("GO_ANALYZER_SEMANTIC") {
            Ok(v) => matches!(v.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"),
            Err(_) => false,
        };
        let helper_path = std::env::var("GO_ANALYZER_SEMANTIC_PATH")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .unwrap_or_else(|| "goanalyzer-semantic".to_string());
        let timeout_ms = std::env::var("GO_ANALYZER_SEMANTIC_TIMEOUT_MS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(2000);
        Self {
            enabled,
            helper_path,
            timeout_ms,
        }
    }
}

#[derive(Serialize)]
struct SemanticRequest {
    file: String,
    line: u32,
    col: u32,
    content: String,
}

#[derive(Deserialize)]
struct SemanticPos {
    line: u32,
    col: u32,
}

#[derive(Deserialize)]
struct SemanticRange {
    start: SemanticPos,
    end: SemanticPos,
}

#[derive(Deserialize)]
struct SemanticUseEntry {
    range: SemanticRange,
    reassign: bool,
    captured: bool,
}

#[derive(Deserialize)]
struct SemanticResponse {
    name: String,
    decl: SemanticRange,
    uses: Vec<SemanticUseEntry>,
    is_pointer: bool,
}

#[derive(Clone, Debug)]
pub struct SemanticUse {
    pub range: Range,
    pub reassign: bool,
    pub captured: bool,
}

#[derive(Clone, Debug)]
pub struct SemanticVariable {
    pub info: VariableInfo,
    pub uses: Vec<SemanticUse>,
}

fn map_range(range: SemanticRange) -> Range {
    Range::new(
        Position::new(range.start.line, range.start.col),
        Position::new(range.end.line, range.end.col),
    )
}

pub async fn resolve_semantic_variable(
    config: &SemanticConfig,
    uri: &Url,
    position: Position,
    code: &str,
) -> Option<SemanticVariable> {
    if !config.enabled {
        return None;
    }
    let file_path = uri.to_file_path().ok()?;
    let request = SemanticRequest {
        file: path_to_string(&file_path),
        line: position.line,
        col: position.character,
        content: code.to_string(),
    };
    let input = serde_json::to_vec(&request).ok()?;
    let mut child = Command::new(&config.helper_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;
    if let Some(stdin) = child.stdin.as_mut() {
        if stdin.write_all(&input).await.is_err() {
            return None;
        }
    }
    let output = tokio::time::timeout(
        Duration::from_millis(config.timeout_ms),
        child.wait_with_output(),
    )
    .await
    .ok()?
    .ok()?;
    if !output.status.success() {
        return None;
    }
    let response: Option<SemanticResponse> = serde_json::from_slice(&output.stdout).ok()?;
    let response = response?;
    let declaration = map_range(response.decl);
    let uses: Vec<SemanticUse> = response
        .uses
        .into_iter()
        .map(|entry| SemanticUse {
            range: map_range(entry.range),
            reassign: entry.reassign,
            captured: entry.captured,
        })
        .collect();
    let info = VariableInfo {
        name: response.name,
        declaration,
        uses: uses.iter().map(|u| u.range).collect(),
        is_pointer: response.is_pointer,
        potential_race: false,
        race_severity: RaceSeverity::Medium,
        var_id: VarId {
            start_byte: 0,
            end_byte: 0,
        },
    };
    Some(SemanticVariable { info, uses })
}

fn path_to_string(path: &PathBuf) -> String {
    path.to_string_lossy().to_string()
}
