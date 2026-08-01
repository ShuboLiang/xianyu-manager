import { useEffect, useRef, useState } from 'react';
import { toast } from 'sonner';
import { ChevronDown, ListTodo } from 'lucide-react';
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Checkbox } from '@/components/ui/checkbox';
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/components/ui/collapsible';
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from '@/components/ui/empty';
import { Input } from '@/components/ui/input';
import { Progress } from '@/components/ui/progress';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table';
import { apiDelete, apiPost, fmtTime } from '@/lib/api';
import { SkeletonRows } from '@/sections/SkeletonRows';
import type { PreviewResponse, QueueProgress, QueueStatus, Selector, Tag } from '@/types/api';

export const QUEUE_STATUS_TEXT: Record<QueueStatus, string> = {
  waiting: '排队中',
  running: '执行中',
  paused: '已暂停',
  done: '已完成',
  cancelled: '已取消',
};

// 队列状态色规范：执行中=琥珀（primary），排队中=蓝灰，已暂停=橙，已完成=绿，已取消=灰
const QUEUE_STATUS_BADGE_CLASS: Partial<Record<QueueStatus, string>> = {
  waiting: 'border-slate-400/60 text-slate-600 dark:text-slate-300',
  paused:
    'border-orange-400/60 bg-orange-50 text-orange-700 dark:bg-orange-950/40 dark:text-orange-300',
  done: 'border-green-500/50 bg-green-50 text-green-700 dark:bg-green-950/40 dark:text-green-300',
  cancelled: 'text-muted-foreground',
};

const ACTIVE_STATUSES: QueueStatus[] = ['waiting', 'running', 'paused'];

/** 预计剩余时间：剩余条目 × 间隔秒数 → 人性化时长 */
function fmtEta(seconds: number): string {
  if (seconds < 60) return `${seconds} 秒`;
  if (seconds < 3600) return `${Math.floor(seconds / 60)} 分 ${seconds % 60} 秒`;
  return `${Math.floor(seconds / 3600)} 小时 ${Math.round((seconds % 3600) / 60)} 分`;
}

interface Props {
  tags: Tag[];
  queues: QueueProgress[];
  loading?: boolean;
  appendTarget: number | null;
  intervalSecs: number;
  onIntervalChange: (v: number) => void;
  onExitAppend: () => void;
  onEnterAppend: (id: number) => void;
  onRefresh: () => void;
  onEnqueueSelector: (selector: Selector) => Promise<boolean>;
}

function SelectorGroup({
  title,
  hint,
  tags,
  checked,
  onToggle,
}: {
  title: string;
  hint: string;
  tags: Tag[];
  checked: Set<number>;
  onToggle: (id: number, on: boolean) => void;
}) {
  return (
    <div>
      <h3 className="text-sm font-medium">{title}</h3>
      <p className="mb-2 mt-0.5 text-xs text-muted-foreground">{hint}</p>
      <div className="flex flex-wrap gap-x-4 gap-y-2">
        {tags.length === 0 ? (
          <span className="text-sm text-muted-foreground">暂无标签</span>
        ) : (
          tags.map((t) => (
            <label key={t.id} className="flex items-center gap-1.5 text-sm">
              <Checkbox
                checked={checked.has(t.id)}
                onCheckedChange={(v) => onToggle(t.id, v === true)}
              />
              {t.name}
            </label>
          ))
        )}
      </div>
    </div>
  );
}

