//! Kimi WebBridge 客户端：通过本机 daemon（默认 http://127.0.0.1:10086）
//! 驱动用户真实浏览器（带登录态）访问闲鱼搜索页并提取商品列表。
//!
//! 这是真实爬虫的第一步：不碰 mtop 签名/Cookie，直接用浏览器里已登录的会话。
//! 抓取流程：navigate 到搜索页 → 等 SPA 渲染 → evaluate JS 提取卡片 → 解析为 Item。

use std::process::Stdio;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value as JsonValue;

use crate::application::ports::XianYuGateway;
use crate::domain::crawl_task::now_unix;
use crate::domain::error::DomainError;
use crate::domain::item::Item;

/// 单次搜索最多返回的候选条数（交给 AI 筛选）
pub const MAX_CANDIDATES: usize = 30;
/// 页面渲染等待：最多重试 4 次，每次间隔 2 秒
const LOAD_ATTEMPTS: u32 = 4;
const LOAD_WAIT: Duration = Duration::from_secs(2);

pub struct WebBridgeClient {
    client: reqwest::Client,
    base_url: String,
    /// WebBridge 会话名：同一任务的所有标签页归为一个标签组
    session: String,
}

impl WebBridgeClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
            base_url: base_url.into(),
            session: "xianyu-crawler".into(),
        }
    }

    /// 探测 WebBridge daemon 是否已可响应
    pub async fn is_running(&self) -> bool {
        match self.client.get(format!("{}/status", self.base_url)).send().await {
            Ok(resp) => resp.status().is_success(),
            Err(e) => {
                tracing::trace!("WebBridge 健康检查失败: {e}");
                false
            }
        }
    }

    /// 发送一条 WebBridge 命令并返回响应 JSON
    async fn command(&self, action: &str, args: JsonValue) -> Result<JsonValue, DomainError> {
        let body = serde_json::json!({
            "action": action,
            "args": args,
            "session": self.session,
        });
        tracing::trace!("WebBridge 请求: {}", body);
        let resp = self
            .client
            .post(format!("{}/command", self.base_url))
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                DomainError::Infrastructure(format!(
                    "WebBridge 连接失败（请确认 Kimi WebBridge 已启动且浏览器扩展已连接）: {e}"
                ))
            })?;
        let status = resp.status();
        let json: JsonValue = resp
            .json()
            .await
            .map_err(|e| DomainError::Infrastructure(format!("WebBridge 响应解析失败: {e}")))?;
        if !status.is_success() {
            return Err(DomainError::Infrastructure(format!(
                "WebBridge 命令 {action} 失败（HTTP {status}）: {json}"
            )));
        }
        // daemon 信封：{"ok": bool, "data": {...}}；先判 ok，再解出 data
        if json.get("ok") == Some(&JsonValue::Bool(false)) {
            return Err(DomainError::Infrastructure(format!(
                "WebBridge 命令 {action} 失败: {json}"
            )));
        }
        let data = json.get("data").cloned().unwrap_or(json);
        if data.get("success") == Some(&JsonValue::Bool(false)) {
            return Err(DomainError::Infrastructure(format!(
                "WebBridge 命令 {action} 失败: {data}"
            )));
        }
        tracing::trace!("WebBridge 命令 {action} 响应: {data}");
        Ok(data)
    }

    /// 在页面里执行 JS，返回其返回值（字符串）
    async fn evaluate_str(&self, code: &str) -> Result<String, DomainError> {
        tracing::trace!("WebBridge evaluate JS: {}", code);
        let resp = self
            .command("evaluate", serde_json::json!({ "code": code }))
            .await?;
        let value = resp
            .get("value")
            .cloned()
            .unwrap_or(JsonValue::Null);
        Ok(match value {
            JsonValue::String(s) => s,
            other => other.to_string(),
        })
    }

    /// 搜索闲鱼：打开搜索页，等渲染后提取候选商品（未筛选的原始列表）
    pub async fn search_xianyu(&self, keyword: &str) -> Result<Vec<Item>, DomainError> {
        let url = format!(
            "https://www.goofish.com/search?q={}",
            percent_encode(keyword)
        );
        tracing::debug!("WebBridge 导航到搜索页: {url}");
        self.command(
            "navigate",
            serde_json::json!({ "url": url, "group_title": "闲鱼数据抓取" }),
        )
        .await?;

        // SPA 需要渲染时间；提取为空则重试等待
        for attempt in 1..=LOAD_ATTEMPTS {
            tracing::debug!("WebBridge 等待渲染（第 {attempt}/{LOAD_ATTEMPTS} 次）");
            tokio::time::sleep(LOAD_WAIT).await;
            let raw = self.evaluate_str(EXTRACT_JS).await?;
            tracing::trace!("WebBridge 原始提取结果: {raw}");
            let items = parse_listings(&raw);
            if !items.is_empty() {
                tracing::info!(
                    "WebBridge 搜索「{keyword}」：第 {attempt} 次提取到 {} 条候选",
                    items.len()
                );
                return Ok(items.into_iter().take(MAX_CANDIDATES).collect());
            }
            tracing::debug!("WebBridge 搜索「{keyword}」第 {attempt} 次提取为空，继续等待");
        }
        tracing::warn!("WebBridge 搜索「{keyword}」{LOAD_ATTEMPTS} 次后仍未提取到商品");
        Err(DomainError::Infrastructure(format!(
            "闲鱼搜索「{keyword}」未提取到商品（页面未渲染完成、未登录或被风控，请检查浏览器标签组「闲鱼数据抓取」中的页面）"
        )))
    }
}

