//! AI 助手会话实体：会话（Conversation）与消息（ConversationMessage）。
//! 会话持久化对话历史，前端可新建/切换/删除/改名，agent 对话时携带最近历史作为上下文。

use super::crawl_task::now_unix;
use super::error::DomainError;

/// 会话标题最大长度（字符）
pub const CONVERSATION_TITLE_MAX_LEN: usize = 40;

/// AI 助手会话（实体）
#[derive(Debug, Clone)]
pub struct Conversation {
    pub id: i64,
    pub title: String,
    /// Unix 秒
    pub created_at: u64,
    pub updated_at: u64,
}

impl Conversation {
    /// 新建会话：标题默认「新会话」，首条用户消息后自动改写为消息摘要
    pub fn new() -> Self {
        let now = now_unix();
        Self {
            id: 0,
            title: "新会话".to_string(),
            created_at: now,
            updated_at: now,
        }
    }

    /// 手动改名（非空、长度受限）
    pub fn rename(&mut self, title: &str) -> Result<(), DomainError> {
        let title = title.trim().to_string();
        if title.is_empty() {
            return Err(DomainError::InvalidInput("会话标题不能为空".into()));
        }
        if title.chars().count() > CONVERSATION_TITLE_MAX_LEN {
            return Err(DomainError::InvalidInput(format!(
                "会话标题最长 {CONVERSATION_TITLE_MAX_LEN} 个字符"
            )));
        }
        self.title = title;
        self.touch();
        Ok(())
    }

    /// 从第一条用户消息自动生成标题（首行截断，超长加省略号）
    pub fn auto_title_from(&mut self, message: &str) {
        let first_line = message.lines().next().unwrap_or("").trim();
        let mut chars: Vec<char> = first_line.chars().collect();
        if chars.is_empty() {
            return;
        }
        if chars.len() > CONVERSATION_TITLE_MAX_LEN {
            chars.truncate(CONVERSATION_TITLE_MAX_LEN - 1);
            self.title = format!("{}…", chars.into_iter().collect::<String>());
        } else {
            self.title = chars.into_iter().collect();
        }
        self.touch();
    }

    fn touch(&mut self) {
        self.updated_at = now_unix();
    }
}

impl Default for Conversation {
    fn default() -> Self {
        Self::new()
    }
}

/// 会话消息角色
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageRole {
    User,
    Assistant,
}

impl MessageRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Assistant => "assistant",
        }
    }

    pub fn from_str(s: &str) -> Result<Self, DomainError> {
        match s {
            "user" => Ok(Self::User),
            "assistant" => Ok(Self::Assistant),
            _ => Err(DomainError::Infrastructure(format!("非法消息角色: {s}"))),
        }
    }
}

/// 追加消息的入参（尚无 id）
#[derive(Debug, Clone)]
pub struct NewConversationMessage {
    pub conversation_id: i64,
    pub role: MessageRole,
    pub content: String,
}

/// 会话消息（实体）
#[derive(Debug, Clone)]
pub struct ConversationMessage {
    pub id: i64,
    /// 所属会话 id（消息列表按会话查询，该字段用于回查归属）
    #[allow(dead_code)]
    pub conversation_id: i64,
    pub role: MessageRole,
    pub content: String,
    /// Unix 秒
    pub created_at: u64,
}
