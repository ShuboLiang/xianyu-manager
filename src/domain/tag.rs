//! 标签实体：管理「爬虫爬哪一类商品」。
//! 后续可按标签挂抓取策略（关键词、频率、页数、过滤规则等），
//! 当前版本只维护标签本身的基础信息，`enabled=false` 的标签不参与抓取。

use super::crawl_task::now_unix;
use super::error::DomainError;

/// 标签名（值对象）：非空、去除首尾空白、不超过 32 字符
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagName(String);

impl TagName {
    pub fn new(raw: impl Into<String>) -> Result<Self, DomainError> {
        let s = raw.into().trim().to_string();
        if s.is_empty() {
            return Err(DomainError::InvalidInput("标签名不能为空".into()));
        }
        if s.chars().count() > 32 {
            return Err(DomainError::InvalidInput("标签名过长（>32 字符）".into()));
        }
        Ok(Self(s))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// 创建标签的入参（尚无 id）
#[derive(Debug, Clone)]
pub struct NewTag {
    pub name: TagName,
    pub remark: Option<String>,
}

/// 标签（实体）
#[derive(Debug, Clone)]
pub struct Tag {
    pub id: i64,
    pub name: TagName,
    /// 是否启用：禁用的标签不参与抓取
    pub enabled: bool,
    pub remark: Option<String>,
    /// Unix 秒
    pub created_at: u64,
    pub updated_at: u64,
}

impl Tag {
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        self.touch();
    }

    pub fn update_info(&mut self, name: TagName, remark: Option<String>) {
        self.name = name;
        self.remark = remark;
        self.touch();
    }

    fn touch(&mut self) {
        self.updated_at = now_unix();
    }
}
