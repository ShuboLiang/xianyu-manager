//! AI 相关应用设置：用户自定义抓取提示词的读写（存 app_settings KV 表）。

use std::sync::Arc;

use crate::domain::error::DomainError;
use crate::domain::repository::SettingsRepository;

/// 用户自定义抓取提示词在 app_settings 表中的键
pub const CRAWL_PROMPT_KEY: &str = "crawl_custom_prompt";
/// AI 抓取模式在 app_settings 表中的键
pub const CRAWL_MODE_KEY: &str = "ai_crawl_mode";
/// 抓取模式：单轮调用（默认，省 token）
pub const CRAWL_MODE_DIRECT: &str = "direct";
/// 抓取模式：ReAct 工具循环（旧路径）
pub const CRAWL_MODE_AGENT: &str = "agent";
/// 提示词长度上限（字符数）
const MAX_PROMPT_LEN: usize = 2000;

pub struct AiSettingsService {
    settings: Arc<dyn SettingsRepository>,
    /// 未在 DB 设置抓取模式时的环境变量兜底（AI_CRAWL_MODE）
    env_crawl_mode: String,
}

impl AiSettingsService {
    pub fn new(settings: Arc<dyn SettingsRepository>, env_crawl_mode: String) -> Self {
        Self {
            settings,
            env_crawl_mode,
        }
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

    /// 读取当前生效的抓取模式（DB 设置 > 环境变量兜底）
    pub async fn get_crawl_mode(&self) -> Result<String, DomainError> {
        Ok(self
            .settings
            .get(CRAWL_MODE_KEY)
            .await?
            .filter(|v| !v.trim().is_empty())
            .unwrap_or_else(|| self.env_crawl_mode.clone()))
    }

    /// 保存抓取模式（仅允许 direct/agent；保存后下一轮抓取生效，无需重启）
    pub async fn save_crawl_mode(&self, mode: String) -> Result<String, DomainError> {
        let mode = mode.trim().to_string();
        if mode != CRAWL_MODE_DIRECT && mode != CRAWL_MODE_AGENT {
            return Err(DomainError::InvalidInput(format!(
                "非法抓取模式「{mode}」，只允许 {CRAWL_MODE_DIRECT} / {CRAWL_MODE_AGENT}"
            )));
        }
        self.settings.set(CRAWL_MODE_KEY, &mode).await?;
        Ok(mode)
    }
}
