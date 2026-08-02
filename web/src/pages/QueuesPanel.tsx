import { useEffect, useRef, useState } from 'react';
import {
  App as AntApp,
  Alert,
  Badge,
  Button,
  Card,
  Checkbox,
  Collapse,
  Divider,
  Empty,
  InputNumber,
  Progress,
  Space,
  Table,
  Tag,
  Typography,
} from 'antd';
import { useQueryClient } from '@tanstack/react-query';
import { apiDelete, apiPost, fmtTime } from '@/lib/api';
import { useQueue } from '@/lib/queue';
import { useTags } from '@/lib/queries';
import type { PreviewResponse, QueueProgress, QueueStatus, Selector } from '@/types/api';

export const QUEUE_STATUS_TEXT: Record<QueueStatus, string> = {
  waiting: '排队中',
  running: '执行中',
  paused: '已暂停',
  done: '已完成',
  cancelled: '已取消',
};

const QUEUE_STATUS_COLOR: Record<QueueStatus, string> = {
  waiting: 'default',
  running: 'processing',
  paused: 'warning',
  done: 'success',
  cancelled: 'default',
};

const ACTIVE_STATUSES: QueueStatus[] = ['waiting', 'running', 'paused'];

/** 预计剩余时间：剩余条目 × 间隔秒数 → 人性化时长 */
function fmtEta(seconds: number): string {
  if (seconds < 60) return `${seconds} 秒`;
  if (seconds < 3600) return `${Math.floor(seconds / 60)} 分 ${seconds % 60} 秒`;
  return `${Math.floor(seconds / 3600)} 小时 ${Math.round((seconds % 3600) / 60)} 分`;
}

