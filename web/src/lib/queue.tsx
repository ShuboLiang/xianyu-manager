// 队列上下文：AppShell 提供实现，任意页面可入队/追加/读取队列状态
import { createContext, useContext } from 'react';
import type { QueueProgress, Selector } from '@/types/api';

export interface QueueCtx {
  queues: QueueProgress[];
  queuesLoading: boolean;
  /** 非 null 表示处于「向某队列追加条目」模式 */
  appendTarget: number | null;
  enterAppend: (queueId: number) => void;
  exitAppend: () => void;
  intervalSecs: number;
  setIntervalSecs: (v: number) => void;
  enqueue: (target: { selector: Selector } | { product_ids: number[] }) => Promise<boolean>;
}

export const QueueContext = createContext<QueueCtx | null>(null);

export function useQueue(): QueueCtx {
  const ctx = useContext(QueueContext);
  if (!ctx) throw new Error('useQueue must be used within QueueContext.Provider');
  return ctx;
}
