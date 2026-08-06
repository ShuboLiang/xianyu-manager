//! AI 驱动抓取·单轮调用实现（省 token 路径，AI_CRAWL_MODE=direct，默认）。
//!
//! 流程：Rust 直接用商品名搜索 → 候选不足才补一次袖珍调用换词重搜
//! → 一次 completion 让 AI 返回选中序号（JSON）→ Rust 校验、算统计、落库。
//! 与 ReAct 路径（crawl_agent_service）产出等价，但 LLM 调用从 3+ 轮降到 1 轮，
//! 候选列表只进一次 prompt，长 URL 不进 prompt（AI 返回序号，Rust 按序号取回）。

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value as JsonValue;

use crate::application::ai::crawl_shared::{finalize_crawl, CrawlOutcome, ProductCrawler, MAX_SELECTED};
use crate::application::ai_settings_service::CRAWL_PROMPT_KEY;
use crate::application::ports::{AiGateway, TokenUsage, XianYuGateway};
use crate::domain::ai_tool_call::source as ai_source;
use crate::domain::ai_tool_call::NewAiToolCall;
use crate::domain::crawl_task::now_unix;
use crate::domain::error::DomainError;
use crate::domain::item::Item;
use crate::domain::product::Product;
use crate::domain::repository::{
    AiToolCallRepository, ItemRepository, ProductRepository, SettingsRepository, TagRepository,
};

/// 进 prompt 的候选上限（搜索返回最多 30 条，截断到 20 条足够挑 8 个）
const MAX_CANDIDATES: usize = 20;
/// 候选少于该数时触发一次「换关键词重搜」的兜底调用
const MIN_CANDIDATES: usize = 5;
/// 候选标题进 prompt 的截断长度（存储用原标题，截断只为省 token）
const TITLE_MAX_CHARS: usize = 80;

pub struct CrawlDirectService {
    gateway: Arc<dyn XianYuGateway>,
    products: Arc<dyn ProductRepository>,
    items: Arc<dyn ItemRepository>,
    ai: Arc<dyn AiGateway>,
    settings: Arc<dyn SettingsRepository>,
    tags: Arc<dyn TagRepository>,
    /// 审计记录（工具名沿用 agent 路径的命名，用户回查时两种模式形态一致）
    calls: Arc<dyn AiToolCallRepository>,
    recycle_factor: f64,
}

/// AI 筛选返回的 JSON：选中候选的序号 + 可选回收价系数
#[derive(Debug, Deserialize)]
struct Selection {
    selected: Vec<usize>,
    recycle_factor: Option<f64>,
}

impl CrawlDirectService {
    pub fn new(
        gateway: Arc<dyn XianYuGateway>,
        products: Arc<dyn ProductRepository>,
        items: Arc<dyn ItemRepository>,
        ai: Arc<dyn AiGateway>,
        settings: Arc<dyn SettingsRepository>,
        tags: Arc<dyn TagRepository>,
        calls: Arc<dyn AiToolCallRepository>,
        recycle_factor: f64,
    ) -> Self {
        Self {
            gateway,
            products,
            items,
            ai,
            settings,
            tags,
            calls,
            recycle_factor,
        }
    }

    /// 落一条审计记录（尽力而为：失败只记日志，不影响抓取主流程）。
    /// usage 仅 LLM 调用行携带（crawl_select / refine_search_keyword），纯工具行传 None。
    async fn audit(
        &self,
        tool_name: &str,
        arguments: JsonValue,
        outcome: Result<JsonValue, &DomainError>,
        started: Instant,
        usage: Option<TokenUsage>,
    ) {
        let (result, error) = match outcome {
            Ok(v) => (Some(v.to_string()), None),
            Err(e) => (None, Some(e.to_string())),
        };
        let (input_tokens, output_tokens, cached_input_tokens) = match usage {
            Some(u) => (
                Some(u.input_tokens),
                Some(u.output_tokens),
                Some(u.cached_input_tokens),
            ),
            None => (None, None, None),
        };
        if let Err(e) = self
            .calls
            .create(&NewAiToolCall {
                tool_name: tool_name.to_string(),
                arguments: arguments.to_string(),
                result,
                error,
                duration_ms: started.elapsed().as_millis() as u64,
                input_tokens,
                output_tokens,
                cached_input_tokens,
                source: ai_source::CRAWL.to_string(),
            })
            .await
        {
            tracing::warn!("抓取审计记录写入失败：{e}");
        }
    }

    /// 搜索并截断到进 prompt 的候选上限（落 xianyu_search 审计，与 agent 路径同名）
    async fn search_candidates(&self, keyword: &str) -> Result<Vec<Item>, DomainError> {
        let started = Instant::now();
        let result = self.gateway.search(keyword, 1).await;
        self.audit(
            "xianyu_search",
            serde_json::json!({ "keyword": keyword }),
            result.as_ref().map(|items| serde_json::json!({ "count": items.len() })).map_err(|e| e),
            started,
            None,
        )
        .await;
        let mut items = result?;
        items.truncate(MAX_CANDIDATES);
        Ok(items)
    }

