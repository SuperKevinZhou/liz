//! Shared filesystem helpers for storage implementations.

use crate::storage::{StoragePaths, StorageResult};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

pub(crate) fn ensure_layout(paths: &StoragePaths) -> StorageResult<()> {
    for dir in paths.required_directories() {
        fs::create_dir_all(dir)?;
    }

    Ok(())
}

pub(crate) fn ensure_parent(path: &Path) -> StorageResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    Ok(())
}

pub(crate) fn write_json<T: Serialize>(path: &Path, value: &T) -> StorageResult<()> {
    ensure_parent(path)?;
    let payload = serde_json::to_vec_pretty(value)?;
    fs::write(path, payload)?;
    Ok(())
}

pub(crate) fn read_json<T: DeserializeOwned>(path: &Path) -> StorageResult<Option<T>> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

pub(crate) fn list_json_files(dir: &Path) -> StorageResult<Vec<PathBuf>> {
    match fs::read_dir(dir) {
        Ok(entries) => {
            let mut files = entries
                .filter_map(|entry| entry.ok())
                .map(|entry| entry.path())
                .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
                .collect::<Vec<_>>();
            files.sort();
            Ok(files)
        }
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(error.into()),
    }
}
