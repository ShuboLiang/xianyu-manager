// 统一 API 请求封装：解包 ApiResponse<T>，code !== 0 时抛出后端 message

import type { ApiResponse } from '@/types/api';

export async function api<T>(path: string, options?: RequestInit): Promise<T> {
  const res = await fetch(path, {
    headers: { 'Content-Type': 'application/json' },
    ...options,
  });
  const body = (await res.json()) as ApiResponse<T>;
  if (body.code !== 0) {
    throw new Error(body.message);
  }
  return body.data as T;
}

export const apiGet = <T>(path: string) => api<T>(path);

export const apiPost = <T>(path: string, payload?: unknown) =>
  api<T>(path, {
    method: 'POST',
    body: payload === undefined ? undefined : JSON.stringify(payload),
  });

export const apiPut = <T>(path: string, payload: unknown) =>
  api<T>(path, { method: 'PUT', body: JSON.stringify(payload) });

export const apiDelete = <T>(path: string) => api<T>(path, { method: 'DELETE' });

// ---------- 展示格式化 ----------

export function fmtPrice(v: number | null | undefined): string {
  return v === null || v === undefined ? '-' : '¥' + v.toFixed(2);
}

export function fmtTime(unix: number | null | undefined): string {
  if (!unix) return '-';
  return new Date(unix * 1000).toLocaleString('zh-CN', { hour12: false });
}
