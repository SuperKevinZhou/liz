//! Thread store interfaces.

use crate::storage::error::StorageResult;
use crate::storage::fs::{ensure_layout, list_json_files, read_json, write_json};
use crate::storage::paths::StoragePaths;
use liz_protocol::{Thread, ThreadId};

/// Minimal interface for thread persistence.
pub trait ThreadStore {
    /// Persists the provided thread record.
    fn put_thread(&self, thread: &Thread) -> StorageResult<()>;

    /// Reads a previously persisted thread record.
    fn get_thread(&self, thread_id: &ThreadId) -> StorageResult<Option<Thread>>;

    /// Lists all persisted thread records.
    fn list_threads(&self) -> StorageResult<Vec<Thread>>;
}

/// Filesystem-backed thread store.
#[derive(Debug, Clone)]
pub struct FsThreadStore {
    paths: StoragePaths,
}

impl FsThreadStore {
    /// Creates a filesystem-backed thread store.
    pub fn new(paths: StoragePaths) -> Self {
        Self { paths }
    }
}

impl ThreadStore for FsThreadStore {
    fn put_thread(&self, thread: &Thread) -> StorageResult<()> {
        ensure_layout(&self.paths)?;
        write_json(&self.paths.thread_file(&thread.id), thread)
    }

    fn get_thread(&self, thread_id: &ThreadId) -> StorageResult<Option<Thread>> {
        ensure_layout(&self.paths)?;
        read_json(&self.paths.thread_file(thread_id))
    }

    fn list_threads(&self) -> StorageResult<Vec<Thread>> {
        ensure_layout(&self.paths)?;

        let mut threads = Vec::new();
        for path in list_json_files(&self.paths.threads_dir())? {
            if let Some(thread) = read_json(&path)? {
                threads.push(thread);
            }
        }

        Ok(threads)
    }
}