export function QueuesCard({
  tags,
  queues,
  loading,
  appendTarget,
  intervalSecs,
  onIntervalChange,
  onExitAppend,
  onEnterAppend,
  onRefresh,
  onEnqueueSelector,
}: Props) {
  const [selAll, setSelAll] = useState<Set<number>>(new Set());
  const [selAny, setSelAny] = useState<Set<number>>(new Set());
  const [selExclude, setSelExclude] = useState<Set<number>>(new Set());
  const [staleDays, setStaleDays] = useState('');
  const [preview, setPreview] = useState<PreviewResponse | null>(null);
  const [historyOpen, setHistoryOpen] = useState(false);
  // 入队表单折叠：队列开始执行时自动收起，给「正在执行」面板让位；追加条目时强制展开
  const [formOpen, setFormOpen] = useState(true);
  // 通用危险操作确认（取消队列 / 删除历史队列）
  const [confirm, setConfirm] = useState<{ title: string; desc: string; run: () => Promise<void> } | null>(null);

  const queuesRef = useRef<QueueProgress[]>([]);
  useEffect(() => {
    const wasRunning = queuesRef.current.some((q) => q.status === 'running');
    const isRunning = queues.some((q) => q.status === 'running');
    if (isRunning && !wasRunning) setFormOpen(false);
    queuesRef.current = queues;
  }, [queues]);

  useEffect(() => {
    if (appendTarget !== null) setFormOpen(true);
  }, [appendTarget]);

  const toggleIn = (set: Set<number>, setter: (s: Set<number>) => void) => (id: number, on: boolean) => {
    const next = new Set(set);
    if (on) next.add(id);
    else next.delete(id);
    setter(next);
  };

  const collectSelector = (): Selector => ({
    tag_all: [...selAll],
    tag_any: [...selAny],
    tag_exclude: [...selExclude],
    stale_days: staleDays ? Number(staleDays) : null,
  });

  const selectorIsEmpty = (s: Selector) =>
    s.tag_all.length === 0 && s.tag_any.length === 0 && s.tag_exclude.length === 0 && s.stale_days === null;

  const doPreview = async () => {
    const selector = collectSelector();
    if (selectorIsEmpty(selector)) {
      toast.error('请先勾选标签条件，或填写天数');
      return;
    }
    try {
      const data = await apiPost<PreviewResponse>('/api/queues/preview', { selector });
      setPreview(data);
    } catch (e) {
      toast.error(`预览失败: ${(e as Error).message}`);
    }
  };

  const doEnqueue = async () => {
    const selector = collectSelector();
    if (selectorIsEmpty(selector)) {
      toast.error('请先勾选标签条件，或填写天数');
      return;
    }
    const ok = await onEnqueueSelector(selector);
    if (ok) setPreview(null);
  };

  const queueOp = async (url: string) => {
    try {
      await apiPost(url);
      onRefresh();
    } catch (e) {
      toast.error(`操作失败: ${(e as Error).message}`);
    }
  };

  const pauseAll = async () => {
    try {
      const n = await apiPost<number>('/api/queues/pause-all');
      toast.success(`已暂停 ${n} 个队列`);
      onRefresh();
    } catch (e) {
      toast.error(`操作失败: ${(e as Error).message}`);
    }
  };

  const resumeAll = async () => {
    try {
      const n = await apiPost<number>('/api/queues/resume-all');
      toast.success(`已恢复 ${n} 个队列`);
      onRefresh();
    } catch (e) {
      toast.error(`操作失败: ${(e as Error).message}`);
    }
  };

  const deleteQueue = (id: number) => {
    setConfirm({
      title: `删除队列 #${id}？`,
      desc: '队列及其条目记录将被永久删除。',
      run: async () => {
        await apiDelete(`/api/queues/${id}`);
        onRefresh();
      },
    });
  };

  const runConfirm = async () => {
    if (!confirm) return;
    try {
      await confirm.run();
      setConfirm(null);
    } catch (e) {
      toast.error(`操作失败: ${(e as Error).message}`);
    }
  };

  const renderActions = (q: QueueProgress) => {
    const ops: { label: string; run: () => void; danger?: boolean }[] = [];
    if (q.status === 'running') ops.push({ label: '暂停', run: () => queueOp(`/api/queues/${q.id}/pause`) });
    if (q.status === 'paused') ops.push({ label: '恢复', run: () => queueOp(`/api/queues/${q.id}/resume`) });
    if (ACTIVE_STATUSES.includes(q.status)) {
      ops.push({ label: '追加', run: () => onEnterAppend(q.id) });
      ops.push({
        label: '取消',
        danger: true,
        run: () =>
          setConfirm({
            title: `取消队列 #${q.id}？`,
            desc: '剩余条目将不再执行（已产生的记录保留）。',
            run: () => queueOp(`/api/queues/${q.id}/cancel`),
          }),
      });
    }
    if (ops.length === 0) return <span className="text-muted-foreground">-</span>;
    return (
      <span className="space-x-3">
        {ops.map((op) => (
          <button
            key={op.label}
            className={op.danger ? 'text-destructive hover:underline' : 'text-primary hover:underline'}
            onClick={op.run}
          >
            {op.label}
          </button>
        ))}
      </span>
    );
  };

  const renderRow = (q: QueueProgress, actions: React.ReactNode) => {
    const finished = q.done + q.failed + q.skipped;
    const pct = q.total > 0 ? Math.round((finished / q.total) * 100) : 0;
    const isActive = ACTIVE_STATUSES.includes(q.status);
    return (
      <TableRow key={q.id}>
        <TableCell>
          <span className="flex items-center gap-2">
            {q.status === 'running' && (
              <span className="h-2 w-2 animate-pulse rounded-full bg-primary" title="正在执行" />
            )}
            #{q.id}
          </span>
        </TableCell>
        <TableCell>
          <Badge
            variant={q.status === 'running' ? 'default' : 'outline'}
            className={QUEUE_STATUS_BADGE_CLASS[q.status]}
          >
            {QUEUE_STATUS_TEXT[q.status] || q.status}
          </Badge>
        </TableCell>
        <TableCell title={`待 ${q.pending} / 成 ${q.done} / 败 ${q.failed} / 跳 ${q.skipped}`}>
          {isActive ? (
            <div className="flex min-w-32 items-center gap-2">
              <Progress value={pct} className="h-1.5 flex-1" />
              <span className="whitespace-nowrap font-data text-xs text-muted-foreground">
                {finished}/{q.total}
              </span>
            </div>
          ) : (
            <span className="font-data text-xs">
              {finished}/{q.total}
            </span>
          )}
        </TableCell>
        <TableCell>{q.interval_secs}s</TableCell>
        <TableCell>{fmtTime(q.created_at)}</TableCell>
        <TableCell>{actions}</TableCell>
      </TableRow>
    );
  };

  const active = queues.filter((q) => ACTIVE_STATUSES.includes(q.status));
  const history = queues.filter((q) => !ACTIVE_STATUSES.includes(q.status));
  // 运行中队列升格为「正在执行」控制台面板，其余活跃队列留在表格
  const runningQueue = queues.find((q) => q.status === 'running');
  const otherActive = active.filter((q) => q.status !== 'running');
  const rqFinished = runningQueue ? runningQueue.done + runningQueue.failed + runningQueue.skipped : 0;
  const rqPct =
    runningQueue && runningQueue.total > 0 ? Math.round((rqFinished / runningQueue.total) * 100) : 0;

  const queueTable = (list: QueueProgress[], isHistory: boolean) => (
    <Table>
      <TableHeader>
        <TableRow>
          <TableHead>ID</TableHead>
          <TableHead>状态</TableHead>
          <TableHead>进度</TableHead>
          <TableHead>间隔</TableHead>
          <TableHead>创建时间</TableHead>
          <TableHead>操作</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        {list.length === 0 ? (
          <TableRow>
            <TableCell colSpan={6} className="text-center text-muted-foreground">
              暂无历史队列
            </TableCell>
          </TableRow>
        ) : (
          list.map((q) =>
            renderRow(
              q,
              isHistory ? (
                <button className="text-destructive hover:underline" onClick={() => deleteQueue(q.id)}>
                  删除
                </button>
              ) : (
                renderActions(q)
              ),
            ),
          )
        )}
      </TableBody>
    </Table>
  );

  return (
    <Card>
      <CardHeader className="flex flex-row flex-wrap items-center justify-between gap-2 space-y-0">
        <CardTitle>抓取队列</CardTitle>
        <div className="space-x-2">
          <Button variant="secondary" onClick={pauseAll}>
            全部暂停
          </Button>
          <Button onClick={resumeAll}>全部恢复</Button>
        </div>
      </CardHeader>
      <CardContent className="space-y-4">
        {appendTarget !== null && (
          <div className="rounded-md border border-primary/40 bg-primary/5 px-3 py-2 text-sm">
            正在向队列 #{appendTarget} 追加条目{' '}
            <button className="text-primary hover:underline" onClick={onExitAppend}>
              退出追加
            </button>
          </div>
        )}
        {runningQueue && (
          <div className="space-y-3 rounded-lg border bg-muted/40 p-4">
            <div className="flex flex-wrap items-center justify-between gap-2">
              <span className="flex items-center gap-2 text-sm font-medium">
                <span className="h-2 w-2 animate-pulse rounded-full bg-primary" />
                正在执行 · 队列 #{runningQueue.id}
              </span>
              <span className="font-data text-xs text-muted-foreground">
                间隔 {runningQueue.interval_secs}s · 剩余约{' '}
                {fmtEta((runningQueue.pending + runningQueue.running) * runningQueue.interval_secs)}
              </span>
            </div>
            <Progress value={rqPct} className="h-2.5" />
            <div className="flex flex-wrap items-end justify-between gap-2">
              <span className="font-data text-3xl font-semibold leading-none">
                {rqFinished}
                <span className="text-base font-medium text-muted-foreground">
                  /{runningQueue.total}
                </span>
              </span>
              <span className="font-data text-xs text-muted-foreground">
                待 {runningQueue.pending} · 成 {runningQueue.done} · 败 {runningQueue.failed} · 跳{' '}
                {runningQueue.skipped}
              </span>
              <div className="text-sm">{renderActions(runningQueue)}</div>
            </div>
          </div>
        )}
        <Collapsible open={formOpen} onOpenChange={setFormOpen}>
          <CollapsibleTrigger className="flex w-full items-center justify-between rounded-md border px-3 py-2 text-sm font-medium hover:bg-muted/50">
            {appendTarget !== null ? `向队列 #${appendTarget} 追加条目` : '新建队列'}
            <ChevronDown
              className={`h-4 w-4 text-muted-foreground transition-transform ${formOpen ? 'rotate-180' : ''}`}
            />
          </CollapsibleTrigger>
          <CollapsibleContent className="space-y-4 pt-4">
            <p className="text-sm text-muted-foreground">
              按标签挑选要抓取的商品，下方三组条件之间是「并且」关系：全部满足才会被抓取。
            </p>
            <div className="grid gap-4 md:grid-cols-3">
              <SelectorGroup
                title="同时具备这些标签"
                hint="商品必须带齐全部勾选的标签"
                tags={tags}
                checked={selAll}
                onToggle={toggleIn(selAll, setSelAll)}
              />
              <SelectorGroup
                title="具备其中任一标签"
                hint="商品带有任一勾选的标签即可"
                tags={tags}
                checked={selAny}
                onToggle={toggleIn(selAny, setSelAny)}
              />
              <SelectorGroup
                title="排除这些标签"
                hint="带有任一勾选标签的商品不抓取"
                tags={tags}
                checked={selExclude}
                onToggle={toggleIn(selExclude, setSelExclude)}
              />
            </div>
            <div className="flex flex-wrap items-end gap-x-6 gap-y-3">
              <div>
                <label className="mb-1 block text-sm font-medium" htmlFor="stale-days">
                  只抓超过 N 天没爬过的商品（可不填）
                </label>
                <div className="flex items-center gap-1.5">
                  <Input
                    id="stale-days"
                    className="w-24"
                    type="number"
                    min={1}
                    placeholder="如 7"
                    value={staleDays}
                    onChange={(e) => setStaleDays(e.target.value)}
                  />
                  <span className="text-sm text-muted-foreground">天</span>
                </div>
              </div>
              <div>
                <label className="mb-1 block text-sm font-medium" htmlFor="interval-secs">
                  每件商品的抓取间隔
                </label>
                <div className="flex items-center gap-1.5">
                  <Input
                    id="interval-secs"
                    className="w-24"
                    type="number"
                    min={1}
                    value={intervalSecs}
                    onChange={(e) => onIntervalChange(Number(e.target.value) || 3)}
                  />
                  <span className="text-sm text-muted-foreground">秒</span>
                </div>
              </div>
              <div className="flex gap-2">
                <Button variant="secondary" onClick={doPreview}>
                  预览匹配商品
                </Button>
                <Button onClick={doEnqueue}>
                  {appendTarget !== null ? `追加到队列 #${appendTarget}` : '创建队列'}
                </Button>
              </div>
            </div>
            {preview && (
              <div className="rounded-md border bg-muted/50 px-3 py-2 text-sm">
                将新增 <b>{preview.to_add.length}</b> 个
                {preview.to_add.length > 0 && '：' + preview.to_add.map((p) => p.name).join('、')}
                {preview.skipped.length > 0 && (
                  <>
                    <br />
                    已在队列，跳过 <b>{preview.skipped.length}</b> 个：
                    {preview.skipped.map((p) => p.name).join('、')}
                  </>
                )}
              </div>
            )}
          </CollapsibleContent>
        </Collapsible>
        {loading ? (
          <Table>
            <TableBody>
              <SkeletonRows cols={6} />
            </TableBody>
          </Table>
        ) : !runningQueue && active.length === 0 ? (
          <Empty className="border">
            <EmptyHeader>
              <EmptyMedia variant="icon">
                <ListTodo />
              </EmptyMedia>
              <EmptyTitle>暂无活跃队列</EmptyTitle>
              <EmptyDescription>
                展开上方「新建队列」按标签条件创建，或在「待爬取商品管理」里勾选商品入队。
              </EmptyDescription>
            </EmptyHeader>
          </Empty>
        ) : (
          otherActive.length > 0 && queueTable(otherActive, false)
        )}
        {history.length > 0 && (
          <div>
            <button className="text-sm text-primary hover:underline" onClick={() => setHistoryOpen(!historyOpen)}>
              {historyOpen ? '▾' : '▸'} 历史队列（{history.length}）
            </button>
            {historyOpen && <div className="mt-2">{queueTable(history, true)}</div>}
          </div>
        )}
      </CardContent>

      <AlertDialog open={confirm !== null} onOpenChange={(o) => !o && setConfirm(null)}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{confirm?.title}</AlertDialogTitle>
            <AlertDialogDescription>{confirm?.desc}</AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>再想想</AlertDialogCancel>
            <AlertDialogAction
              className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
              onClick={runConfirm}
            >
              确认执行
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </Card>
  );
}
