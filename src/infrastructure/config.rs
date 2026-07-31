use std::net::SocketAddr;

/// 全局配置，从环境变量读取，带默认值
#[derive(Debug, Clone)]
pub struct Config {
    pub host: String,
    pub port: u16,
    /// 前端静态文件目录
    pub static_dir: String,
    /// 网关实现：`mock` 使用假数据便于开发，`http`=真实闲鱼接口（未实现）
    pub gateway: String,
    /// SQLite 数据库文件路径
    pub database_path: String,
    /// AI 兜底配置（优先级：数据库默认配置 > 环境变量）
    pub ai_fallback: AiEnvFallback,
}

/// AI 环境变量兜底：未在数据库配置默认 provider 时生效
#[derive(Debug, Clone)]
pub struct AiEnvFallback {
    pub api_key: Option<String>,
    pub base_url: String,
    pub model: String,
}

impl Config {
    pub fn from_env() -> Self {
        Self {
            host: std::env::var("HOST").unwrap_or_else(|_| "127.0.0.1".into()),
            port: std::env::var("PORT")
                .ok()
                .and_then(|p| p.parse().ok())
                .unwrap_or(3000),
            static_dir: std::env::var("STATIC_DIR").unwrap_or_else(|_| "static".into()),
            gateway: std::env::var("GATEWAY").unwrap_or_else(|_| "mock".into()),
            database_path: std::env::var("DATABASE_PATH")
                .unwrap_or_else(|_| "data/xianyu.db".into()),
            ai_fallback: AiEnvFallback {
                api_key: std::env::var("AI_API_KEY").ok().filter(|k| !k.trim().is_empty()),
                base_url: std::env::var("AI_BASE_URL")
                    .unwrap_or_else(|_| "https://api.openai.com/v1".into()),
                model: std::env::var("AI_MODEL").unwrap_or_else(|_| "gpt-4o-mini".into()),
            },
        }
    }

    pub fn listen_addr(&self) -> SocketAddr {
        format!("{}:{}", self.host, self.port)
            .parse()
            .expect("非法的监听地址")
    }
}
