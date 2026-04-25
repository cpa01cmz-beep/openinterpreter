use crate::function_tool::FunctionCallError;
use crate::tools::context::FunctionToolOutput;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolPayload;
use crate::tools::handlers::claude_code::effective_turn_file_system_policy;
use crate::tools::handlers::claude_code::ensure_readable_path;
use crate::tools::handlers::claude_code::ensure_writable_path;
use crate::tools::handlers::claude_code::parse_absolute_path;
use crate::tools::handlers::parse_arguments;
use crate::tools::registry::ToolHandler;
use crate::tools::registry::ToolKind;
use codex_utils_absolute_path::AbsolutePathBuf;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::LazyLock;
use tokio::sync::Mutex;

pub struct MinimalStrReplaceEditorHandler;

static UNDO_STACKS: LazyLock<Mutex<HashMap<String, Vec<String>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Deserialize)]
struct MinimalEditorArgs {
    command: MinimalEditorCommand,
    path: String,
    view_range: Option<Vec<usize>>,
    file_text: Option<String>,
    old_str: Option<String>,
    new_str: Option<String>,
    insert_line: Option<usize>,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum MinimalEditorCommand {
    View,
    Create,
    StrReplace,
    Insert,
    UndoEdit,
}

impl ToolHandler for MinimalStrReplaceEditorHandler {
    type Output = FunctionToolOutput;

    fn kind(&self) -> ToolKind {
        ToolKind::Function
    }

    async fn is_mutating(&self, invocation: &ToolInvocation) -> bool {
        let ToolPayload::Function { arguments } = &invocation.payload else {
            return true;
        };
        parse_arguments::<MinimalEditorArgs>(arguments)
            .map(|args| !matches!(args.command, MinimalEditorCommand::View))
            .unwrap_or(true)
    }

