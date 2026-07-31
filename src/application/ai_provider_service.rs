//! 用例：AI 供应商配置的增删改查、默认配置管理、连通性测试。
//! 配置解析优先级：数据库默认配置 > 环境变量兜底（见 AiEnvFallback）。

use std::sync::Arc;

use crate::application::ports::{AiEnvFallback, AiGateway};
use crate::domain::ai_provider::{AiProvider, NewAiProvider};
use crate::domain::error::DomainError;
use crate::domain::repository::AiProviderRepository;

/// 更新配置的补丁：None 表示不修改该字段；
/// api_key 特殊：None=保持不变，Some("")=清空，Some(v)=替换
#[derive(Debug, Default)]
pub struct AiProviderPatch {
    pub name: Option<String>,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub model: Option<String>,
    pub timeout_secs: Option<u32>,
    pub max_retries: Option<u32>,
}

/// 连通性测试结果
#[derive(Debug)]
pub struct TestConnectionResult {
    pub latency_ms: u64,
    pub reply: String,
}

/// 当前生效的 AI 配置来源
#[derive(Debug)]
pub struct AiStatus {
    pub configured: bool,
    /// "database" | "env"，未配置为 None
    pub source: Option<String>,
    /// 仅 database 来源时有值
    pub name: Option<String>,
    pub model: Option<String>,
}

pub struct AiProviderService {
    providers: Arc<dyn AiProviderRepository>,
    gateway: Arc<dyn AiGateway>,
    env_fallback: AiEnvFallback,
}

impl AiProviderService {
    pub fn new(
        providers: Arc<dyn AiProviderRepository>,
        gateway: Arc<dyn AiGateway>,
        env_fallback: AiEnvFallback,
    ) -> Self {
        Self {
            providers,
            gateway,
            env_fallback,
        }
    }

    pub async fn create_provider(
        &self,
        name: String,
        base_url: String,
        api_key: Option<String>,
        model: String,
        timeout_secs: u32,
        max_retries: u32,
    ) -> Result<AiProvider, DomainError> {
        let new_provider = NewAiProvider {
            name: name.trim().to_string(),
            base_url: base_url.trim().to_string(),
            api_key: normalize_key(api_key),
            model: model.trim().to_string(),
            timeout_secs: timeout_secs.max(1),
            max_retries,
        };
        new_provider.validate()?;
        self.ensure_name_available(&new_provider.name, None).await?;
        self.providers.create(&new_provider).await
    }

    pub async fn get_provider(&self, id: i64) -> Result<AiProvider, DomainError> {
        self.providers
            .find(id)
            .await?
            .ok_or_else(|| DomainError::NotFound(format!("AI 配置 {id}")))
    }

    pub async fn list_providers(&self) -> Result<Vec<AiProvider>, DomainError> {
        self.providers.list().await
    }

    pub async fn update_provider(
        &self,
        id: i64,
        patch: AiProviderPatch,
    ) -> Result<AiProvider, DomainError> {
        let mut provider = self.get_provider(id).await?;

        let name = match patch.name {
            Some(n) => n.trim().to_string(),
            None => provider.name.clone(),
        };
        if name != provider.name {
            self.ensure_name_available(&name, Some(id)).await?;
        }
        provider.update_info(
            name,
            patch.base_url.unwrap_or_else(|| provider.base_url.clone()),
            patch.api_key,
            patch.model.unwrap_or_else(|| provider.model.clone()),
            patch.timeout_secs.unwrap_or(provider.timeout_secs),
            patch.max_retries.unwrap_or(provider.max_retries),
        );
        if provider.name.is_empty() {
            return Err(DomainError::InvalidInput("配置名称不能为空".into()));
        }
        self.providers.update(&provider).await?;
        Ok(provider)
    }

    pub async fn delete_provider(&self, id: i64) -> Result<(), DomainError> {
        if !self.providers.delete(id).await? {
            return Err(DomainError::NotFound(format!("AI 配置 {id}")));
        }
        Ok(())
    }

    /// 设为默认：先清掉全部默认标记再置位，保证全局最多一条默认
    pub async fn set_default(&self, id: i64) -> Result<AiProvider, DomainError> {
        let mut provider = self.get_provider(id).await?;
        self.providers.clear_default().await?;
        provider.set_default(true);
        self.providers.update(&provider).await?;
        tracing::info!("AI 默认配置切换为 #{}（{}）", provider.id, provider.name);
        Ok(provider)
    }

    /// 连通性测试：用指定配置发极简 prompt，返回耗时与回复；
    /// 网络/鉴权错误原样透出
    pub async fn test_connection(&self, id: i64) -> Result<TestConnectionResult, DomainError> {
        let provider = self.get_provider(id).await?;
        if provider.api_key.as_ref().is_none_or(|k| k.is_empty()) {
            return Err(DomainError::InvalidState(format!(
                "配置「{}」未设置 API Key",
                provider.name
            )));
        }
        let started = std::time::Instant::now();
        let reply = self
            .gateway
            .complete_with(&provider, "You are a helpful assistant.", "回复 ok")
            .await?;
        Ok(TestConnectionResult {
            latency_ms: started.elapsed().as_millis() as u64,
            reply,
        })
    }

    /// 当前生效配置的来源：数据库默认 > 环境变量 > 未配置
    pub async fn status(&self) -> Result<AiStatus, DomainError> {
        if let Some(p) = self.providers.find_default().await? {
            return Ok(AiStatus {
                configured: true,
                source: Some("database".into()),
                name: Some(p.name),
                model: Some(p.model),
            });
        }
        if self.env_fallback.api_key.is_some() {
            return Ok(AiStatus {
                configured: true,
                source: Some("env".into()),
                name: None,
                model: None,
            });
        }
        Ok(AiStatus {
            configured: false,
            source: None,
            name: None,
            model: None,
        })
    }

    /// 校验配置名未被占用；exclude_id 用于更新时排除自身
    async fn ensure_name_available(
        &self,
        name: &str,
        exclude_id: Option<i64>,
    ) -> Result<(), DomainError> {
        if let Some(existing) = self.providers.find_by_name(name).await? {
            if Some(existing.id) != exclude_id {
                return Err(DomainError::Conflict(format!("配置名「{name}」已存在")));
            }
        }
        Ok(())
    }
}

/// 空白的密钥视为未设置
fn normalize_key(api_key: Option<String>) -> Option<String> {
    api_key.and_then(|k| {
        let k = k.trim().to_string();
        if k.is_empty() { None } else { Some(k) }
    })
}
