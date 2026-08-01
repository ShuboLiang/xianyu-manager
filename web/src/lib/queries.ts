// 全局共享数据的 react-query hooks；页面级分页查询（products/items/aiCalls）在各页面内定义
import { useQuery } from '@tanstack/react-query';
import { apiGet } from '@/lib/api';
import type { QueueProgress, StatsResponse, Tag } from '@/types/api';

export const useTags = () =>
  useQuery({ queryKey: ['tags'], queryFn: () => apiGet<Tag[]>('/api/tags') });

// 有 waiting/running 队列时每 2 秒自刷新（替代原 App.tsx 的手写轮询）
export const useQueues = () =>
  useQuery({
    queryKey: ['queues'],
    queryFn: () => apiGet<QueueProgress[]>('/api/queues'),
    refetchInterval: (query) =>
      query.state.data?.some((q) => q.status === 'waiting' || q.status === 'running') ? 2000 : false,
  });

export const useStats = () =>
  useQuery({ queryKey: ['stats'], queryFn: () => apiGet<StatsResponse>('/api/stats') });

export const useHealth = () =>
  useQuery({
    queryKey: ['health'],
    queryFn: () => apiGet<string>('/api/health'),
    retry: false,
    refetchInterval: 15_000,
  });
