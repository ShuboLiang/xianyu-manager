import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  App as AntApp,
  Alert,
  Badge,
  Button,
  Layout,
  Menu,
  Tag,
  theme as antdTheme,
  Tooltip,
} from 'antd';
import {
  DashboardOutlined,
  InboxOutlined,
  LineChartOutlined,
  MoonOutlined,
  RobotOutlined,
  ShoppingOutlined,
  SunOutlined,
  TagsOutlined,
} from '@ant-design/icons';
import { useQueryClient } from '@tanstack/react-query';
import { Outlet, useLocation, useNavigate } from 'react-router';
import { apiPost } from '@/lib/api';
import { QueueContext, type QueueCtx } from '@/lib/queue';
import { useHealth, useQueues } from '@/lib/queries';
import { useThemeMode } from '@/lib/theme-mode';
import type { EnqueueResponse, Selector } from '@/types/api';

const NAV_ITEMS = [
  { key: '/', icon: <DashboardOutlined />, label: '概览' },
  { key: '/products', icon: <ShoppingOutlined />, label: '商品管理' },
  { key: '/tags', icon: <TagsOutlined />, label: '标签管理' },
  { key: '/items', icon: <InboxOutlined />, label: '抓取数据' },
  { key: '/trends', icon: <LineChartOutlined />, label: '价格趋势' },
  { type: 'divider' as const },
  { key: '/ai', icon: <RobotOutlined />, label: 'AI 配置' },
];

