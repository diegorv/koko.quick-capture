use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

/// Pool of reusable `Vec<f32>` buffers backed by a bounded std mpsc channel.
///
/// Checkout is a try_recv (never blocks the audio thread).
/// Return is automatic on PooledBuffer drop.
/// If pool is exhausted, a fresh Vec is allocated.
pub struct BufferPool {
    tx: mpsc::SyncSender<Vec<f32>>,
    rx: Mutex<mpsc::Receiver<Vec<f32>>>,
    stats: Arc<PoolStats>,
}

struct PoolStats {
    checkouts: AtomicU32,
    fallbacks: AtomicU32,
}

pub struct PooledBuffer {
    buf: Option<Vec<f32>>,
    return_tx: mpsc::SyncSender<Vec<f32>>,
}

impl PooledBuffer {
    pub fn as_slice(&self) -> &[f32] {
        self.buf.as_ref().unwrap()
    }

    pub fn as_mut_vec(&mut self) -> &mut Vec<f32> {
        self.buf.as_mut().unwrap()
    }

    pub fn into_vec(mut self) -> Vec<f32> {
        self.buf.take().unwrap()
    }
}

impl std::ops::Deref for PooledBuffer {
    type Target = [f32];
    fn deref(&self) -> &[f32] {
        self.as_slice()
    }
}

impl std::ops::DerefMut for PooledBuffer {
    fn deref_mut(&mut self) -> &mut [f32] {
        self.buf.as_mut().unwrap().as_mut_slice()
    }
}

impl Drop for PooledBuffer {
    fn drop(&mut self) {
        if let Some(mut buf) = self.buf.take() {
            buf.clear();
            let _ = self.return_tx.try_send(buf);
        }
    }
}

impl BufferPool {
    pub fn new(count: usize, capacity: usize) -> Self {
        let (tx, rx) = mpsc::sync_channel(count);
        for _ in 0..count {
            let _ = tx.try_send(Vec::with_capacity(capacity));
        }
        BufferPool {
            tx,
            rx: Mutex::new(rx),
            stats: Arc::new(PoolStats {
                checkouts: AtomicU32::new(0),
                fallbacks: AtomicU32::new(0),
            }),
        }
    }

    pub fn checkout(&self) -> PooledBuffer {
        let rx = self.rx.lock().expect("pool mutex");
        let buf = match rx.try_recv() {
            Ok(buf) => {
                self.stats.checkouts.fetch_add(1, Ordering::Relaxed);
                buf
            }
            Err(_) => {
                self.stats.fallbacks.fetch_add(1, Ordering::Relaxed);
                Vec::new()
            }
        };
        PooledBuffer {
            buf: Some(buf),
            return_tx: self.tx.clone(),
        }
    }

    pub fn stats(&self) -> (u32, u32) {
        (
            self.stats.checkouts.load(Ordering::Relaxed),
            self.stats.fallbacks.load(Ordering::Relaxed),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checkout_and_return() {
        let pool = BufferPool::new(4, 1024);
        {
            let mut buf = pool.checkout();
            buf.as_mut_vec().extend_from_slice(&[1.0, 2.0, 3.0]);
            assert_eq!(buf.len(), 3);
        }
        let (checkouts, fallbacks) = pool.stats();
        assert_eq!(checkouts, 1);
        assert_eq!(fallbacks, 0);
    }

    #[test]
    fn exhausted_pool_falls_back() {
        let pool = BufferPool::new(2, 64);
        let _a = pool.checkout();
        let _b = pool.checkout();
        let _c = pool.checkout(); // fallback
        let (checkouts, fallbacks) = pool.stats();
        assert_eq!(checkouts, 2);
        assert_eq!(fallbacks, 1);
    }

    #[test]
    fn returned_buffer_is_cleared() {
        let pool = BufferPool::new(1, 64);
        {
            let mut buf = pool.checkout();
            buf.as_mut_vec().extend_from_slice(&[1.0, 2.0]);
        }
        let buf = pool.checkout();
        assert!(buf.is_empty());
    }

    #[test]
    fn into_vec_takes_ownership() {
        let pool = BufferPool::new(2, 64);
        let mut buf = pool.checkout();
        buf.as_mut_vec().extend_from_slice(&[1.0, 2.0, 3.0]);
        let v = buf.into_vec();
        assert_eq!(v, vec![1.0, 2.0, 3.0]);
    }
}