    /// 候选太少时问 AI 要一个更好的搜索关键词（袖珍调用，输入输出都极小；落审计）
    async fn refine_keyword(&self, product_name: &str, found: usize) -> Option<String> {
        let started = Instant::now();
        let system = "你是搜索关键词优化助手。只输出一个搜索关键词，不要输出任何其他内容。";
        let user = format!(
            "在闲鱼搜索「{product_name}」只找到 {found} 个在售结果。\
            给出一个更可能命中真实在售标题的搜索关键词（可去多余空格、补充型号、简化措辞），\
            只输出关键词本身。"
        );
        let completion = self.ai.complete(system, &user).await.ok()?;
        let keyword = completion
            .text
            .lines()
            .next()
            .unwrap_or("")
            .trim()
            .trim_matches(|c| c == '"' || c == '\'')
            .to_string();
        if keyword.is_empty() || keyword.chars().count() > 64 || keyword == product_name {
            return None;
        }
        self.audit(
            "refine_search_keyword",
            serde_json::json!({ "product_name": product_name, "found": found }),
            Ok(serde_json::json!({ "keyword": keyword })),
            started,
            completion.usage,
        )
        .await;
        Some(keyword)
    }

    /// 构造筛选提示词（静态内容在前、变量在后，利于供应商前缀缓存）
    async fn build_prompts(&self, product: &Product, candidates: &[Item]) -> Result<(String, String), DomainError> {
        let mut system = format!(
            "你是二手行情筛选助手。给你目标商品和一组闲鱼在售候选（每行一条：序号. 标题 ¥价格 · 卖家），\
            挑出最多 {MAX_SELECTED} 个有效商品：\n\
            - 标题必须与目标商品高度匹配（同型号/同规格）；\n\
            - 剔除配件、求购帖、明显不相关、价格异常（远高于或低于正常行情）的条目；\n\
            - 同等匹配度下优先选描述信息更完整、卖家信息更全的。\n\
            只输出一个 JSON 对象，不要输出任何其他文字或 markdown 代码块：\n\
            {{\"selected\": [选中的候选序号, 1-{MAX_SELECTED} 个, 按质量从高到低], \"recycle_factor\": null}}"
        );

        // 每次抓取读取最新用户自定义提示词（保存后下一轮即生效，无需重启）
        let custom_prompt = self
            .settings
            .get(CRAWL_PROMPT_KEY)
            .await?
            .unwrap_or_default();
        let custom_prompt = custom_prompt.trim();
        if !custom_prompt.is_empty() {
            system.push_str(&format!(
                "\n用户自定义定价与筛选规则（优先级高于默认规则）：\n\
                {custom_prompt}\n\
                若规则以折扣系数表达（如「CPU 类打八折」），且目标商品匹配规则，\
                在 JSON 的 recycle_factor 字段填该系数（(0,1]，如 0.8）；无匹配规则时填 null。"
            ));
        }

        let mut user = format!("目标商品：{}\n", product.name.as_str());
        let mut tag_names = Vec::new();
        for id in &product.tag_ids {
            if let Ok(Some(tag)) = self.tags.find(*id).await {
                tag_names.push(tag.name.as_str().to_string());
            }
        }
        if !tag_names.is_empty() {
            user.push_str(&format!("所属标签：{}\n", tag_names.join("、")));
        }
        user.push_str("候选列表：\n");
        for (i, it) in candidates.iter().enumerate() {
            let title: String = it.title.chars().take(TITLE_MAX_CHARS).collect();
            user.push_str(&format!("{i}. {title} ¥{} · {}\n", it.price, it.seller));
        }

        Ok((system, user))
    }

    /// 一次筛选调用 + JSON 解析（落 crawl_select 审计；参数只记候选数，不复制整份候选列表）
    async fn select_once(
        &self,
        system: &str,
        user: &str,
        candidate_count: usize,
    ) -> Result<Selection, DomainError> {
        let started = Instant::now();
        let mut usage = None;
        let result = match self.ai.complete(system, user).await {
            Ok(completion) => {
                usage = completion.usage;
                parse_selection(&completion.text)
            }
            Err(e) => Err(e),
        };
        self.audit(
            "crawl_select",
            serde_json::json!({ "candidates": candidate_count }),
            result
                .as_ref()
                .map(|s| {
                    serde_json::json!({
                        "selected": s.selected,
                        "recycle_factor": s.recycle_factor,
                    })
                })
                .map_err(|e| e),
            started,
            usage,
        )
        .await;
        result
    }
}

#[async_trait]
impl ProductCrawler for CrawlDirectService {
    async fn check_ai_available(&self) -> bool {
        self.ai.is_available().await
    }

