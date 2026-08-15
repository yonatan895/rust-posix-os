//! POSIX Epoll Asynchronous Event Multiplexing Inode.

use crate::ostd::sync::SpinLock;
use crate::services::vfs::{FileType, Inode};
use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use alloc::vec::Vec;
use posix_abi::*;

/// Inode representing an epoll event monitoring instance.
pub struct EpollInstance {
    /// Registered file descriptor interest list guarded by a spinlock.
    interests: SpinLock<BTreeMap<i32, EpollEvent>>,
}

impl EpollInstance {
    /// Creates a new reference-counted epoll instance.
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            interests: SpinLock::new(BTreeMap::new()),
        })
    }

    /// Modifies the interest list of this epoll instance (ADD, MOD, DEL).
    pub fn ctl(&self, op: i32, fd: i32, event: EpollEvent) -> Result<(), i32> {
        let mut map = self.interests.lock();
        match op {
            EPOLL_CTL_ADD => {
                if map.contains_key(&fd) {
                    return Err(EEXIST);
                }
                map.insert(fd, event);
                Ok(())
            }
            EPOLL_CTL_MOD => {
                if !map.contains_key(&fd) {
                    return Err(ENOENT);
                }
                map.insert(fd, event);
                Ok(())
            }
            EPOLL_CTL_DEL => {
                if map.remove(&fd).is_none() {
                    return Err(ENOENT);
                }
                Ok(())
            }
            _ => Err(EINVAL),
        }
    }

    /// Polls monitored descriptors and fills `events` with ready I/O events.
    pub fn wait(&self, events: &mut [EpollEvent], maxevents: usize) -> Result<usize, i32> {
        if maxevents == 0 || events.is_empty() {
            return Err(EINVAL);
        }

        let map = self.interests.lock();
        let mut ready_count = 0;

        let proc_lock = match crate::services::process::get_current_process() {
            Some(p) => p,
            None => return Err(ESRCH),
        };
        let proc = proc_lock.lock();

        for (&fd, &interest) in map.iter() {
            if ready_count >= maxevents || ready_count >= events.len() {
                break;
            }

            if let Some(handle) = proc.get_fd(fd) {
                let poll_flags = handle.inode.poll();
                let mut triggered_events = 0u32;

                if (interest.events & EPOLLIN != 0) && (poll_flags.readable) {
                    triggered_events |= EPOLLIN;
                }
                if (interest.events & EPOLLOUT != 0) && (poll_flags.writable) {
                    triggered_events |= EPOLLOUT;
                }
                if poll_flags.hangup {
                    triggered_events |= EPOLLHUP;
                }
                if poll_flags.error {
                    triggered_events |= EPOLLERR;
                }

                if triggered_events != 0 {
                    events[ready_count] = EpollEvent {
                        events: triggered_events,
                        data: interest.data,
                    };
                    ready_count += 1;
                }
            }
        }

        Ok(ready_count)
    }
}

impl Inode for EpollInstance {
    fn file_type(&self) -> FileType {
        FileType::Anonymous
    }

    fn read(&self, _offset: usize, _buf: &mut [u8]) -> Result<usize, i32> {
        Err(EINVAL)
    }

    fn write(&self, _offset: usize, _buf: &[u8]) -> Result<usize, i32> {
        Err(EINVAL)
    }

    fn lookup(&self, _name: &str) -> Result<Arc<dyn Inode>, i32> {
        Err(ENOTDIR)
    }

    fn readdir(&self) -> Result<Vec<Dirent64>, i32> {
        Err(ENOTDIR)
    }

    fn stat(&self) -> Result<Stat, i32> {
        Ok(Stat {
            st_mode: S_IFREG | 0o600,
            ..Default::default()
        })
    }

    fn as_epoll(&self) -> Option<&EpollInstance> {
        Some(self)
    }
}
