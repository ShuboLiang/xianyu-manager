//! AI 供应商配置实体：OpenAI 兼容端点（DeepSeek/千问/Kimi 等通用）。
//! 多条配置存库、一条默认；密钥明文存本地 SQLite（个人本地工具），
//! 对外展示一律走 `masked_key()`。

use super::crawl_task::now_unix;
use super::error::DomainError;

/// 创建 AI 供应商配置的入参（尚无 id）
#[derive(Debug, Clone)]
pub struct NewAiProvider {
    pub name: String,
    pub base_url: String,
    pub api_key: Option<String>,
    pub model: String,
    pub timeout_secs: u32,
    pub max_retries: u32,
}

/// AI 供应商配置（实体）
#[derive(Debug, Clone)]
pub struct AiProvider {
    pub id: i64,
    pub name: String,
    pub base_url: String,
    /// 完整密钥，仅基础设施层构建 client 时使用；响应一律用 `masked_key()`
    pub api_key: Option<String>,
    pub model: String,
    pub timeout_secs: u32,
    pub max_retries: u32,
    pub is_default: bool,
    /// Unix 秒
    pub created_at: u64,
    pub updated_at: u64,
}

impl NewAiProvider {
    /// 构造校验：名称/端点/模型非空，超时至少 1 秒
    pub fn validate(&self) -> Result<(), DomainError> {
        if self.name.trim().is_empty() {
            return Err(DomainError::InvalidInput("配置名称不能为空".into()));
        }
        if self.base_url.trim().is_empty() {
            return Err(DomainError::InvalidInput("base_url 不能为空".into()));
        }
        if self.model.trim().is_empty() {
            return Err(DomainError::InvalidInput("模型名不能为空".into()));
        }
        if self.timeout_secs == 0 {
            return Err(DomainError::InvalidInput("超时时间至少 1 秒".into()));
        }
        Ok(())
    }
}

impl AiProvider {
    /// 密钥掩码：保留 `sk-` 前缀（如有）与后 4 位；未设置返回 None
    pub fn masked_key(&self) -> Option<String> {
        let key = self.api_key.as_ref().filter(|k| !k.is_empty())?;
        let tail: String = key
            .chars()
            .rev()
            .take(4)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        let prefix = if key.starts_with("sk-") { "sk-" } else { "" };
        Some(format!("{prefix}****{tail}"))
    }

    /// 部分字段更新；api_key 语义：None=保持不变，Some("")=清空，Some(v)=替换
    pub fn update_info(
        &mut self,
        name: String,
        base_url: String,
        api_key: Option<String>,
        model: String,
        timeout_secs: u32,
        max_retries: u32,
    ) {
        self.name = name;
        self.base_url = base_url;
        match api_key {
            None => {}
            Some(k) if k.is_empty() => self.api_key = None,
            Some(k) => self.api_key = Some(k),
        }
        self.model = model;
        self.timeout_secs = timeout_secs.max(1);
        self.max_retries = max_retries;
        self.touch();
    }

    pub fn set_default(&mut self, is_default: bool) {
        self.is_default = is_default;
        self.touch();
    }

    fn touch(&mut self) {
        self.updated_at = now_unix();
    }
}
