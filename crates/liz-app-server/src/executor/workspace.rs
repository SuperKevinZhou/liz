//! Local workspace-backed tool implementations.

use crate::runtime::{RuntimeError, RuntimeResult};
use liz_protocol::{
    WorkspaceApplyPatchRequest, WorkspaceApplyPatchResult, WorkspaceListEntry,
    WorkspaceListRequest, WorkspaceListResult, WorkspaceReadRequest, WorkspaceReadResult,
    WorkspaceSearchMatch, WorkspaceSearchRequest, WorkspaceSearchResult, WorkspaceWriteTextRequest,
    WorkspaceWriteTextResult,
};
use std::fs;
use std::path::{Path, PathBuf};

pub fn list(request: &WorkspaceListRequest) -> RuntimeResult<WorkspaceListResult> {
    let root = PathBuf::from(&request.root);
    ensure_directory(&root)?;

    let mut entries = Vec::new();
    let mut truncated = false;
    collect_entries(
        &root,
        &root,
        request.recursive,
        request.include_hidden,
        request.max_entries.unwrap_or(usize::MAX),
        &mut entries,
        &mut truncated,
    )?;

    Ok(WorkspaceListResult { root: request.root.clone(), entries, truncated })
}

pub fn search(request: &WorkspaceSearchRequest) -> RuntimeResult<WorkspaceSearchResult> {
    let root = PathBuf::from(&request.root);
    ensure_directory(&root)?;

    let mut files = Vec::new();
    collect_files(&root, &root, request.include_hidden, &mut files)?;
    let mut matches = Vec::new();
    let mut truncated = false;
    let needle = if request.case_sensitive {
        request.pattern.clone()
    } else {
        request.pattern.to_ascii_lowercase()
    };

    for file in files {
        if matches.len() >= request.max_results.unwrap_or(usize::MAX) {
            truncated = true;
            break;
        }

        let Ok(content) = fs::read_to_string(root.join(&file)) else {
            continue;
        };

        for (index, line) in content.lines().enumerate() {
            let haystack =
                if request.case_sensitive { line.to_owned() } else { line.to_ascii_lowercase() };
            if haystack.contains(&needle) {
                matches.push(WorkspaceSearchMatch {
                    path: file.clone(),
                    line_number: index + 1,
                    line: line.to_owned(),
                });
                if matches.len() >= request.max_results.unwrap_or(usize::MAX) {
                    truncated = true;
                    break;
                }
            }
        }
    }

    Ok(WorkspaceSearchResult {
        root: request.root.clone(),
        pattern: request.pattern.clone(),
        matches,
        truncated,
    })
}

pub fn read(request: &WorkspaceReadRequest) -> RuntimeResult<WorkspaceReadResult> {
    let path = PathBuf::from(&request.path);
    ensure_file(&path)?;

    let content = fs::read_to_string(&path).map_err(|error| {
        RuntimeError::invalid_state(
            "workspace_read_failed",
            format!("failed to read {}: {error}", path.display()),
        )
    })?;
    let lines = content.lines().map(ToOwned::to_owned).collect::<Vec<_>>();
    let total_lines = lines.len().max(1);
    let start_line = request.start_line.unwrap_or(1).max(1);
    let end_line = request.end_line.unwrap_or(lines.len().max(1)).max(start_line);
    let content = if lines.is_empty() {
        String::new()
    } else {
        lines[(start_line - 1).min(lines.len())..end_line.min(lines.len())].join("\n")
    };

    Ok(WorkspaceReadResult {
        path: request.path.clone(),
        content,
        start_line,
        end_line: end_line.min(total_lines),
        total_lines,
    })
}

pub fn write_text(request: &WorkspaceWriteTextRequest) -> RuntimeResult<WorkspaceWriteExecution> {
    let path = PathBuf::from(&request.path);
    let before = fs::read_to_string(&path).unwrap_or_default();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            RuntimeError::invalid_state(
                "workspace_write_parent_failed",
                format!("failed to create parent directories for {}: {error}", path.display()),
            )
        })?;
    }
    fs::write(&path, &request.content).map_err(|error| {
        RuntimeError::invalid_state(
            "workspace_write_failed",
            format!("failed to write {}: {error}", path.display()),
        )
    })?;

    Ok(WorkspaceWriteExecution {
        result: WorkspaceWriteTextResult {
            path: request.path.clone(),
            changed: before != request.content,
            byte_length: request.content.len(),
        },
        before,
        after: request.content.clone(),
    })
}

