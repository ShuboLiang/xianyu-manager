//! 用例：标签的增删改查。

use std::sync::Arc;

use crate::domain::error::DomainError;
use crate::domain::repository::TagRepository;
use crate::domain::tag::{NewTag, Tag, TagName};

/// 更新标签的补丁：None 表示不修改该字段
#[derive(Debug, Default)]
pub struct TagPatch {
    pub name: Option<String>,
    pub enabled: Option<bool>,
    pub remark: Option<String>,
}

pub struct TagService {
    tags: Arc<dyn TagRepository>,
}

impl TagService {
    pub fn new(tags: Arc<dyn TagRepository>) -> Self {
        Self { tags }
    }

    pub async fn create_tag(
        &self,
        name: String,
        remark: Option<String>,
    ) -> Result<Tag, DomainError> {
        let name = TagName::new(name)?;
        self.ensure_name_available(name.as_str(), None).await?;
        let new_tag = NewTag {
            name,
            remark: normalize_remark(remark),
        };
        self.tags.create(&new_tag).await
    }

    pub async fn get_tag(&self, id: i64) -> Result<Tag, DomainError> {
        self.tags
            .find(id)
            .await?
            .ok_or_else(|| DomainError::NotFound(format!("标签 {id}")))
    }

    pub async fn list_tags(&self) -> Result<Vec<Tag>, DomainError> {
        self.tags.list().await
    }

    pub async fn update_tag(&self, id: i64, patch: TagPatch) -> Result<Tag, DomainError> {
        let mut tag = self.get_tag(id).await?;

        if let Some(enabled) = patch.enabled {
            tag.set_enabled(enabled);
        }
        if patch.name.is_some() || patch.remark.is_some() {
            let name = match patch.name {
                Some(n) => TagName::new(n)?,
                None => tag.name.clone(),
            };
            // 改名时校验新名字不与其他标签冲突
            if name != tag.name {
                self.ensure_name_available(name.as_str(), Some(id)).await?;
            }
            let remark = match patch.remark {
                Some(r) => normalize_remark(Some(r)),
                None => tag.remark.clone(),
            };
            tag.update_info(name, remark);
        }

        self.tags.update(&tag).await?;
        Ok(tag)
    }

    pub async fn delete_tag(&self, id: i64) -> Result<(), DomainError> {
        if !self.tags.delete(id).await? {
            return Err(DomainError::NotFound(format!("标签 {id}")));
        }
        Ok(())
    }

    /// 校验标签名未被占用；exclude_id 用于更新时排除自身
    async fn ensure_name_available(
        &self,
        name: &str,
        exclude_id: Option<i64>,
    ) -> Result<(), DomainError> {
        if let Some(existing) = self.tags.find_by_name(name).await? {
            if Some(existing.id) != exclude_id {
                return Err(DomainError::Conflict(format!("标签名「{name}」已存在")));
            }
        }
        Ok(())
    }
}

/// 空白的备注视为没有备注
fn normalize_remark(remark: Option<String>) -> Option<String> {
    remark.and_then(|r| {
        let r = r.trim().to_string();
        if r.is_empty() { None } else { Some(r) }
    })
}