export function AppShell() {
  const { message } = AntApp.useApp();
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const location = useLocation();
  const { mode, toggle } = useThemeMode();
  const { token } = antdTheme.useToken();

  const { data: queues = [], isLoading: queuesLoading } = useQueues();
  const health = useHealth();

  const [appendTarget, setAppendTarget] = useState<number | null>(null);
  const [intervalSecs, setIntervalSecs] = useState(3);

  // 队列刚全部结束时补刷商品统计、原始数据与 KPI（替代原 loadQueues 里的手写逻辑）
  const wasActive = useRef(false);
  useEffect(() => {
    const active = queues.some((q) => q.status === 'waiting' || q.status === 'running');
    if (!active && wasActive.current) {
      queryClient.invalidateQueries({ queryKey: ['products'] });
      queryClient.invalidateQueries({ queryKey: ['items'] });
      queryClient.invalidateQueries({ queryKey: ['stats'] });
    }
    wasActive.current = active;
  }, [queues, queryClient]);

  const enqueue = useCallback(
    async (target: { selector: Selector } | { product_ids: number[] }): Promise<boolean> => {
      const isAppend = appendTarget !== null;
      const url = isAppend ? `/api/queues/${appendTarget}/entries` : '/api/queues';
      try {
        const data = await apiPost<EnqueueResponse>(url, { ...target, interval_secs: intervalSecs });
        let msg = `${isAppend ? '追加' : '入队'}成功：新增 ${data.added.length} 个`;
        if (data.skipped.length) msg += `，跳过 ${data.skipped.length} 个（已在队列）`;
        if (!isAppend && data.status === 'waiting') msg += '；已有队列在执行，本队列将自动排队执行';
        message.success(msg);
        if (isAppend) setAppendTarget(null);
        queryClient.invalidateQueries({ queryKey: ['queues'] });
        return true;
      } catch (e) {
        message.error(`${isAppend ? '追加' : '入队'}失败: ${(e as Error).message}`);
        return false;
      }
    },
    [appendTarget, intervalSecs, message, queryClient],
  );

  const queueCtx: QueueCtx = useMemo(
    () => ({
      queues,
      queuesLoading,
      appendTarget,
      enterAppend: (id) => {
        setAppendTarget(id);
        navigate('/');
      },
      exitAppend: () => setAppendTarget(null),
      intervalSecs,
      setIntervalSecs,
      enqueue,
    }),
    [queues, queuesLoading, appendTarget, intervalSecs, enqueue, navigate],
  );

  // Header 左侧的队列状态指示：运行中 > 排队中 > 无
  const runningQueue = queues.find((q) => q.status === 'running');
  const waitingCount = queues.filter((q) => q.status === 'waiting').length;
  const queueIndicator = runningQueue ? (
    <Button type="text" size="small" onClick={() => navigate('/')}>
      <Badge status="processing" />
      <span className="num">
        队列 #{runningQueue.id} 执行中 {runningQueue.done + runningQueue.failed + runningQueue.skipped}/
        {runningQueue.total}
      </span>
    </Button>
  ) : waitingCount > 0 ? (
    <Button type="text" size="small" onClick={() => navigate('/')}>
      <Badge status="warning" />
      {waitingCount} 个队列排队中
    </Button>
  ) : null;

  const healthTag = health.isLoading ? (
    <Tag>检测服务中…</Tag>
  ) : health.isSuccess ? (
    <Tag color="success">服务正常</Tag>
  ) : (
    <Tag color="error">服务不可用</Tag>
  );

  const selectedKey =
    NAV_ITEMS.filter((i) => 'key' in i)
      .map((i) => (i as { key: string }).key)
      .filter((k) => (k === '/' ? location.pathname === '/' : location.pathname.startsWith(k)))[0] ??
    '/';

  return (
    <QueueContext.Provider value={queueCtx}>
      <Layout style={{ minHeight: '100vh' }}>
        <Layout.Sider
          width={200}
          breakpoint="lg"
          collapsedWidth={0}
          // 不用 theme="dark"：那是 antd 旧版深蓝（#001529）菜单，与 darkAlgorithm 的中性深色不协调；
          // light 主题走 cssVar，深浅色下都自动取 colorBgContainer
          theme="light"
          style={{
            background: token.colorBgContainer,
            borderRight: `1px solid ${token.colorBorderSecondary}`,
          }}
        >
          <div style={{ padding: '16px 20px 12px' }}>
            <div style={{ fontSize: 16, fontWeight: 600 }}>闲鱼管理台</div>
            <div style={{ fontSize: 12, color: token.colorTextSecondary, marginTop: 2 }}>
              二手行情监控
            </div>
          </div>
          <Menu
            mode="inline"
            theme="light"
            selectedKeys={[selectedKey]}
            items={NAV_ITEMS}
            onClick={({ key }) => navigate(key)}
            style={{ borderInlineEnd: 'none', background: 'transparent' }}
          />
        </Layout.Sider>

        <Layout>
          <Layout.Header
            style={{
              position: 'sticky',
              top: 0,
              zIndex: 20,
              display: 'flex',
              alignItems: 'center',
              gap: 8,
              height: 48,
              lineHeight: 'normal',
              paddingInline: 16,
              background: token.colorBgContainer,
              borderBottom: `1px solid ${token.colorBorderSecondary}`,
            }}
          >
            {queueIndicator}
            <span style={{ flex: 1 }} />
            {healthTag}
            <Tooltip title={mode === 'dark' ? '切换为浅色模式' : '切换为深色模式'}>
              <Button
                type="text"
                size="small"
                icon={mode === 'dark' ? <SunOutlined /> : <MoonOutlined />}
                onClick={toggle}
                style={{ width: 32 }}
              />
            </Tooltip>
          </Layout.Header>

          <Layout.Content style={{ padding: '16px 24px 32px', minWidth: 0 }}>
            {health.isError && (
              <Alert
                type="error"
                showIcon
                style={{ marginBottom: 16 }}
                message="后端服务不可用"
                description="无法连接后端接口，请确认服务已启动（cargo run，默认 http://127.0.0.1:3000）。"
                action={
                  <Button size="small" onClick={() => queryClient.invalidateQueries()}>
                    重试连接
                  </Button>
                }
              />
            )}
            <Outlet />
          </Layout.Content>
        </Layout>
      </Layout>
    </QueueContext.Provider>
  );
}