pub fn apply_patch(request: &WorkspaceApplyPatchRequest) -> RuntimeResult<WorkspacePatchExecution> {
    let path = PathBuf::from(&request.path);
    ensure_file(&path)?;
    let before = fs::read_to_string(&path).map_err(|error| {
        RuntimeError::invalid_state(
            "workspace_patch_read_failed",
            format!("failed to read {}: {error}", path.display()),
        )
    })?;
    let matches = before.matches(&request.search).count();
    if matches == 0 {
        return Err(RuntimeError::not_found(
            "workspace_patch_search_not_found",
            format!("patch target was not found in {}", path.display()),
        ));
    }

    let replacements = if request.replace_all { matches } else { 1 };
    let after = if request.replace_all {
        before.replace(&request.search, &request.replace)
    } else {
        before.replacen(&request.search, &request.replace, 1)
    };
    fs::write(&path, &after).map_err(|error| {
        RuntimeError::invalid_state(
            "workspace_patch_write_failed",
            format!("failed to write {}: {error}", path.display()),
        )
    })?;

    Ok(WorkspacePatchExecution {
        result: WorkspaceApplyPatchResult {
            path: request.path.clone(),
            replacements,
            changed: before != after,
        },
        before,
        after,
    })
}

fn collect_entries(
    root: &Path,
    current: &Path,
    recursive: bool,
    include_hidden: bool,
    max_entries: usize,
    entries: &mut Vec<WorkspaceListEntry>,
    truncated: &mut bool,
) -> RuntimeResult<()> {
    let mut children = fs::read_dir(current).map_err(|error| {
        RuntimeError::invalid_state(
            "workspace_list_failed",
            format!("failed to enumerate {}: {error}", current.display()),
        )
    })?;
    let mut paths = Vec::new();
    while let Some(entry) = children.next() {
        let entry = entry.map_err(|error| {
            RuntimeError::invalid_state(
                "workspace_list_failed",
                format!("failed to enumerate {}: {error}", current.display()),
            )
        })?;
        paths.push(entry.path());
    }
    paths.sort();

    for path in paths {
        if entries.len() >= max_entries {
            *truncated = true;
            break;
        }
        if !include_hidden && is_hidden(&path) {
            continue;
        }
        let metadata = fs::metadata(&path).map_err(|error| {
            RuntimeError::invalid_state(
                "workspace_list_failed",
                format!("failed to stat {}: {error}", path.display()),
            )
        })?;
        entries.push(WorkspaceListEntry {
            path: relative_display(root, &path),
            is_dir: metadata.is_dir(),
        });
        if recursive && metadata.is_dir() {
            collect_entries(
                root,
                &path,
                recursive,
                include_hidden,
                max_entries,
                entries,
                truncated,
            )?;
            if *truncated {
                break;
            }
        }
    }

    Ok(())
}

fn collect_files(
    root: &Path,
    current: &Path,
    include_hidden: bool,
    files: &mut Vec<String>,
) -> RuntimeResult<()> {
    let mut children = fs::read_dir(current).map_err(|error| {
        RuntimeError::invalid_state(
            "workspace_search_failed",
            format!("failed to enumerate {}: {error}", current.display()),
        )
    })?;
    let mut paths = Vec::new();
    while let Some(entry) = children.next() {
        let entry = entry.map_err(|error| {
            RuntimeError::invalid_state(
                "workspace_search_failed",
                format!("failed to enumerate {}: {error}", current.display()),
            )
        })?;
        paths.push(entry.path());
    }
    paths.sort();

    for path in paths {
        if !include_hidden && is_hidden(&path) {
            continue;
        }
        let metadata = fs::metadata(&path).map_err(|error| {
            RuntimeError::invalid_state(
                "workspace_search_failed",
                format!("failed to stat {}: {error}", path.display()),
            )
        })?;
        if metadata.is_dir() {
            collect_files(root, &path, include_hidden, files)?;
        } else {
            files.push(relative_display(root, &path));
        }
    }

    Ok(())
}

fn ensure_directory(path: &Path) -> RuntimeResult<()> {
    if !path.exists() {
        return Err(RuntimeError::not_found(
            "workspace_root_not_found",
            format!("workspace root {} does not exist", path.display()),
        ));
    }
    if !path.is_dir() {
        return Err(RuntimeError::invalid_state(
            "workspace_root_not_directory",
            format!("workspace root {} is not a directory", path.display()),
        ));
    }
    Ok(())
}

fn ensure_file(path: &Path) -> RuntimeResult<()> {
    if !path.exists() {
        return Err(RuntimeError::not_found(
            "workspace_file_not_found",
            format!("workspace file {} does not exist", path.display()),
        ));
    }
    if !path.is_file() {
        return Err(RuntimeError::invalid_state(
            "workspace_path_not_file",
            format!("workspace path {} is not a file", path.display()),
        ));
    }
    Ok(())
}

fn relative_display(root: &Path, path: &Path) -> String {
    path.strip_prefix(root).unwrap_or(path).to_string_lossy().replace('\\', "/")
}

fn is_hidden(path: &Path) -> bool {
    path.file_name().and_then(|name| name.to_str()).is_some_and(|name| name.starts_with('.'))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceWriteExecution {
    pub result: WorkspaceWriteTextResult,
    pub before: String,
    pub after: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspacePatchExecution {
    pub result: WorkspaceApplyPatchResult,
    pub before: String,
    pub after: String,
}
