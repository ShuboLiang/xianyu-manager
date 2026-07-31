//! 共享的取消令牌存储，用于跨方法的 cancel 信号传递。
//! 存储 watch::Sender<bool>，cancel 时 send(true)。

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use tokio::sync::watch;

static CANCEL_TOKENS: LazyLock<Mutex<HashMap<String, watch::Sender<bool>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub struct CancelGuard {
    task_id: String,
}

impl Drop for CancelGuard {
    fn drop(&mut self) {
        let _ = CANCEL_TOKENS.lock().map(|mut m| m.remove(&self.task_id));
    }
}

pub fn register(task_id: String, tx: watch::Sender<bool>) -> CancelGuard {
    let _ = CANCEL_TOKENS.lock().map(|mut m| m.insert(task_id.clone(), tx));
    CancelGuard { task_id }
}

pub fn send(task_id: &str) {
    if let Ok(guard) = CANCEL_TOKENS.lock() {
        if let Some(tx) = guard.get(task_id) {
            let _ = tx.send(true);
        }
    }
}
