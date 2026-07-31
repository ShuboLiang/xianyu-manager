import { useState } from 'react';
import { toast } from 'sonner';
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
import { Input } from '@/components/ui/input';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { apiDelete, apiGet, apiPost, apiPut, fmtTime } from '@/lib/api';
import { Pager } from '@/sections/Pager';
import type { AiProvider, AiStatus, AiToolCall, PageResponse, TestConnectionResponse } from '@/types/api';

// 供应商模板：后端统一使用 OpenAI 兼容协议。
// base_url / model 参考 rig-core 0.41 各 provider 常量与官方 OpenAI 兼容端点文档（2026-07）。
const AI_PRESETS: Record<string, { name: string; base_url: string; model: string }> = {
  // 国际主流
  openai: { name: 'OpenAI', base_url: 'https://api.openai.com/v1', model: 'gpt-4.1-mini' },
  xai: { name: 'xAI (Grok)', base_url: 'https://api.x.ai/v1', model: 'grok-4.3' },
  groq: { name: 'Groq', base_url: 'https://api.groq.com/openai/v1', model: 'llama-3.3-70b-versatile' },
  mistral: { name: 'Mistral AI', base_url: 'https://api.mistral.ai/v1', model: 'mistral-large-3' },
  together: { name: 'Together AI', base_url: 'https://api.together.xyz/v1', model: 'meta-llama/Llama-4-Scout-17B-16E-Instruct' },
  openrouter: { name: 'OpenRouter', base_url: 'https://openrouter.ai/api/v1', model: 'openai/gpt-4.1-mini' },
  hyperbolic: { name: 'Hyperbolic', base_url: 'https://api.hyperbolic.xyz/v1', model: 'meta-llama/Llama-4-Scout-17B-16E-Instruct' },
  perplexity: { name: 'Perplexity', base_url: 'https://api.perplexity.ai', model: 'sonar' },
  gemini: { name: 'Gemini (OpenAI 兼容)', base_url: 'https://generativelanguage.googleapis.com/v1beta/openai', model: 'gemini-2.5-flash' },
  // 国内主流
  deepseek: { name: 'DeepSeek', base_url: 'https://api.deepseek.com/v1', model: 'deepseek-v4-flash' },
  moonshot: { name: 'Kimi (Moonshot 国内)', base_url: 'https://api.moonshot.cn/v1', model: 'kimi-k3' },
  moonshot_global: { name: 'Kimi (Moonshot 国际)', base_url: 'https://api.moonshot.ai/v1', model: 'kimi-k3' },
  qwen: { name: '通义千问', base_url: 'https://dashscope.aliyuncs.com/compatible-mode/v1', model: 'qwen3.5-plus' },
  zhipu: { name: '智谱 AI', base_url: 'https://open.bigmodel.cn/api/paas/v4', model: 'glm-4.5' },
  siliconflow: { name: 'SiliconFlow', base_url: 'https://api.siliconflow.cn/v1', model: 'deepseek-ai/DeepSeek-V4-Flash' },
  minimax: { name: 'MiniMax', base_url: 'https://api.minimaxi.com/v1', model: 'MiniMax-M3' },
  // 本地
  ollama: { name: 'Ollama (本地)', base_url: 'http://localhost:11434/v1', model: 'llama4:scout' },
  // 自定义
  custom: { name: '自定义 OpenAI 兼容', base_url: '', model: '' },
};

interface Props {
  providers: AiProvider[];
  status: AiStatus | null;
  toolCalls: PageResponse<AiToolCall>; // 服务端分页的调用记录
  onCallsPageChange: (page: number, pageSize: number) => void;
  onRefresh: () => void;
}

