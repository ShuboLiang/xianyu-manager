//! AI 助手写操作的人工确认闸口（应用层用例，进程内存态）。
//!
//! 每个会话（conversation）一个审批上下文：
//! - 模式：normal（写工具需确认）/ yolo（全部放行，不干预）；
//! - 「该对话全部允许」：允许后该会话内该工具不再询问；
//! - 其余写工具调用创建一条待确认审批，阻塞等待用户决策（允许本次 / 全部允许 / 拒绝），
//!   前端轮询到审批后弹框，决策经接口回写唤醒被阻塞的 agent 循环。
//!
//! 决策超时（`DECISION_TIMEOUT`）自动按拒绝处理，避免 agent 无限阻塞。
//! 会话模式不持久化（进程重启回到 normal，可接受）。

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::{watch, Mutex};

use crate::application::ports::ToolApproval;
use crate::domain::crawl_task::now_unix;
use crate::domain::error::DomainError;

/// 等待用户决策的最长时间，超时自动按拒绝处理
const DECISION_TIMEOUT: Duration = Duration::from_secs(300);

/// 会话审批模式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalMode {
    /// 写工具需人工确认
    Normal,
    /// 全部放行，无任何干预
    Yolo,
}

impl ApprovalMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Yolo => "yolo",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "normal" => Some(Self::Normal),
            "yolo" => Some(Self::Yolo),
            _ => None,
        }
    }
}

/// 用户对一条待确认审批的决策
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalDecision {
    /// 允许本次（仅放行这一次）
    AllowOnce,
    /// 该对话全部允许（放行本次，且该会话内该工具不再询问）
    AllowAlways,
    /// 拒绝（本次不执行，结果回填模型说明被拒）
    Deny,
}

impl ApprovalDecision {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "allow_once" => Some(Self::AllowOnce),
            "allow_always" => Some(Self::AllowAlways),
            "deny" => Some(Self::Deny),
            _ => None,
        }
    }
}

/// 一条待用户确认的审批（前端弹框展示用）
#[derive(Debug, Clone)]
pub struct PendingApproval {
    pub id: u64,
    pub conversation_id: i64,
    pub tool_name: String,
    /// 调用参数（JSON 字符串，弹框展示）
    pub arguments: String,
    pub created_at: u64,
}

struct PendingSlot {
    info: PendingApproval,
    /// 决策发送端：check() 阻塞等待它收到 Some(决策)
    tx: watch::Sender<Option<ApprovalDecision>>,
}

#[derive(Default)]
struct Inner {
    modes: HashMap<i64, ApprovalMode>,
    /// 会话内已「该对话全部允许」的工具名
    allowed_tools: HashMap<i64, HashSet<String>>,
    pendings: HashMap<u64, PendingSlot>,
}

/// 写操作确认闸口注册中心：管理各会话的模式/授权与待确认审批。
/// inner 与 next_id 均为 Arc，`handle()` 产出的会话句柄与之共享状态。
#[derive(Clone)]
pub struct ToolApprovalRegistry {
    inner: Arc<Mutex<Inner>>,
    next_id: Arc<AtomicU64>,
}

