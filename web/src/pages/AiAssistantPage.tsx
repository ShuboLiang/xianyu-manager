import { useQuery } from '@tanstack/react-query';
import { AiChat } from '@/components/AiChat';
import { PageHeader } from '@/components/PageHeader';
import { apiGet } from '@/lib/api';
import type { AiStatus } from '@/types/api';

export function AiAssistantPage() {
  const { data: status } = useQuery({
    queryKey: ['aiStatus'],
    queryFn: () => apiGet<AiStatus>('/api/ai/status'),
  });

  return (
    <div>
      <PageHeader
        title="AI 助手"
        description="自然语言指令，AI 自主调用管理工具查改数据（商品 / 标签 / 队列 / 统计）"
      />
      <AiChat configured={status?.configured ?? false} />
    </div>
  );
}
