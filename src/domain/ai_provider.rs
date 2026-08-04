//! AI 供应商配置实体：OpenAI 兼容端点（DeepSeek/千问/Kimi 等通用）。
//! 多条配置存库、一条默认；密钥明文存本地 SQLite（个人本地工具），
//! 对外展示一律走 `masked_key()`。

use std::fmt;

use super::crawl_task::now_unix;
use super::error::DomainError;

/// 配置名（值对象）：非空、去除首尾空白、不超过 64 字符
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderName(String);

impl ProviderName {
    pub fn new(raw: impl Into<String>) -> Result<Self, DomainError> {
        let s = raw.into().trim().to_string();
        if s.is_empty() {
            return Err(DomainError::InvalidInput("配置名称不能为空".into()));
        }
        if s.chars().count() > 64 {
            return Err(DomainError::InvalidInput("配置名称过长（>64 字符）".into()));
        }
        Ok(Self(s))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ProviderName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// OpenAI 兼容端点 URL（值对象）：非空、去除首尾空白、不超过 512 字符
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BaseUrl(String);

impl BaseUrl {
    pub fn new(raw: impl Into<String>) -> Result<Self, DomainError> {
        let s = raw.into().trim().to_string();
        if s.is_empty() {
            return Err(DomainError::InvalidInput("base_url 不能为空".into()));
        }
        if s.chars().count() > 512 {
            return Err(DomainError::InvalidInput("base_url 过长（>512 字符）".into()));
        }
        Ok(Self(s))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for BaseUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// 模型名（值对象）：非空、去除首尾空白、不超过 128 字符
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelName(String);

impl ModelName {
    pub fn new(raw: impl Into<String>) -> Result<Self, DomainError> {
        let s = raw.into().trim().to_string();
        if s.is_empty() {
            return Err(DomainError::InvalidInput("模型名不能为空".into()));
        }
        if s.chars().count() > 128 {
            return Err(DomainError::InvalidInput("模型名过长（>128 字符）".into()));
        }
        Ok(Self(s))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ModelName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// 超时秒数校验：至少 1 秒（构造与更新共用，非法输入报错而非静默钳制）
fn validate_timeout(timeout_secs: u32) -> Result<(), DomainError> {
    if timeout_secs == 0 {
        return Err(DomainError::InvalidInput("超时时间至少 1 秒".into()));
    }
    Ok(())
}

/// 额外请求参数（值对象）：非空时必须是合法 JSON 对象，请求时透传给
/// OpenAI 兼容端点（如 DeepSeek 关思考 `{"thinking": {"type": "disabled"}}`、
/// 千问 `{"enable_thinking": false}`）。端点私有参数不写死在代码里，由用户按供应商文档配置。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtraParams(String);

impl ExtraParams {
    pub fn new(raw: impl Into<String>) -> Result<Self, DomainError> {
        let s = raw.into().trim().to_string();
        if s.is_empty() {
            return Err(DomainError::InvalidInput("额外请求参数不能为空".into()));
        }
        if s.chars().count() > 4096 {
            return Err(DomainError::InvalidInput("额外请求参数过长（>4096 字符）".into()));
        }
        let v: serde_json::Value = serde_json::from_str(&s)
            .map_err(|e| DomainError::InvalidInput(format!("额外请求参数不是合法 JSON: {e}")))?;
        if !v.is_object() {
            return Err(DomainError::InvalidInput(
                "额外请求参数必须是 JSON 对象".into(),
            ));
        }
        Ok(Self(s))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// 解析为 JSON（构造时已校验，不会失败）
    pub fn as_json(&self) -> serde_json::Value {
        serde_json::from_str(&self.0).expect("ExtraParams 构造时已校验为合法 JSON")
    }
}

impl fmt::Display for ExtraParams {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// 空白串归一化为 None；非空走 ExtraParams 校验
fn normalize_extra_params(raw: Option<String>) -> Result<Option<ExtraParams>, DomainError> {
    match raw {
        None => Ok(None),
        Some(s) if s.trim().is_empty() => Ok(None),
        Some(s) => Ok(Some(ExtraParams::new(s)?)),
    }
}

/// 创建 AI 供应商配置的入参（尚无 id）
#[derive(Debug, Clone)]
pub struct NewAiProvider {
    pub name: ProviderName,
    pub base_url: BaseUrl,
    pub api_key: Option<String>,
    pub model: ModelName,
    pub timeout_secs: u32,
    pub max_retries: u32,
    pub extra_params: Option<ExtraParams>,
}

impl NewAiProvider {
    /// 构造即校验：名称/端点/模型走值对象，超时至少 1 秒；
    /// extra_params 空白归一化为 None，非空必须是合法 JSON 对象
    pub fn new(
        name: String,
        base_url: String,
        api_key: Option<String>,
        model: String,
        timeout_secs: u32,
        max_retries: u32,
        extra_params: Option<String>,
    ) -> Result<Self, DomainError> {
        validate_timeout(timeout_secs)?;
        Ok(Self {
            name: ProviderName::new(name)?,
            base_url: BaseUrl::new(base_url)?,
            api_key,
            model: ModelName::new(model)?,
            timeout_secs,
            max_retries,
            extra_params: normalize_extra_params(extra_params)?,
        })
    }
}

/// AI 供应商配置（实体）
#[derive(Debug, Clone)]
pub struct AiProvider {
    pub id: i64,
    pub name: ProviderName,
    pub base_url: BaseUrl,
    /// 完整密钥，仅基础设施层构建 client 时使用；响应一律用 `masked_key()`
    pub api_key: Option<String>,
    pub model: ModelName,
    pub timeout_secs: u32,
    pub max_retries: u32,
    pub extra_params: Option<ExtraParams>,
    pub is_default: bool,
    /// Unix 秒
    pub created_at: u64,
    pub updated_at: u64,
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

    /// 部分字段更新；api_key 语义：None=保持不变，Some("")=清空，Some(v)=替换；
    /// extra_params 语义相同（Some(v) 必须是合法 JSON 对象）；
    /// 超时至少 1 秒，非法输入报错（不静默钳制）
    pub fn update_info(
        &mut self,
        name: ProviderName,
        base_url: BaseUrl,
        api_key: Option<String>,
        model: ModelName,
        timeout_secs: u32,
        max_retries: u32,
        extra_params: Option<String>,
    ) -> Result<(), DomainError> {
        validate_timeout(timeout_secs)?;
        self.name = name;
        self.base_url = base_url;
        match api_key {
            None => {}
            Some(k) if k.is_empty() => self.api_key = None,
            Some(k) => self.api_key = Some(k),
        }
        self.model = model;
        self.timeout_secs = timeout_secs;
        self.max_retries = max_retries;
        if let Some(raw) = extra_params {
            self.extra_params = normalize_extra_params(Some(raw))?;
        }
        self.touch();
        Ok(())
    }

    pub fn set_default(&mut self, is_default: bool) {
        self.is_default = is_default;
        self.touch();
    }

    fn touch(&mut self) {
        self.updated_at = now_unix();
    }
}