    async fn handle(&self, invocation: ToolInvocation) -> Result<Self::Output, FunctionCallError> {
        let ToolInvocation {
            session,
            turn,
            payload,
            ..
        } = invocation;
        let ToolPayload::Function { arguments } = payload else {
            return Err(FunctionCallError::RespondToModel(
                "str_replace_editor received unsupported payload".to_string(),
            ));
        };
        let args: MinimalEditorArgs = parse_arguments(&arguments)?;
        let path = parse_absolute_path(&args.path)?;
        let file_system_policy =
            effective_turn_file_system_policy(session.as_ref(), turn.as_ref()).await;

        match args.command {
            MinimalEditorCommand::View => {
                ensure_readable_path(&file_system_policy, turn.as_ref(), &path)?;
                let content = tokio::fs::read_to_string(path.as_path())
                    .await
                    .map_err(|err| {
                        FunctionCallError::RespondToModel(format!("view failed: {err}"))
                    })?;
                Ok(FunctionToolOutput::from_text(
                    format_view_output(&content, args.view_range.as_deref()),
                    Some(true),
                ))
            }
            MinimalEditorCommand::Create => {
                ensure_writable_path(&file_system_policy, turn.as_ref(), &path)?;
                if tokio::fs::try_exists(path.as_path()).await.unwrap_or(false) {
                    return Err(FunctionCallError::RespondToModel(format!(
                        "create failed: file already exists: {}",
                        path.display()
                    )));
                }
                let file_text = require_arg(args.file_text, "file_text")?;
                save_undo(&path, String::new()).await;
                tokio::fs::write(path.as_path(), file_text)
                    .await
                    .map_err(|err| {
                        FunctionCallError::RespondToModel(format!("create failed: {err}"))
                    })?;
                Ok(FunctionToolOutput::from_text(
                    format!("Created file: {}", path.display()),
                    Some(true),
                ))
            }
            MinimalEditorCommand::StrReplace => {
                ensure_readable_path(&file_system_policy, turn.as_ref(), &path)?;
                ensure_writable_path(&file_system_policy, turn.as_ref(), &path)?;
                let old_str = require_arg(args.old_str, "old_str")?;
                let new_str = require_arg(args.new_str, "new_str")?;
                let content = tokio::fs::read_to_string(path.as_path())
                    .await
                    .map_err(|err| {
                        FunctionCallError::RespondToModel(format!("str_replace failed: {err}"))
                    })?;
                let matches = content.match_indices(&old_str).count();
                if matches == 0 {
                    return Err(FunctionCallError::RespondToModel(
                        "str_replace failed: old_str was not found".to_string(),
                    ));
                }
                if matches > 1 {
                    return Err(FunctionCallError::RespondToModel(
                        "str_replace failed: old_str is not unique".to_string(),
                    ));
                }
                save_undo(&path, content.clone()).await;
                tokio::fs::write(path.as_path(), content.replacen(&old_str, &new_str, 1))
                    .await
                    .map_err(|err| {
                        FunctionCallError::RespondToModel(format!("str_replace failed: {err}"))
                    })?;
                Ok(FunctionToolOutput::from_text(
                    format!("Edited file: {}", path.display()),
                    Some(true),
                ))
            }
            MinimalEditorCommand::Insert => {
                ensure_readable_path(&file_system_policy, turn.as_ref(), &path)?;
                ensure_writable_path(&file_system_policy, turn.as_ref(), &path)?;
                let insert_line = args.insert_line.ok_or_else(|| {
                    FunctionCallError::RespondToModel(
                        "insert failed: missing insert_line".to_string(),
                    )
                })?;
                let new_str = require_arg(args.new_str, "new_str")?;
                let content = tokio::fs::read_to_string(path.as_path())
                    .await
                    .map_err(|err| {
                        FunctionCallError::RespondToModel(format!("insert failed: {err}"))
                    })?;
                let updated = insert_text(&content, insert_line, &new_str)?;
                save_undo(&path, content).await;
                tokio::fs::write(path.as_path(), updated)
                    .await
                    .map_err(|err| {
                        FunctionCallError::RespondToModel(format!("insert failed: {err}"))
                    })?;
                Ok(FunctionToolOutput::from_text(
                    format!("Edited file: {}", path.display()),
                    Some(true),
                ))
            }
            MinimalEditorCommand::UndoEdit => {
                ensure_writable_path(&file_system_policy, turn.as_ref(), &path)?;
                let previous = pop_undo(&path).await.ok_or_else(|| {
                    FunctionCallError::RespondToModel(
                        "undo_edit failed: no previous edit for this file".to_string(),
                    )
                })?;
                tokio::fs::write(path.as_path(), previous)
                    .await
                    .map_err(|err| {
                        FunctionCallError::RespondToModel(format!("undo_edit failed: {err}"))
                    })?;
                Ok(FunctionToolOutput::from_text(
                    format!("Reverted last edit for: {}", path.display()),
                    Some(true),
                ))
            }
        }
    }
}

fn require_arg(value: Option<String>, name: &str) -> Result<String, FunctionCallError> {
    value.ok_or_else(|| FunctionCallError::RespondToModel(format!("missing required {name}")))
}

async fn save_undo(path: &AbsolutePathBuf, content: String) {
    let mut stacks = UNDO_STACKS.lock().await;
    stacks
        .entry(path.display().to_string())
        .or_default()
        .push(content);
}

async fn pop_undo(path: &AbsolutePathBuf) -> Option<String> {
    let mut stacks = UNDO_STACKS.lock().await;
    stacks
        .get_mut(&path.display().to_string())
        .and_then(Vec::pop)
}

fn format_view_output(content: &str, view_range: Option<&[usize]>) -> String {
    let lines = content.lines().collect::<Vec<_>>();
    let (start, end) = match view_range {
        Some([start, end]) => (start.saturating_sub(1), (*end).min(lines.len())),
        Some([start]) => (start.saturating_sub(1), lines.len()),
        _ => (0, lines.len()),
    };
    lines
        .iter()
        .enumerate()
        .skip(start)
        .take(end.saturating_sub(start))
        .map(|(index, line)| format!("{}\t{line}", index + 1))
        .collect::<Vec<_>>()
        .join("\n")
}

fn insert_text(
    content: &str,
    insert_line: usize,
    new_str: &str,
) -> Result<String, FunctionCallError> {
    let mut lines = content.lines().map(str::to_string).collect::<Vec<_>>();
    if insert_line > lines.len() {
        return Err(FunctionCallError::RespondToModel(format!(
            "insert failed: insert_line {insert_line} is past end of file"
        )));
    }
    lines.insert(insert_line, new_str.to_string());
    let mut updated = lines.join("\n");
    if content.ends_with('\n') {
        updated.push('\n');
    }
    Ok(updated)
}