export function QueuesPanel() {
  const { message, modal } = AntApp.useApp();
  const queryClient = useQueryClient();
  const { data: tags = [] } = useTags();
  const {
    queues,
    queuesLoading,
    appendTarget,
    enterAppend,
    exitAppend,
    intervalSecs,
    setIntervalSecs,
    enqueue,
  } = useQueue();

  const [selAll, setSelAll] = useState<number[]>([]);
  const [selAny, setSelAny] = useState<number[]>([]);
  const [selExclude, setSelExclude] = useState<number[]>([]);
  const [staleDays, setStaleDays] = useState<number | null>(null);
  const [noTag, setNoTag] = useState(false);
  const [preview, setPreview] = useState<PreviewResponse | null>(null);
  // 入队表单折叠：队列开始执行时自动收起；追加条目时强制展开
  const [formOpen, setFormOpen] = useState<string[]>(['form']);
  const [historyOpen, setHistoryOpen] = useState<string[]>([]);

  const queuesRef = useRef<QueueProgress[]>([]);
  useEffect(() => {
    const wasRunning = queuesRef.current.some((q) => q.status === 'running');
    const isRunning = queues.some((q) => q.status === 'running');
    if (isRunning && !wasRunning) setFormOpen([]);
    queuesRef.current = queues;
  }, [queues]);

  useEffect(() => {
    if (appendTarget !== null) setFormOpen(['form']);
  }, [appendTarget]);

  const collectSelector = (): Selector => ({
    tag_all: selAll,
    tag_any: selAny,
    tag_exclude: selExclude,
    stale_days: staleDays,
    no_tag: noTag,
  });

  const selectorIsEmpty = (s: Selector) =>
    s.tag_all.length === 0 && s.tag_any.length === 0 && s.tag_exclude.length === 0 && s.stale_days === null && !s.no_tag;

  const doPreview = async () => {
    const selector = collectSelector();
    if (selectorIsEmpty(selector)) {
      message.error('请先勾选标签条件，或填写天数');
      return;
    }
    try {
      setPreview(await apiPost<PreviewResponse>('/api/queues/preview', { selector }));
    } catch (e) {
      message.error(`预览失败: ${(e as Error).message}`);
    }
  };

  const doEnqueue = async () => {
    const selector = collectSelector();
    if (selectorIsEmpty(selector)) {
      message.error('请先勾选标签条件，或填写天数');
      return;
    }
    const ok = await enqueue({ selector });
    if (ok) setPreview(null);
  };

  const refreshQueues = () => queryClient.invalidateQueries({ queryKey: ['queues'] });

  const queueOp = async (url: string) => {
    try {
      await apiPost(url);
      refreshQueues();
    } catch (e) {
      message.error(`操作失败: ${(e as Error).message}`);
    }
  };

  const pauseAll = async () => {
    try {
      const n = await apiPost<number>('/api/queues/pause-all');
      message.success(`已暂停 ${n} 个队列`);
      refreshQueues();
    } catch (e) {
      message.error(`操作失败: ${(e as Error).message}`);
    }
  };

  const resumeAll = async () => {
    try {
      const n = await apiPost<number>('/api/queues/resume-all');
      message.success(`已恢复 ${n} 个队列`);
      refreshQueues();
    } catch (e) {
      message.error(`操作失败: ${(e as Error).message}`);
    }
  };

  const confirmDanger = (title: string, content: string, run: () => Promise<void>) => {
    modal.confirm({
      title,
      content,
      okText: '确认执行',
      okButtonProps: { danger: true },
      cancelText: '再想想',
      onOk: async () => {
        try {
          await run();
        } catch (e) {
          message.error(`操作失败: ${(e as Error).message}`);
        }
      },
    });
  };

  const renderActions = (q: QueueProgress) => {
    const ops: { label: string; run: () => void; danger?: boolean }[] = [];
    if (q.status === 'running') ops.push({ label: '暂停', run: () => queueOp(`/api/queues/${q.id}/pause`) });
    if (q.status === 'paused') ops.push({ label: '恢复', run: () => queueOp(`/api/queues/${q.id}/resume`) });
    if (ACTIVE_STATUSES.includes(q.status)) {
      ops.push({ label: '追加', run: () => enterAppend(q.id) });
      ops.push({
        label: '取消',
        danger: true,
        run: () =>
          confirmDanger(`取消队列 #${q.id}？`, '剩余条目将不再执行（已产生的记录保留）。', () =>
            queueOp(`/api/queues/${q.id}/cancel`),
          ),
      });
    }
    if (ops.length === 0) return <Typography.Text type="secondary">-</Typography.Text>;
    return (
      <Space split={<Typography.Text type="secondary">|</Typography.Text>} size={4}>
        {ops.map((op) => (
          <Button key={op.label} type="link" size="small" danger={op.danger} onClick={op.run} style={{ padding: 0 }}>
            {op.label}
          </Button>
        ))}
      </Space>
    );
  };

  const queueColumns = (isHistory: boolean) => [
    {
      title: 'ID',
      dataIndex: 'id',
      width: 70,
      render: (id: number, q: QueueProgress) => (
        <Space size={6}>
          {q.status === 'running' && <Badge status="processing" />}
          <span className="num">#{id}</span>
        </Space>
      ),
    },
    {
      title: '状态',
      dataIndex: 'status',
      width: 90,
      render: (s: QueueStatus) => <Tag color={QUEUE_STATUS_COLOR[s]}>{QUEUE_STATUS_TEXT[s] || s}</Tag>,
    },
    {
      title: '进度',
      key: 'progress',
      render: (_: unknown, q: QueueProgress) => {
        const finished = q.done + q.failed + q.skipped;
        const pct = q.total > 0 ? Math.round((finished / q.total) * 100) : 0;
        return ACTIVE_STATUSES.includes(q.status) ? (
          <Space size={8} style={{ minWidth: 140 }}>
            <Progress percent={pct} size="small" style={{ width: 100, margin: 0 }} showInfo={false} />
            <span className="num" style={{ fontSize: 12, color: 'inherit', opacity: 0.65 }}>
              {finished}/{q.total}
            </span>
          </Space>
        ) : (
          <span className="num" style={{ fontSize: 12 }}>
            {finished}/{q.total}
          </span>
        );
      },
    },
    { title: '间隔', dataIndex: 'interval_secs', width: 70, render: (v: number) => `${v}s` },
    {
      title: '创建时间',
      dataIndex: 'created_at',
      width: 160,
      render: (v: number) => <span className="num">{fmtTime(v)}</span>,
    },
    {
      title: '操作',
      key: 'actions',
      width: 170,
      render: (_: unknown, q: QueueProgress) =>
        isHistory ? (
          <Button
            type="link"
            size="small"
            danger
            style={{ padding: 0 }}
            onClick={() =>
              confirmDanger(`删除队列 #${q.id}？`, '队列及其条目记录将被永久删除。', async () => {
                await apiDelete(`/api/queues/${q.id}`);
                refreshQueues();
              })
            }
          >
            删除
          </Button>
        ) : (
          renderActions(q)
        ),
    },
  ];

  const runningQueue = queues.find((q) => q.status === 'running');
  const otherActive = queues.filter((q) => ACTIVE_STATUSES.includes(q.status) && q.status !== 'running');
  const history = queues.filter((q) => !ACTIVE_STATUSES.includes(q.status));
  const rqFinished = runningQueue ? runningQueue.done + runningQueue.failed + runningQueue.skipped : 0;
  const rqPct = runningQueue && runningQueue.total > 0 ? Math.round((rqFinished / runningQueue.total) * 100) : 0;

  // 行式选择器：左侧条件说明，右侧可点选标签（CheckableTag），紧凑不撑列
  const toggleTag = (list: number[], setter: (v: number[]) => void) => (id: number, checked: boolean) =>
    setter(checked ? [...list, id] : list.filter((x) => x !== id));

  const selectorGroup = (
    title: string,
    hint: string,
    value: number[],
    onChange: (v: number[]) => void,
  ) => (
    <div style={{ display: 'flex', gap: 16, flexWrap: 'wrap' }}>
      <div style={{ width: 150, flexShrink: 0 }}>
        <div style={{ fontSize: 13, fontWeight: 500 }}>{title}</div>
        <Typography.Text type="secondary" style={{ fontSize: 12 }}>
          {hint}
        </Typography.Text>
      </div>
      <div style={{ flex: 1, minWidth: 220 }}>
        {tags.length === 0 ? (
          <Typography.Text type="secondary">暂无标签</Typography.Text>
        ) : (
          tags.map((t) => (
            <Tag.CheckableTag
              key={t.id}
              checked={value.includes(t.id)}
              onChange={(c) => toggleTag(value, onChange)(t.id, c)}
              style={{ padding: '2px 10px', fontSize: 13, border: '1px solid', userSelect: 'none' }}
            >
              {t.name}
            </Tag.CheckableTag>
          ))
        )}
      </div>
    </div>
  );

  return (
    <Card
      title="抓取队列"
      extra={
        <Space>
          <Button onClick={pauseAll}>全部暂停</Button>
          <Button type="primary" onClick={resumeAll}>
            全部恢复
          </Button>
        </Space>
      }
    >
      <Space direction="vertical" size={12} style={{ width: '100%' }}>
        {appendTarget !== null && (
          <Alert
            type="info"
            showIcon
            message={
              <>
                正在向队列 #{appendTarget} 追加条目{' '}
                <Button type="link" size="small" style={{ padding: 0 }} onClick={exitAppend}>
                  退出追加
                </Button>
              </>
            }
          />
        )}

        {runningQueue && (
          <Card size="small" styles={{ body: { display: 'flex', flexDirection: 'column', gap: 8 } }}>
            <div style={{ display: 'flex', flexWrap: 'wrap', justifyContent: 'space-between', gap: 8 }}>
              <Space size={8}>
                <Badge status="processing" />
                <Typography.Text strong>正在执行 · 队列 #{runningQueue.id}</Typography.Text>
              </Space>
              <Typography.Text type="secondary" className="num" style={{ fontSize: 12 }}>
                间隔 {runningQueue.interval_secs}s · 剩余约{' '}
                {fmtEta((runningQueue.pending + runningQueue.running) * runningQueue.interval_secs)}
              </Typography.Text>
            </div>
            <Progress percent={rqPct} size="small" />
            <div
              style={{ display: 'flex', flexWrap: 'wrap', alignItems: 'flex-end', justifyContent: 'space-between', gap: 8 }}
            >
              <span className="num" style={{ fontSize: 28, fontWeight: 600, lineHeight: 1 }}>
                {rqFinished}
                <span style={{ fontSize: 14, fontWeight: 500, opacity: 0.55 }}>/{runningQueue.total}</span>
              </span>
              <Typography.Text type="secondary" className="num" style={{ fontSize: 12 }}>
                待 {runningQueue.pending} · 成 {runningQueue.done} · 败 {runningQueue.failed} · 跳{' '}
                {runningQueue.skipped}
              </Typography.Text>
              {renderActions(runningQueue)}
            </div>
          </Card>
        )}

        <Collapse
          activeKey={formOpen}
          onChange={(keys) => setFormOpen(keys as string[])}
          items={[
            {
              key: 'form',
              label: appendTarget !== null ? `向队列 #${appendTarget} 追加条目` : '新建队列',
              children: (
                <Space direction="vertical" size={16} style={{ width: '100%' }}>
                  <Typography.Text type="secondary" style={{ fontSize: 13 }}>
                    按标签挑选要抓取的商品，下方三组条件之间是「并且」关系：全部满足才会被抓取。
                  </Typography.Text>
                  {selectorGroup('同时具备这些标签', '商品必须带齐全部勾选的标签', selAll, setSelAll)}
                  {selectorGroup('具备其中任一标签', '商品带有任一勾选的标签即可', selAny, setSelAny)}
                  {selectorGroup('排除这些标签', '带有任一勾选标签的商品不抓取', selExclude, setSelExclude)}
                  <Checkbox checked={noTag} onChange={(e) => setNoTag(e.target.checked)}>
                    只抓取「无标签」商品
                  </Checkbox>
                  <Divider style={{ margin: 0 }} />
                  <Space wrap size={24} align="end">
                    <div>
                      <div style={{ fontSize: 13, marginBottom: 4 }}>只抓超过 N 天未抓取的商品（可不填）</div>
                      <InputNumber
                        min={1}
                        placeholder="如 7"
                        addonAfter="天"
                        value={staleDays}
                        onChange={(v) => setStaleDays(v)}
                        style={{ width: 140 }}
                      />
                    </div>
                    <div>
                      <div style={{ fontSize: 13, marginBottom: 4 }}>每件商品的抓取间隔</div>
                      <InputNumber
                        min={1}
                        addonAfter="秒"
                        value={intervalSecs}
                        onChange={(v) => setIntervalSecs(v || 3)}
                        style={{ width: 140 }}
                      />
                    </div>
                    <Space>
                      <Button onClick={doPreview}>预览匹配商品</Button>
                      <Button type="primary" onClick={doEnqueue}>
                        {appendTarget !== null ? `追加到队列 #${appendTarget}` : '创建队列'}
                      </Button>
                    </Space>
                  </Space>
                  {preview && (
                    <Alert
                      type="info"
                      message={
                        <>
                          将新增 <b>{preview.to_add.length}</b> 个
                          {preview.to_add.length > 0 && '：' + preview.to_add.map((p) => p.name).join('、')}
                          {preview.skipped.length > 0 && (
                            <>
                              <br />
                              已在队列，跳过 <b>{preview.skipped.length}</b> 个：
                              {preview.skipped.map((p) => p.name).join('、')}
                            </>
                          )}
                        </>
                      }
                    />
                  )}
                </Space>
              ),
            },
          ]}
        />

        {!runningQueue && otherActive.length === 0 && !queuesLoading ? (
          <Empty
            description={
              <>
                <div style={{ fontWeight: 500 }}>暂无活跃队列</div>
                <div style={{ fontSize: 13 }}>
                  展开上方「新建队列」按标签条件创建，或在「商品管理」里勾选商品入队。
                </div>
              </>
            }
            style={{ padding: '32px 0' }}
          />
        ) : (
          otherActive.length > 0 && (
            <Table
              rowKey="id"
              size="small"
              columns={queueColumns(false)}
              dataSource={otherActive}
              pagination={false}
            />
          )
        )}

        {history.length > 0 && (
          <Collapse
            activeKey={historyOpen}
            onChange={(keys) => setHistoryOpen(keys as string[])}
            ghost
            items={[
              {
                key: 'history',
                label: `历史队列（${history.length}）`,
                children: (
                  <Table
                    rowKey="id"
                    size="small"
                    columns={queueColumns(true)}
                    dataSource={history}
                    pagination={false}
                  />
                ),
              },
            ]}
          />
        )}
      </Space>
    </Card>
  );
}