/// 尝试自动启动 WebBridge daemon。
/// 如果 daemon 已经在运行则直接返回；否则用 `bin_path start` 拉起，并轮询等待就绪。
pub async fn launch_webbridge_daemon(bin_path: &str, url: &str) -> Result<(), DomainError> {
    let probe = WebBridgeClient::new(url);
    if probe.is_running().await {
        tracing::info!("WebBridge daemon 已在运行 ({url})");
        return Ok(());
    }

    tracing::info!("WebBridge daemon 未响应，尝试从 {bin_path} 启动");
    let mut child = std::process::Command::new(bin_path)
        .arg("start")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| DomainError::Infrastructure(format!("启动 WebBridge 失败: {e}")))?;

    // 异步等待 daemon 就绪，最多 30 秒
    let start = std::time::Instant::now();
    let timeout = Duration::from_secs(30);
    let check_interval = Duration::from_millis(500);

    while start.elapsed() < timeout {
        match child.try_wait() {
            Ok(Some(status)) => {
                return Err(DomainError::Infrastructure(format!(
                    "WebBridge 进程已退出（code={}），请检查 {bin_path} 是否可执行",
                    status.code().unwrap_or(-1)
                )));
            }
            Ok(None) => {}
            Err(e) => {
                return Err(DomainError::Infrastructure(format!(
                    "等待 WebBridge 进程时出错: {e}"
                )));
            }
        }

        if probe.is_running().await {
            tracing::info!("WebBridge daemon 启动成功 ({url})");
            return Ok(());
        }
        tokio::time::sleep(check_interval).await;
    }

    let _ = child.kill();
    Err(DomainError::Infrastructure(
        "WebBridge daemon 在 30 秒内未能就绪，请手动检查".into(),
    ))
}

/// 让 WebBridgeClient 也能当普通网关用（CrawlService 等旧路径直接拿原始候选）
#[async_trait]
impl XianYuGateway for WebBridgeClient {
    async fn search(&self, keyword: &str, _page: u32) -> Result<Vec<Item>, DomainError> {
        self.search_xianyu(keyword).await
    }
}

