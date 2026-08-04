use std::net::SocketAddr;

/// 全局配置，从环境变量读取，带默认值
#[derive(Debug, Clone)]
pub struct Config {
    pub host: String,
    pub port: u16,
    /// 前端静态文件目录
    pub static_dir: String,
    /// 网关实现：`mock` 假数据（开发）；`webbridge` 真实抓取（WebBridge 浏览器 + AI 筛选）；`http` 真实接口（未实现）
    pub gateway: String,
    /// Kimi WebBridge daemon 地址（GATEWAY=webbridge 时使用）
    pub webbridge_url: String,
    /// WebBridge 可执行文件路径；GATEWAY=webbridge 且 daemon 未启动时自动拉起
    pub webbridge_bin_path: Option<String>,
    /// 回收价系数：回收价 = 中位数 × 系数，默认 0.9
    pub recycle_factor: f64,
    /// AI 抓取实现：`direct` 单轮调用（默认，省 token）；`agent` ReAct 工具循环（旧路径，保留）
    pub ai_crawl_mode: String,
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
            gateway: std::env::var("GATEWAY").unwrap_or_else(|_| "webbridge".into()),
            webbridge_url: std::env::var("WEBBRIDGE_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:10086".into()),
            webbridge_bin_path: std::env::var("WEBBRIDGE_BIN_PATH")
                .ok()
                .filter(|p| !p.trim().is_empty())
                .or_else(default_webbridge_bin_path),
            recycle_factor: std::env::var("RECYCLE_FACTOR")
                .ok()
                .and_then(|v| v.parse::<f64>().ok())
                .filter(|f| *f > 0.0 && *f <= 1.0)
                .unwrap_or(0.9),
            ai_crawl_mode: std::env::var("AI_CRAWL_MODE")
                .unwrap_or_else(|_| "direct".into()),
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

/// 默认 WebBridge 可执行文件路径：
/// Windows: `%USERPROFILE%\.kimi-webbridge\bin\kimi-webbridge.exe`
/// 其他:    `~/.kimi-webbridge/bin/kimi-webbridge`
fn default_webbridge_bin_path() -> Option<String> {
    let home = if cfg!(windows) {
        std::env::var("USERPROFILE").ok()
    } else {
        std::env::var("HOME").ok()
    }?;
    let name = if cfg!(windows) {
        "kimi-webbridge.exe"
    } else {
        "kimi-webbridge"
    };
    Some(std::path::Path::new(&home)
        .join(".kimi-webbridge")
        .join("bin")
        .join(name)
        .to_string_lossy()
        .into_owned())
}