export function AiCard({ providers, status, toolCalls, onCallsPageChange, onRefresh }: Props) {
  const [editingId, setEditingId] = useState<number | null>(null);
  const [name, setName] = useState('');
  const [baseUrl, setBaseUrl] = useState('https://api.openai.com/v1');
  const [apiKey, setApiKey] = useState('');
  const [model, setModel] = useState('gpt-4o-mini');
  const [timeoutSecs, setTimeoutSecs] = useState(60);
  const [pendingDeleteId, setPendingDeleteId] = useState<number | null>(null);

  const applyPreset = (key: string) => {
    const preset = AI_PRESETS[key];
    if (!preset) return;
    if (preset.name) setName(preset.name);
    setBaseUrl(preset.base_url);
    setModel(preset.model);
  };

  const resetForm = () => {
    setEditingId(null);
    setName('');
    setBaseUrl('https://api.openai.com/v1');
    setApiKey('');
    setModel('gpt-4o-mini');
    setTimeoutSecs(60);
  };

  const submit = async () => {
    if (!name.trim() || !baseUrl.trim() || !model.trim()) {
      toast.error('名称、base_url、模型名必填');
      return;
    }
    const payload = {
      name: name.trim(),
      base_url: baseUrl.trim(),
      api_key: apiKey.trim() || null,
      model: model.trim(),
      timeout_secs: timeoutSecs || 60,
    };
    const isEdit = editingId !== null;
    try {
      if (isEdit) {
        await apiPut(`/api/ai/providers/${editingId}`, payload);
      } else {
        await apiPost('/api/ai/providers', payload);
      }
      resetForm();
      onRefresh();
    } catch (e) {
      toast.error(`${isEdit ? '更新' : '添加'}失败: ${(e as Error).message}`);
    }
  };

  const startEdit = async (id: number) => {
    try {
      const p = await apiGet<AiProvider>(`/api/ai/providers/${id}`);
      setEditingId(p.id);
      setName(p.name);
      setBaseUrl(p.base_url);
      setApiKey(''); // 密钥不回填，留空表示不修改
      setModel(p.model);
      setTimeoutSecs(p.timeout_secs);
    } catch (e) {
      toast.error(`加载配置失败: ${(e as Error).message}`);
    }
  };

  const test = async (id: number) => {
    try {
      const data = await apiPost<TestConnectionResponse>(`/api/ai/providers/${id}/test`);
      toast.success(`连通正常，耗时 ${data.latency_ms} ms\n模型回复：${data.reply}`);
    } catch (e) {
      toast.error(`测试失败: ${(e as Error).message}`);
    }
  };

  const setDefault = async (id: number) => {
    try {
      await apiPost(`/api/ai/providers/${id}/default`);
      onRefresh();
    } catch (e) {
      toast.error(`设置默认失败: ${(e as Error).message}`);
    }
  };

  const confirmDelete = async () => {
    if (pendingDeleteId === null) return;
    try {
      await apiDelete(`/api/ai/providers/${pendingDeleteId}`);
      if (editingId === pendingDeleteId) resetForm();
      setPendingDeleteId(null);
      onRefresh();
    } catch (e) {
      toast.error(`删除失败: ${(e as Error).message}`);
    }
  };

  return (
    <Card>
      <Tabs defaultValue="providers">
        <CardHeader className="flex flex-row items-center justify-between space-y-0">
          <CardTitle>AI</CardTitle>
          <TabsList>
            <TabsTrigger value="providers">接口配置</TabsTrigger>
            <TabsTrigger value="calls">调用记录（{toolCalls.total}）</TabsTrigger>
          </TabsList>
        </CardHeader>
        <TabsContent value="providers">
          <CardContent className="space-y-4">
            {status && !status.configured && (
              <div className="rounded-md border border-amber-400/50 bg-amber-50 px-3 py-2 text-sm dark:bg-amber-950/30">
                尚未配置 AI 接口（请在下方添加或设置 AI_API_KEY 环境变量）
              </div>
            )}
            <div className="flex flex-wrap gap-2">
              <Select onValueChange={applyPreset}>
                <SelectTrigger className="w-56">
                  <SelectValue placeholder="-- 选择供应商模板 --" />
                </SelectTrigger>
                <SelectContent>
                  {Object.entries(AI_PRESETS).map(([key, preset]) => (
                    <SelectItem key={key} value={key}>
                      {preset.name || key}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
              <Input className="w-48" placeholder="配置名称，如：DeepSeek" value={name} onChange={(e) => setName(e.target.value)} />
              <Input
                className="w-24"
                type="number"
                min={1}
                title="超时（秒）"
                value={timeoutSecs}
                onChange={(e) => setTimeoutSecs(Number(e.target.value) || 60)}
              />
            </div>
            <div className="flex flex-wrap gap-2">
              <Input className="w-72" placeholder="https://api.openai.com/v1" value={baseUrl} onChange={(e) => setBaseUrl(e.target.value)} />
              <Input
                className="w-56"
                type="password"
                autoComplete="off"
                placeholder={editingId !== null ? 'API Key（留空不修改）' : 'API Key'}
                value={apiKey}
                onChange={(e) => setApiKey(e.target.value)}
              />
              <Input className="w-56" placeholder="模型名，如：gpt-4o-mini" value={model} onChange={(e) => setModel(e.target.value)} />
            </div>
            <p className="text-sm text-muted-foreground">选择模板可自动填入 rig 推荐 base_url 与模型；也可手动修改。</p>
            <div className="flex gap-2">
              <Button onClick={submit}>{editingId !== null ? '保存修改' : '添加配置'}</Button>
              {editingId !== null && (
                <Button variant="secondary" onClick={resetForm}>
                  取消编辑
                </Button>
              )}
            </div>
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>名称</TableHead>
                  <TableHead>Base URL</TableHead>
                  <TableHead>模型</TableHead>
                  <TableHead>密钥</TableHead>
                  <TableHead>默认</TableHead>
                  <TableHead>操作</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {providers.length === 0 ? (
                  <TableRow>
                    <TableCell colSpan={6} className="text-center text-muted-foreground">
                      暂无 AI 配置
                    </TableCell>
                  </TableRow>
                ) : (
                  providers.map((p) => (
                    <TableRow key={p.id}>
                      <TableCell>{p.name}</TableCell>
                      <TableCell className="max-w-64 truncate" title={p.base_url}>
                        {p.base_url}
                      </TableCell>
                      <TableCell>{p.model}</TableCell>
                      <TableCell>{p.api_key || '-'}</TableCell>
                      <TableCell>{p.is_default ? <Badge>默认</Badge> : '-'}</TableCell>
                      <TableCell className="space-x-3 whitespace-nowrap">
                        <button className="text-primary hover:underline" onClick={() => startEdit(p.id)}>
                          编辑
                        </button>
                        <button className="text-primary hover:underline" onClick={() => test(p.id)}>
                          测试
                        </button>
                        {!p.is_default && (
                          <button className="text-primary hover:underline" onClick={() => setDefault(p.id)}>
                            设为默认
                          </button>
                        )}
                        <button className="text-destructive hover:underline" onClick={() => setPendingDeleteId(p.id)}>
                          删除
                        </button>
                      </TableCell>
                    </TableRow>
                  ))
                )}
              </TableBody>
            </Table>
          </CardContent>
        </TabsContent>
        <TabsContent value="calls">
          <CardContent className="space-y-3">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>时间</TableHead>
                  <TableHead>工具</TableHead>
                  <TableHead>参数</TableHead>
                  <TableHead>结果/错误</TableHead>
                  <TableHead>耗时</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {toolCalls.items.length === 0 ? (
                  <TableRow>
                    <TableCell colSpan={5} className="text-center text-muted-foreground">
                      暂无工具调用记录
                    </TableCell>
                  </TableRow>
                ) : (
                  toolCalls.items.map((c) => (
                    <TableRow key={c.id}>
                      <TableCell className="whitespace-nowrap">{fmtTime(c.created_at)}</TableCell>
                      <TableCell>{c.tool_name}</TableCell>
                      <TableCell className="max-w-64 truncate" title={c.arguments}>
                        {c.arguments.slice(0, 60)}
                        {c.arguments.length > 60 ? '...' : ''}
                      </TableCell>
                      <TableCell className="max-w-48 truncate" title={c.result || c.error || ''}>
                        {c.result ? '成功' : c.error ? '失败: ' + c.error.slice(0, 40) : '-'}
                      </TableCell>
                      <TableCell>{c.duration_ms} ms</TableCell>
                    </TableRow>
                  ))
                )}
              </TableBody>
            </Table>
            <Pager
              page={toolCalls.page}
              pageSize={toolCalls.page_size}
              total={toolCalls.total}
              onChange={onCallsPageChange}
            />
          </CardContent>
        </TabsContent>
      </Tabs>

      <AlertDialog open={pendingDeleteId !== null} onOpenChange={(o) => !o && setPendingDeleteId(null)}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>
              删除 AI 配置「{providers.find((p) => p.id === pendingDeleteId)?.name}」？
            </AlertDialogTitle>
            <AlertDialogDescription>删除后不可恢复；若该配置为默认，需重新指定默认配置。</AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>取消</AlertDialogCancel>
            <AlertDialogAction
              className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
              onClick={confirmDelete}
            >
              确认删除
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </Card>
  );
}
