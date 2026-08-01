//! AI 相关应用设置：用户自定义抓取提示词的读写（存 app_settings KV 表）。

use std::sync::Arc;

use crate::domain::error::DomainError;
use crate::domain::repository::SettingsRepository;

/// 用户自定义抓取提示词在 app_settings 表中的键
pub const CRAWL_PROMPT_KEY: &str = "crawl_custom_prompt";
/// 提示词长度上限（字符数）
const MAX_PROMPT_LEN: usize = 2000;

pub struct AiSettingsService {
    settings: Arc<dyn SettingsRepository>,
}

impl AiSettingsService {
    pub fn new(settings: Arc<dyn SettingsRepository>) -> Self {
        Self { settings }
    }

    /// 读取自定义抓取提示词，未设置时返回空串
    pub async fn get_crawl_prompt(&self) -> Result<String, DomainError> {
        Ok(self
            .settings
            .get(CRAWL_PROMPT_KEY)
            .await?
            .unwrap_or_default())
    }

    /// 保存自定义抓取提示词（trim 后整体覆盖；空串 = 清空）
    pub async fn save_crawl_prompt(&self, prompt: String) -> Result<String, DomainError> {
        let prompt = prompt.trim().to_string();
        if prompt.chars().count() > MAX_PROMPT_LEN {
            return Err(DomainError::InvalidInput(format!(
                "提示词过长（最多 {MAX_PROMPT_LEN} 字）"
            )));
        }
        self.settings.set(CRAWL_PROMPT_KEY, &prompt).await?;
        Ok(prompt)
    }
}