/// 解析 evaluate 返回的 JSON 数组为 Item 列表；脏数据（无标题/价格<=0）直接丢弃
fn parse_listings(raw: &str) -> Vec<Item> {
    let Ok(JsonValue::Array(arr)) = serde_json::from_str::<JsonValue>(raw) else {
        tracing::trace!("parse_listings 无法解析为 JSON 数组: {raw}");
        return Vec::new();
    };
    tracing::trace!("parse_listings 原始数组长度: {}", arr.len());
    let now = now_unix();
    let items: Vec<Item> = arr
        .iter()
        .filter_map(|v| {
            let title = v.get("title")?.as_str()?.trim().to_string();
            let price = v.get("price")?.as_f64()?;
            let url = v.get("url")?.as_str()?.trim().to_string();
            let seller = v
                .get("seller")
                .and_then(|s| s.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            if title.is_empty() || price <= 0.0 || url.is_empty() {
                return None;
            }
            Some(Item {
                // 闲鱼条目没有独立 id 字段，用详情页 URL 当稳定标识
                id: url.clone(),
                title,
                price,
                seller,
                url,
                crawled_at: now,
                product_id: None,
            })
        })
        .collect();
    tracing::trace!("parse_listings 过滤后有效条目: {}/{}", items.len(), arr.len());
    items
}

/// 极简百分号编码（非 ASCII 与保留字符 → %XX），避免引入 url crate
fn percent_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.as_bytes() {
        match *b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// 页面内提取脚本：从搜索结果页的详情链接反推卡片，抓标题/价格/卖家。
/// 闲鱼 DOM 类名带 hash 不稳定，这里用「指向 item?id= 的链接」做锚点，
/// 在祖先容器里找 ¥ 价格与首行标题，尽量与具体类名解耦。
const EXTRACT_JS: &str = r#"(() => {
  const seen = new Set();
  const out = [];
  const anchors = document.querySelectorAll('a[href*="item?id="]');
  for (const a of anchors) {
    const href = (a.href || '').split('&')[0];
    if (!href || seen.has(href)) continue;
    seen.add(href);
    // 向上找一个包含价格的容器（最多 6 层）
    let card = a;
    let m = null;
    for (let i = 0; i < 6 && card; i++) {
      const text = card.innerText || '';
      m = text.match(/[¥￥]\s*([0-9]+(?:\.[0-9]+)?)/);
      if (m) break;
      card = card.parentElement;
    }
    if (!m || !card) continue;
    const lines = (card.innerText || '')
      .split('\n').map(s => s.trim()).filter(Boolean);
    const title = (a.getAttribute('title') || lines[0] || '').trim();
    if (!title) continue;
    // 卖家：搜索卡片只展示信用等级（如「卖家信用极好」），没有昵称
    const priceIdx = lines.findIndex(l => /[¥￥]/.test(l));
    const seller = lines.slice(priceIdx + 1).find(l => l.includes('信用')) || '';
    out.push({ title: title.slice(0, 120), price: parseFloat(m[1]), url: href, seller });
    if (out.length >= 40) break;
  }
  return JSON.stringify(out);
})()"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_listings_filters_dirty_rows() {
        let raw = r#"[
            {"title":"佳能 5D4 机身 95新","price":6500.0,"url":"https://www.goofish.com/item?id=1","seller":"数码老王"},
            {"title":"","price":100.0,"url":"https://www.goofish.com/item?id=2","seller":""},
            {"title":"无价格","price":0,"url":"https://www.goofish.com/item?id=3","seller":""},
            {"title":"无链接","price":100.0,"url":"","seller":""}
        ]"#;
        let items = parse_listings(raw);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "佳能 5D4 机身 95新");
        assert_eq!(items[0].price, 6500.0);
        assert_eq!(items[0].id, "https://www.goofish.com/item?id=1");
    }

    #[test]
    fn parse_listings_tolerates_garbage() {
        assert!(parse_listings("not json").is_empty());
        assert!(parse_listings("null").is_empty());
        assert!(parse_listings("[]").is_empty());
    }

    #[test]
    fn percent_encode_chinese() {
        assert_eq!(percent_encode("佳能 5D4"), "%E4%BD%B3%E8%83%BD%205D4");
        assert_eq!(percent_encode("a-b_c.d~e"), "a-b_c.d~e");
    }
}
