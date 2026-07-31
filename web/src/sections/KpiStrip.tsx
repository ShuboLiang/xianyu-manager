import { Card } from '@/components/ui/card';
import { Skeleton } from '@/components/ui/skeleton';
import { fmtTime } from '@/lib/api';
import type { Item, Product, QueueProgress } from '@/types/api';

interface Props {
  queues: QueueProgress[];
  products: Product[];
  items: Item[];
  loading?: boolean;
}

const ACTIVE_STATUSES = ['waiting', 'running', 'paused'];

interface Stat {
  label: string;
  value: string;
  small?: boolean;
}

export function KpiStrip({ queues, products, items, loading }: Props) {
  const runningCount = queues.filter((q) => q.status === 'running').length;
  const pendingEntries = queues
    .filter((q) => ACTIVE_STATUSES.includes(q.status))
    .reduce((sum, q) => sum + q.pending + q.running, 0);

  const todayStart = new Date();
  todayStart.setHours(0, 0, 0, 0);
  const crawledToday = items.filter((it) => it.crawled_at * 1000 >= todayStart.getTime()).length;

  const lastCrawled = products.reduce<number | null>(
    (max, p) => (p.last_crawled_at && (!max || p.last_crawled_at > max) ? p.last_crawled_at : max),
    null,
  );

  const stats: Stat[] = [
    { label: '运行中队列', value: String(runningCount) },
    { label: '待处理条目', value: String(pendingEntries) },
    { label: '商品总数', value: String(products.length) },
    { label: '今日抓取', value: String(crawledToday) },
    { label: '最后爬取', value: fmtTime(lastCrawled), small: true },
  ];

  return (
    <Card className="py-0">
      <div className="grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-5 lg:divide-x">
        {stats.map((s) => (
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