impl Default for ToolApprovalRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolApprovalRegistry {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner::default())),
            next_id: Arc::new(AtomicU64::new(1)),
        }
    }

    /// 会话审批上下文句柄（实现 ToolApproval，供 run_agent 使用）
    pub fn handle(&self, conversation_id: i64) -> Arc<dyn ToolApproval> {
        Arc::new(ConversationApproval {
            conversation_id,
            inner: self.inner.clone(),
            next_id: self.next_id.clone(),
        })
    }

    pub async fn set_mode(&self, conversation_id: i64, mode: ApprovalMode) {
        self.inner.lock().await.modes.insert(conversation_id, mode);
    }

    pub async fn get_mode(&self, conversation_id: i64) -> ApprovalMode {
        self.inner
            .lock()
            .await
            .modes
            .get(&conversation_id)
            .copied()
            .unwrap_or(ApprovalMode::Normal)
    }

    /// 某会话当前待确认的审批（按创建时间正序，前端弹框逐个处理）
    pub async fn list_pending(&self, conversation_id: i64) -> Vec<PendingApproval> {
        let inner = self.inner.lock().await;
        let mut pendings: Vec<PendingApproval> = inner
            .pendings
            .values()
            .filter(|s| s.info.conversation_id == conversation_id)
            .map(|s| s.info.clone())
            .collect();
        pendings.sort_by_key(|p| p.id);
        pendings
    }

    /// 用户对某条审批作出决策；唤醒被阻塞的 check()，并将审批移出待确认列表
    pub async fn decide(&self, id: u64, decision: ApprovalDecision) -> Result<(), DomainError> {
        let slot = self.inner.lock().await.pendings.remove(&id);
        match slot {
            Some(slot) => {
                // 全部允许：本次放行后该工具在该会话内不再询问
                if decision == ApprovalDecision::AllowAlways {
                    self.inner
                        .lock()
                        .await
                        .allowed_tools
                        .entry(slot.info.conversation_id)
                        .or_default()
                        .insert(slot.info.tool_name);
                }
                let _ = slot.tx.send(Some(decision));
                Ok(())
            }
            None => Err(DomainError::NotFound(format!("待确认的审批 #{id}"))),
        }
    }

    /// 清掉某会话残留的待确认审批（拒绝并移除）。新对话开始时调用，
    /// 防止上次请求中断（客户端断开）遗留的审批卡住后续轮询。
    pub async fn reset(&self, conversation_id: i64) {
        let ids: Vec<u64> = {
            let inner = self.inner.lock().await;
            inner
                .pendings
                .values()
                .filter(|s| s.info.conversation_id == conversation_id)
                .map(|s| s.info.id)
                .collect()
        };
        for id in ids {
            let _ = self.decide(id, ApprovalDecision::Deny).await;
        }
    }
}

/// 核心检查逻辑（注册中心与会话句柄共用）：写工具执行前调用。
/// yolo 模式 / 已授权工具直接放行；否则创建待确认审批并阻塞等待用户决策，超时按拒绝处理。
async fn check_approval(
    inner: &Arc<Mutex<Inner>>,
    next_id: &AtomicU64,
    conversation_id: i64,
    tool_name: &str,
    arguments: &str,
) -> Result<bool, DomainError> {
    {
        let inner = inner.lock().await;
        if inner.modes.get(&conversation_id) == Some(&ApprovalMode::Yolo) {
            return Ok(true);
        }
        if inner
            .allowed_tools
            .get(&conversation_id)
            .map(|s| s.contains(tool_name))
            .unwrap_or(false)
        {
            return Ok(true);
        }
    }

    let id = next_id.fetch_add(1, Ordering::Relaxed);
    let (tx, mut rx) = watch::channel(None);
    let info = PendingApproval {
        id,
        conversation_id,
        tool_name: tool_name.to_string(),
        arguments: arguments.to_string(),
        created_at: now_unix(),
    };
    inner
        .lock()
        .await
        .pendings
        .insert(id, PendingSlot { info, tx });

    tracing::info!("AI 助手写操作 #{id} 待确认: {tool_name}");
    let decision = tokio::time::timeout(DECISION_TIMEOUT, rx.wait_for(|d| d.is_some()))
        .await
        .ok()
        .and_then(|r| r.ok())
        .map(|d| *d)
        .flatten()
        .unwrap_or(ApprovalDecision::Deny);

    // 决策方已把审批移出；若超时/异常则这里兜底移除
    inner.lock().await.pendings.remove(&id);
    if decision == ApprovalDecision::AllowAlways {
        inner
            .lock()
            .await
            .allowed_tools
            .entry(conversation_id)
            .or_default()
            .insert(tool_name.to_string());
    }

    tracing::info!("AI 助手写操作 #{id} 决策: {:?}", decision);
    Ok(decision != ApprovalDecision::Deny)
}

/// 某个会话的审批上下文（ToolApproval 实现）
struct ConversationApproval {
    conversation_id: i64,
    inner: Arc<Mutex<Inner>>,
    next_id: Arc<AtomicU64>,
}

#[async_trait]
impl ToolApproval for ConversationApproval {
    async fn check(&self, tool_name: &str, arguments: &str) -> Result<bool, DomainError> {
        check_approval(
            &self.inner,
            &self.next_id,
            self.conversation_id,
            tool_name,
            arguments,
        )
        .await
    }
}
