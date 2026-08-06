//! 用例：AI 助手会话。会话（Conversation）持久化对话历史，
//! chat 时把最近历史拼进 prompt，让 agent 感知上下文。

use std::sync::Arc;

use crate::application::ai::admin_tools::AdminToolsService;
use crate::domain::ai_conversation::{
    Conversation, ConversationMessage, MessageRole, NewConversationMessage,
};
use crate::domain::error::DomainError;
use crate::domain::repository::ConversationRepository;

pub struct ChatSessionService {
    conversations: Arc<dyn ConversationRepository>,
    admin_tools: Arc<AdminToolsService>,
}

impl ChatSessionService {
    pub fn new(
        conversations: Arc<dyn ConversationRepository>,
        admin_tools: Arc<AdminToolsService>,
    ) -> Self {
        Self {
            conversations,
            admin_tools,
        }
    }

    /// 新建会话（标题默认「新会话」，首条消息后自动改写）
    pub async fn create(&self) -> Result<Conversation, DomainError> {
        let conversation = self
            .conversations
            .create_conversation(&Conversation::new())
            .await?;
        tracing::info!("新建 AI 助手会话 #{}", conversation.id);
        Ok(conversation)
    }

    /// 全部会话 + 各自消息数（列表页用，避免 N+1）
    pub async fn list_with_counts(&self) -> Result<Vec<(Conversation, u64)>, DomainError> {
        let conversations = self.conversations.list_conversations().await?;
        let mut result = Vec::with_capacity(conversations.len());
        for c in conversations {
            let count = self.conversations.count_messages(c.id).await?;
            result.push((c, count));
        }
        Ok(result)
    }

    /// 会话消息数
    pub async fn message_count(&self, id: i64) -> Result<u64, DomainError> {
        self.conversations.count_messages(id).await
    }

    /// 会话详情 + 全部消息
    pub async fn get(&self, id: i64) -> Result<(Conversation, Vec<ConversationMessage>), DomainError> {
        let conversation = self
            .conversations
            .find_conversation(id)
            .await?
            .ok_or_else(|| DomainError::NotFound(format!("会话 {id}")))?;
        let messages = self.conversations.list_messages(id).await?;
        Ok((conversation, messages))
    }

    /// 手动改名
    pub async fn rename(&self, id: i64, title: String) -> Result<Conversation, DomainError> {
        let mut conversation = self
            .conversations
            .find_conversation(id)
            .await?
            .ok_or_else(|| DomainError::NotFound(format!("会话 {id}")))?;
        conversation.rename(&title)?;
        self.conversations.update_conversation(&conversation).await?;
        Ok(conversation)
    }

    /// 删除会话及其全部消息
    pub async fn delete(&self, id: i64) -> Result<(), DomainError> {
        if !self.conversations.delete_conversation(id).await? {
            return Err(DomainError::NotFound(format!("会话 {id}")));
        }
        tracing::info!("删除 AI 助手会话 #{id}");
        Ok(())
    }

    /// 清空会话的全部消息（保留会话本身），可用于重置对话上下文
    pub async fn clear(&self, id: i64) -> Result<(), DomainError> {
        self.conversations
            .find_conversation(id)
            .await?
            .ok_or_else(|| DomainError::NotFound(format!("会话 {id}")))?;
        self.conversations.clear_messages(id).await?;
        tracing::info!("清空 AI 助手会话 #{id} 的消息");
        Ok(())
    }

    /// 发一条用户消息：落库 → 带最近历史跑 agent → 落 AI 回复 → 返回回复文本。
    /// 首条消息时自动从用户消息生成会话标题。
    pub async fn chat(&self, id: i64, user_message: &str) -> Result<String, DomainError> {
        let message = user_message.trim().to_string();
        if message.is_empty() {
            return Err(DomainError::InvalidInput("消息不能为空".into()));
        }

        let mut conversation = self
            .conversations
            .find_conversation(id)
            .await?
            .ok_or_else(|| DomainError::NotFound(format!("会话 {id}")))?;

        // 读取已有消息作为历史（当前这条还没落库）
        let history = self.conversations.list_messages(id).await?;
        let history_pairs: Vec<(String, String)> = history
            .iter()
            .map(|m| (m.role.as_str().to_string(), m.content.clone()))
            .collect();

        // 落用户消息
        self.conversations
            .add_message(&NewConversationMessage {
                conversation_id: id,
                role: MessageRole::User,
                content: message.clone(),
            })
            .await?;

        // 首条消息：自动生成标题
        if history.is_empty() {
            conversation.auto_title_from(&message);
            self.conversations.update_conversation(&conversation).await?;
        }

        // 跑 agent（带历史）
        let reply = self
            .admin_tools
            .chat_with_history(&message, &history_pairs)
            .await?;

        // 落 AI 回复
        self.conversations
            .add_message(&NewConversationMessage {
                conversation_id: id,
                role: MessageRole::Assistant,
                content: reply.clone(),
            })
            .await?;

        Ok(reply)
    }
}
