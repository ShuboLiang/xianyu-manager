// API 类型定义：全部由 ts-rs 从 src/interfaces/dto.rs 自动生成到 ./generated/，
// 本文件只做友好别名与少量收窄，不再手工维护字段。
// 重新生成：cargo test export_bindings（详见 AGENTS.md「前端约定」）

import type { AiProviderResponse } from './generated/AiProviderResponse';
import type { AiStatusResponse } from './generated/AiStatusResponse';
import type { AiToolCallResponse } from './generated/AiToolCallResponse';
import type { ClassifySuggestionDto } from './generated/ClassifySuggestionDto';
import type { ClassifyTaskResponse } from './generated/ClassifyTaskResponse';
import type { ClassifyWarningDto } from './generated/ClassifyWarningDto';
import type { CrawlPromptResponse } from './generated/CrawlPromptResponse';
import type { ItemResponse } from './generated/ItemResponse';
import type { ProductBriefResponse } from './generated/ProductBriefResponse';
import type { ProductResponse } from './generated/ProductResponse';
import type { QueueResponse } from './generated/QueueResponse';
import type { SelectorDto } from './generated/SelectorDto';
import type { TagResponse } from './generated/TagResponse';

// 泛型响应包装：Rust 侧 ApiResponse<T> 不经 ts-rs 导出（泛型），形状固定，手写即可
export interface ApiResponse<T> {
  code: number;
  message: string;
  data: T | null;
}

// 分页响应包装：同 ApiResponse，泛型不经 ts-rs 导出，手写
export interface PageResponse<T> {
  items: T[];
  total: number;
  page: number;
  page_size: number;
}

// ---------- 直接别名 ----------

export type Item = ItemResponse;
export type Tag = TagResponse;
export type Product = ProductResponse;
export type ProductBrief = ProductBriefResponse;
export type Selector = SelectorDto;
export type AiProvider = AiProviderResponse;
export type AiStatus = AiStatusResponse;
export type AiToolCall = AiToolCallResponse;
export type ClassifySuggestion = ClassifySuggestionDto;
export type ClassifyTask = ClassifyTaskResponse;
export type ClassifyWarning = ClassifyWarningDto;
export type CrawlPrompt = CrawlPromptResponse;

// ---------- 全局统计 ----------

export type { StatsResponse } from './generated/StatsResponse';

// ---------- 队列状态收窄为联合类型（Rust 侧是 String） ----------

export type QueueStatus = 'waiting' | 'running' | 'paused' | 'done' | 'cancelled';

export type QueueProgress = Omit<QueueResponse, 'status'> & { status: QueueStatus };

// ---------- 原样 re-export（同名类型） ----------

export type { BatchSkippedItem } from './generated/BatchSkippedItem';
export type { AiToolCallPurgePreviewResponse } from './generated/AiToolCallPurgePreviewResponse';
export type { AiToolCallPurgeRequest } from './generated/AiToolCallPurgeRequest';
export type { AiToolCallPurgeResponse } from './generated/AiToolCallPurgeResponse';
export type { ClassifyProductsResponse } from './generated/ClassifyProductsResponse';
export type { EnqueueResponse } from './generated/EnqueueResponse';
export type { ItemBatchDeletePreviewResponse } from './generated/ItemBatchDeletePreviewResponse';
export type { ItemBatchDeleteResponse } from './generated/ItemBatchDeleteResponse';
export type { PreviewResponse } from './generated/PreviewResponse';
export type { PriceTrendPoint } from './generated/PriceTrendPoint';
export type { PriceTrendSeries } from './generated/PriceTrendSeries';
export type { ProductBatchCreateResponse } from './generated/ProductBatchCreateResponse';
export type { ProductBatchDeletePreviewResponse } from './generated/ProductBatchDeletePreviewResponse';
export type { ProductBatchDeleteResponse } from './generated/ProductBatchDeleteResponse';
export type { TestConnectionResponse } from './generated/TestConnectionResponse';
