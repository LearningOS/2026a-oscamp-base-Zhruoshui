//! # File Descriptor Table
//!
//! Implement a simple file descriptor (fd) table — the core data structure
//! for managing open files in an OS kernel.
//!
//! ## Background
//!

//!
//! ```text
//! fd table:
//!   0 -> Stdin
//!   1 -> Stdout
//!   2 -> Stderr
//!   3 -> File("/etc/passwd")
//!   4 -> (empty)
//!   5 -> Socket(...)
//! ```
//!
//! ## Task
//!
//! Implement the following methods on `FdTable`:
//!
//! - `new()` — create an empty fd table
//! - `alloc(file)` -> `usize` — allocate a new fd, return the fd number
//!   - Prefer reusing the smallest closed fd number
//!   - If no free slot, extend the table
//! - `get(fd)` -> `Option<Arc<dyn File>>` — get the file object for an fd
//! - `close(fd)` -> `bool` — close an fd, return whether it succeeded (false if fd doesn't exist)
//! - `count()` -> `usize` — return the number of currently allocated fds (excluding closed ones)
//!
//! ## Key Concepts
//!
//! - Trait objects: `Arc<dyn File>`
//! - `Vec<Option<T>>` as a sparse table
//! - fd number reuse strategy (find smallest free slot)
//! - `Arc` reference counting and resource release

use std::{num::IntErrorKind::PosOverflow, os::fd, sync::Arc};

/// File abstraction trait — all "files" in the kernel (regular files, pipes, sockets) implement this
pub trait File: Send + Sync {                 // send安全跨线程转移所有权、sync安全在线程之间共享引用
    fn read(&self, buf: &mut [u8]) -> isize;  // 读取的内容写缓冲区
    fn write(&self, buf: &[u8]) -> isize;     // 读缓冲区内容写入文件对象
}

/// File descriptor table  文件描述符表
pub struct FdTable {
    // TODO: Design the internal structure
    // Hint: use Vec<Option<Arc<dyn File>>>
    //       the index is the fd number, None means the fd is closed or unallocated
    // 
    // Option: 代表 fd 的状态
    //      Some(file) 代表 fd 当前打开
    //      None       代表 fd 空闲、关闭或者未使用
    // 
    // Arc：代表文件对象被共享
    //      是原子引用计数的智能指针，表示多个所有者共享同一个对象。
    //      
    // File: File 是一个trait，文件抽象特性——内核中的所有“文件”
    //      （普通文件、管道、套接字）都实现了此特性
    //
    table: Vec<Option<Arc<dyn File>>>,
}

impl FdTable {
    /// Create an empty fd table
    pub fn new() -> Self {
        // TODO
        Self { table: Vec::new() }
    }

    /// Allocate a new fd, return the fd number.
    ///
    /// Prefers reusing the smallest closed fd number; if no free slot, appends to the end.
    pub fn alloc(&mut self, file: Arc<dyn File>) -> usize {
        if let Some(fd) = self.table.iter().position(|pos| pos.is_none()) {
            self.table[fd] = Some(file);
            fd
        }else {
            self.table.push(Some((file)));
            self.table.len() - 1 
        }
        
    }

    /// Get the file object for an fd. Returns None if the fd doesn't exist or is closed.
    pub fn get(&self, fd: usize) -> Option<Arc<dyn File>> {
        // TODO
        match self.table.get(fd) {
            None => None,                           // fd 越界
            Some(None) => None,                     // fd 已关闭
            Some(Some(file)) => Some(file.clone()), // fd 有效
        }
    }

    /// Close an fd. Returns true on success, false if the fd doesn't exist or is already closed.
    pub fn close(&mut self, fd: usize) -> bool {
        // TODO
        match self.table.get_mut(fd) {
            Some(fd) => fd.take().is_some(),
            None => false
        }
    }

    /// Return the number of currently allocated fds (excluding closed ones)
    pub fn count(&self) -> usize {
        // TODO
        self.table.iter().flatten().count()    //跳过多层嵌套的None
    }
}

impl Default for FdTable {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================
// Test File implementation
// ============================================================
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct MockFile {
        id: usize,
        write_log: Mutex<Vec<Vec<u8>>>,
    }

    impl MockFile {
        fn new(id: usize) -> Arc<Self> {
            Arc::new(Self {
                id,
                write_log: Mutex::new(vec![]),
            })
        }
    }

    impl File for MockFile {
        fn read(&self, buf: &mut [u8]) -> isize {
            buf[0] = self.id as u8;
            1
        }
        fn write(&self, buf: &[u8]) -> isize {
            self.write_log.lock().unwrap().push(buf.to_vec());
            buf.len() as isize
        }
    }

    #[test]
    fn test_alloc_basic() {
        let mut table = FdTable::new();
        let fd = table.alloc(MockFile::new(0));
        assert_eq!(fd, 0, "first fd should be 0");
        let fd2 = table.alloc(MockFile::new(1));
        assert_eq!(fd2, 1, "second fd should be 1");
    }

    #[test]
    fn test_get() {
        let mut table = FdTable::new();
        let file = MockFile::new(42);
        let fd = table.alloc(file);
        let got = table.get(fd);
        assert!(got.is_some(), "get should return Some");
        let mut buf = [0u8; 1];
        got.unwrap().read(&mut buf);
        assert_eq!(buf[0], 42);
    }

    #[test]
    fn test_get_invalid() {
        let table = FdTable::new();
        assert!(table.get(0).is_none());
        assert!(table.get(999).is_none());
    }

    #[test]
    fn test_close_and_reuse() {
        let mut table = FdTable::new();
        let fd0 = table.alloc(MockFile::new(0)); // fd=0
        let fd1 = table.alloc(MockFile::new(1)); // fd=1
        let fd2 = table.alloc(MockFile::new(2)); // fd=2

        assert!(table.close(fd1), "closing fd=1 should succeed");
        assert!(
            table.get(fd1).is_none(),
            "get should return None after close"
        );

        // Next allocation should reuse fd=1 (smallest free)
        let fd_new = table.alloc(MockFile::new(99));
        assert_eq!(fd_new, fd1, "should reuse the smallest closed fd");

        let _ = (fd0, fd2);
    }

    #[test]
    fn test_close_invalid() {
        let mut table = FdTable::new();
        assert!(
            !table.close(0),
            "closing non-existent fd should return false"
        );
    }

    #[test]
    fn test_count() {
        let mut table = FdTable::new();
        assert_eq!(table.count(), 0);
        let fd0 = table.alloc(MockFile::new(0));
        let fd1 = table.alloc(MockFile::new(1));
        assert_eq!(table.count(), 2);
        table.close(fd0);
        assert_eq!(table.count(), 1);
        table.close(fd1);
        assert_eq!(table.count(), 0);
    }

    #[test]
    fn test_write_through_fd() {
        let mut table = FdTable::new();
        let file = MockFile::new(0);
        let fd = table.alloc(file);
        let f = table.get(fd).unwrap();
        let n = f.write(b"hello");
        assert_eq!(n, 5);
    }
}
