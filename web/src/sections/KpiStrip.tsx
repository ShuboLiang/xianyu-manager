import { Card } from '@/components/ui/card';
import { Skeleton } from '@/components/ui/skeleton';
import { fmtTime } from '@/lib/api';
import type { QueueProgress, StatsResponse } from '@/types/api';

interface Props {
  stats: StatsResponse | null;
  queues: QueueProgress[];
  loading?: boolean;
}

const ACTIVE_STATUSES = ['waiting', 'running', 'paused'];

interface Stat {
  label: string;
  value: string;
  small?: boolean;
}

export function KpiStrip({ stats, queues, loading }: Props) {
  const runningCount = queues.filter((q) => q.status === 'running').length;
  const pendingEntries = queues
    .filter((q) => ACTIVE_STATUSES.includes(q.status))
    .reduce((sum, q) => sum + q.pending + q.running, 0);

  const statsItems: Stat[] = [
    { label: '运行中队列', value: String(runningCount) },
    { label: '待处理条目', value: String(pendingEntries) },
    { label: '商品总数', value: stats ? String(stats.product_count) : '-' },
    { label: '24h 抓取', value: stats ? String(stats.crawled_today) : '-' },
    { label: '最后爬取', value: stats ? fmtTime(stats.last_crawled_at) : '-', small: true },
  ];

  return (
    <Card className="py-0">
      <div className="grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-5 lg:divide-x">
        {statsItems.map((s) => (
          <div key={s.label} className="px-4 py-3.5">
            <p className="text-xs text-muted-foreground">{s.label}</p>
            {loading ? (
              <Skeleton className={`mt-1 ${s.small ? 'h-7 w-24' : 'h-8 w-12'}`} />
            ) : (
              <p
                className={`mt-1 font-data ${
                  s.small ? 'text-sm font-medium leading-7' : 'text-2xl font-semibold leading-8'
                }`}
              >
                {s.value}
              </p>
            )}
          </div>
        ))}
      </div>
    </Card>
  );
}
