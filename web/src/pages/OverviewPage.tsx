import { Card, Col, Row, Statistic } from 'antd';
import { PageHeader } from '@/components/PageHeader';
import { fmtTime } from '@/lib/api';
import { useQueue } from '@/lib/queue';
import { useStats } from '@/lib/queries';
import { QueuesPanel } from '@/pages/QueuesPanel';

const ACTIVE_STATUSES = ['waiting', 'running', 'paused'];

export function OverviewPage() {
  const { queues } = useQueue();
  const { data: stats, isLoading } = useStats();

  const runningCount = queues.filter((q) => q.status === 'running').length;
  const pendingEntries = queues
    .filter((q) => ACTIVE_STATUSES.includes(q.status))
    .reduce((sum, q) => sum + q.pending + q.running, 0);

  return (
    <div>
      <PageHeader title="概览" description="抓取队列运行状态与核心指标" />
      <Row gutter={[12, 12]} style={{ marginBottom: 16 }}>
        <Col flex="1 1 160px">
          <Card>
            <Statistic title="运行中队列" value={runningCount} loading={isLoading} />
          </Card>
        </Col>
        <Col flex="1 1 160px">
          <Card>
            <Statistic title="待处理条目" value={pendingEntries} loading={isLoading} />
          </Card>
        </Col>
        <Col flex="1 1 160px">
          <Card>
            <Statistic title="商品总数" value={stats?.product_count ?? '-'} loading={isLoading} />
          </Card>
        </Col>
        <Col flex="1 1 160px">
          <Card>
            <Statistic title="近 24h 抓取" value={stats?.crawled_today ?? '-'} loading={isLoading} />
          </Card>
        </Col>
        <Col flex="1 1 160px">
          <Card>
            <Statistic
              title="最后爬取"
              value={stats ? fmtTime(stats.last_crawled_at) : '-'}
              loading={isLoading}
              valueStyle={{ fontSize: 15, fontFamily: "'IBM Plex Mono', ui-monospace, monospace" }}
            />
          </Card>
        </Col>
      </Row>
      <QueuesPanel />
    </div>
  );
}