    async fn crawl_product(&self, product: &Product) -> Result<CrawlOutcome, DomainError> {
        // 1. Rust 直接用商品名搜索；候选不足才问 AI 换词重搜一次
        let mut candidates = self.search_candidates(product.name.as_str()).await?;
        if candidates.len() < MIN_CANDIDATES {
            tracing::debug!(
                "商品 {} 候选仅 {} 条，触发换词重搜",
                product.id,
                candidates.len()
            );
            if let Some(better) = self.refine_keyword(product.name.as_str(), candidates.len()).await {
                tracing::debug!("商品 {} 换词重搜：{}", product.id, better);
                let retry = self.search_candidates(&better).await?;
                if retry.len() > candidates.len() {
                    candidates = retry;
                }
            }
        }
        if candidates.is_empty() {
            return Err(DomainError::InvalidState(format!(
                "商品「{}」未搜索到任何候选",
                product.name.as_str()
            )));
        }

        // 2. 一次 LLM 调用筛选；输出不是合法 JSON 时重试一次（兜底，非常态）
        let (system, user) = self.build_prompts(product, &candidates).await?;
        let selection = match self.select_once(&system, &user, candidates.len()).await {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("商品 {} AI 筛选输出解析失败（{e}），重试一次", product.id);
                self.select_once(&system, &user, candidates.len()).await?
            }
        };

        // 3. 校验序号：去重、越界丢弃、最多 MAX_SELECTED 个
        let mut seen = HashSet::new();
        let picked: Vec<usize> = selection
            .selected
            .iter()
            .copied()
            .filter(|&i| i < candidates.len() && seen.insert(i))
            .take(MAX_SELECTED)
            .collect();
        if picked.is_empty() {
            return Err(DomainError::InvalidState(format!(
                "商品「{}」AI 未选中任何有效商品",
                product.name.as_str()
            )));
        }

        // recycle_factor：非法值兜底为默认系数（不因此判失败）
        let factor = match selection.recycle_factor {
            Some(f) if f > 0.0 && f <= 1.0 => f,
            Some(f) => {
                tracing::warn!("商品 {} AI 返回非法 recycle_factor={f}，用默认系数", product.id);
                self.recycle_factor
            }
            None => self.recycle_factor,
        };

        // 4. 按序号取回完整字段（URL 不进 prompt，从本地候选还原），算统计并落库（落 save_crawl_result 审计）
        let now = now_unix();
        let items: Vec<Item> = picked
            .iter()
            .map(|&i| {
                let mut it = candidates[i].clone();
                it.product_id = Some(product.id);
                it.crawled_at = now;
                it
            })
            .collect();
        let started = Instant::now();
        let outcome_result = finalize_crawl(
            &self.products,
            &self.items,
            product.id,
            &items,
            factor,
        )
        .await;
        self.audit(
            "save_crawl_result",
            serde_json::json!({
                "items": items.iter().map(|i| serde_json::json!({
                    "title": i.title, "price": i.price, "seller": i.seller, "url": i.url,
                })).collect::<Vec<_>>(),
                "recycle_factor": factor,
            }),
            outcome_result.as_ref().map(|o| {
                serde_json::json!({
                    "saved": true,
                    "count": o.count,
                    "median_price": o.median_price,
                    "avg_price": o.avg_price,
                    "mode_price": o.mode_price,
                    "recycle_factor": factor,
                    "recycle_price": o.recycle_price,
                })
            }).map_err(|e| e),
            started,
            None,
        )
        .await;
        let outcome = outcome_result?;
        tracing::info!(
            "商品 {} 抓取完成（direct）：{} 条有效，中位数 {:.2}，回收价 {:.2}",
            product.id, outcome.count, outcome.median_price, outcome.recycle_price
        );
        Ok(outcome)
    }
}

/// 解析 AI 返回的筛选 JSON：容忍 markdown 代码块和前后多余文字
fn parse_selection(raw: &str) -> Result<Selection, DomainError> {
    let text = raw.trim();
    // 优先去掉 ```json ... ``` 围栏；否则截取第一个 { 到最后一个 }
    let json = if let Some(start) = text.find("```") {
        let body = text[start..]
            .trim_start_matches('`')
            .trim_start_matches("json");
        let body = match body.rfind("```") {
            Some(end) => &body[..end],
            None => body,
        };
        body.trim().to_string()
    } else {
        match (text.find('{'), text.rfind('}')) {
            (Some(s), Some(e)) if s < e => text[s..=e].to_string(),
            _ => text.to_string(),
        }
    };
    serde_json::from_str::<Selection>(&json)
        .map_err(|e| DomainError::InvalidState(format!("AI 筛选输出不是合法 JSON: {e}")))
}

#[cfg(test)]
mod tests {
    use super::parse_selection;

    #[test]
    fn parses_plain_json() {
        let s = parse_selection(r#"{"selected": [0, 3, 5], "recycle_factor": 0.85}"#).unwrap();
        assert_eq!(s.selected, vec![0, 3, 5]);
        assert_eq!(s.recycle_factor, Some(0.85));
    }

    #[test]
    fn parses_fenced_json() {
        let s = parse_selection("```json\n{\"selected\": [1], \"recycle_factor\": null}\n```").unwrap();
        assert_eq!(s.selected, vec![1]);
        assert_eq!(s.recycle_factor, None);
    }

    #[test]
    fn parses_json_with_surrounding_text() {
        let s = parse_selection("好的，筛选结果如下：\n{\"selected\": [2, 4]}\n以上是选中项。").unwrap();
        assert_eq!(s.selected, vec![2, 4]);
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_selection("没有有效商品").is_err());
    }
}
