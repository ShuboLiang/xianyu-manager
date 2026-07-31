use std::net::SocketAddr;

/// 全局配置，从环境变量读取，带默认值
#[derive(Debug, Clone)]
pub struct Config {
    pub host: String,
    pub port: u16,
    /// 前端静态文件目录
    pub static_dir: String,
    /// 网关实现：`mock` 使用假数据便于开发，`http` 走真实闲鱼接口（未实现）
    pub gateway: String,
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
        }
    }

    pub fn listen_addr(&self) -> SocketAddr {
        format!("{}:{}", self.host, self.port)
            .parse()
            .expect("非法的监听地址")
    }
}
