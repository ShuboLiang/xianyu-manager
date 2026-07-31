//! 领域错误类型，全项目统一的错误语义。
//! 不依赖任何外部框架（axum/reqwest/sqlx 一律不出现在这里）。

use std::fmt;

#[derive(Debug)]
pub enum DomainError {
    /// 输入不合法（空关键词、页数越界等）
    InvalidInput(String),
    /// 资源不存在
    NotFound(String),
    /// 状态不允许的操作（如对运行中的任务再次启动）
    InvalidState(String),
    /// 基础设施故障（网络、存储），由 infra 层转换而来
    Infrastructure(String),
}

impl fmt::Display for DomainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput(m) => write!(f, "输入不合法: {m}"),
            Self::NotFound(m) => write!(f, "资源不存在: {m}"),
            Self::InvalidState(m) => write!(f, "状态不允许: {m}"),
            Self::Infrastructure(m) => write!(f, "基础设施故障: {m}"),
        }
    }
}

impl std::error::Error for DomainError {}
