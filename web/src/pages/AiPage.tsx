import { useEffect, useState } from 'react';
import {
  App as AntApp,
  Alert,
  Button,
  Card,
  Col,
  Form,
  Input,
  InputNumber,
  Modal,
  Radio,
  Row,
  Segmented,
  Select,
  Space,
  Switch,
  Table,
  Tabs,
  Tag,
  Tooltip,
  Typography,
} from 'antd';
import { keepPreviousData, useQuery, useQueryClient } from '@tanstack/react-query';
import { PageHeader } from '@/components/PageHeader';
import { apiDelete, apiGet, apiPost, apiPut, fmtTime } from '@/lib/api';
import type {
  AiProvider,
  AiStatus,
  AiToolCall,
  AiToolCallPurgePreviewResponse,
  AiToolCallPurgeRequest,
  AiToolCallPurgeResponse,
  CrawlModeResponse,
  CrawlPrompt,
  PageResponse,
  TestConnectionResponse,
} from '@/types/api';

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

interface ProviderFormValues {
  name: string;
  base_url: string;
  api_key?: string;
  model: string;
  timeout_secs: number;
}

// ---------- 思考模式控制 ----------
type ThinkingEffort = 'low' | 'high' | 'max';
interface ThinkingState {
  on: boolean;
  effort: ThinkingEffort;
}

const EFFORT_LABEL: Record<ThinkingEffort, string> = { low: '低', high: '高', max: '最高' };
const DEFAULT_THINKING: ThinkingState = { on: true, effort: 'high' };

const isQwenUrl = (baseUrl: string) => /qwen|dashscope/i.test(baseUrl);

/** 已保存参数 → 思考模式状态；null = 供应商默认（开 + 高）；无法识别的自定义参数返回 null */
function parseThinkingParams(raw: string | null): ThinkingState | null {
  if (!raw) return DEFAULT_THINKING;
  try {
    const v = JSON.parse(raw);
    if (v?.thinking?.type === 'disabled' || v?.enable_thinking === false) {
      return { on: false, effort: 'high' };
    }
    if (v?.thinking?.type === 'enabled' || v?.enable_thinking === true || v?.reasoning_effort) {
      const e = v?.reasoning_effort;
      return { on: true, effort: e === 'low' || e === 'max' ? e : 'high' };
    }
    return null;
  } catch {
    return null;
  }
}

/** 表单状态 → 提交参数；开 + 高是供应商默认，不带任何参数 */
function buildThinkingParams(baseUrl: string, s: ThinkingState): string | null {
  if (isQwenUrl(baseUrl)) {
    return s.on ? '{"enable_thinking": true}' : '{"enable_thinking": false}';
  }
  if (!s.on) return '{"thinking": {"type": "disabled"}}';
  if (s.effort === 'high') return null;
  return JSON.stringify({ thinking: { type: 'enabled' }, reasoning_effort: s.effort });
}

// 工具名标签配色：抓取工具蓝、写库工具绿、LLM 调用紫，其余默认
const TOOL_TAG_COLOR: Record<string, string> = {
  xianyu_search: 'blue',
  save_crawl_result: 'green',
  llm_call: 'purple',
  crawl_select: 'purple',
  refine_search_keyword: 'purple',
};

/** JSON 美化：能解析则缩进展示，否则原样输出 */
function prettyJson(raw: string): string {
  try {
    return JSON.stringify(JSON.parse(raw), null, 2);
  } catch {
    return raw;
  }
}

export function AiPage() {
  const { message, modal } = AntApp.useApp();
  const queryClient = useQueryClient();

  const { data: ai } = useQuery({
    queryKey: ['ai'],
    queryFn: async () => {
      const [providers, status, prompt, crawlMode] = await Promise.all([
        apiGet<AiProvider[]>('/api/ai/providers'),
        apiGet<AiStatus>('/api/ai/status'),
        apiGet<CrawlPrompt>('/api/ai/crawl-prompt'),
        apiGet<CrawlModeResponse>('/api/ai/crawl-mode'),
      ]);
      return { providers, status, prompt: prompt.custom_prompt, crawlMode: crawlMode.mode };
    },
  });
  const providers = ai?.providers ?? [];
  const status = ai?.status ?? null;

  // ---------- 抓取模式切换（direct 单轮调用 / agent ReAct，下一轮抓取生效） ----------
  const [modeSaving, setModeSaving] = useState(false);
  const switchCrawlMode = async (v: string | number) => {
    setModeSaving(true);
    try {
      await apiPut('/api/ai/crawl-mode', { mode: String(v) });
      message.success('抓取模式已切换，下一轮抓取生效');
      refresh();
    } catch (e) {
      message.error(`切换失败: ${(e as Error).message}`);
    } finally {
      setModeSaving(false);
    }
  };

  const refresh = () => queryClient.invalidateQueries({ queryKey: ['ai'] });

  // ---------- 供应商表单 ----------
  const [editingId, setEditingId] = useState<number | null>(null);
  const [thinking, setThinking] = useState<ThinkingState>(DEFAULT_THINKING);
  const [form] = Form.useForm<ProviderFormValues>();
  const watchedBaseUrl = Form.useWatch('base_url', form) ?? '';

  const applyPreset = (key: string) => {
    const preset = AI_PRESETS[key];
    if (!preset) return;
    form.setFieldsValue({
      name: preset.name || form.getFieldValue('name'),
      base_url: preset.base_url,
      model: preset.model,
    });
  };

  const submit = async () => {
    const values = await form.validateFields();
    const payload = {
      name: values.name.trim(),
      base_url: values.base_url.trim(),
      api_key: values.api_key?.trim() || null,
      model: values.model.trim(),
      timeout_secs: values.timeout_secs || 60,
      extra_params: buildThinkingParams(values.base_url, thinking),
    };
    const isEdit = editingId !== null;
    try {
      if (isEdit) {
        await apiPut(`/api/ai/providers/${editingId}`, payload);
      } else {
        await apiPost('/api/ai/providers', payload);
      }
      message.success(isEdit ? '已保存修改' : '已添加配置');
      setEditingId(null);
      setThinking(DEFAULT_THINKING);
      form.resetFields();
      refresh();
    } catch (e) {
      message.error(`${isEdit ? '更新' : '添加'}失败: ${(e as Error).message}`);
    }
  };

  const startEdit = async (id: number) => {
    try {
      const p = await apiGet<AiProvider>(`/api/ai/providers/${id}`);
      setThinking(parseThinkingParams(p.extra_params) ?? DEFAULT_THINKING);
      setEditingId(p.id);
      form.setFieldsValue({
        name: p.name,
        base_url: p.base_url,
        api_key: undefined, // 密钥不回填，留空表示不修改
        model: p.model,
        timeout_secs: p.timeout_secs,
      });
    } catch (e) {
      message.error(`加载配置失败: ${(e as Error).message}`);
    }
  };

  const test = async (id: number) => {
    try {
      const data = await apiPost<TestConnectionResponse>(`/api/ai/providers/${id}/test`);
      message.success(`连通正常，耗时 ${data.latency_ms} ms，模型回复：${data.reply}`);
    } catch (e) {
      message.error(`测试失败: ${(e as Error).message}`);
    }
  };

  const setDefault = async (id: number) => {
    try {
      await apiPost(`/api/ai/providers/${id}/default`);
      refresh();
    } catch (e) {
      message.error(`设置默认失败: ${(e as Error).message}`);
    }
  };

  const confirmDelete = (p: AiProvider) => {
    modal.confirm({
      title: `删除 AI 配置「${p.name}」？`,
      content: '删除后不可恢复；若该配置为默认，需重新指定默认配置。',
      okText: '确认删除',
      okButtonProps: { danger: true },
      cancelText: '取消',
      onOk: async () => {
        try {
          await apiDelete(`/api/ai/providers/${p.id}`);
          if (editingId === p.id) {
            setEditingId(null);
            form.resetFields();
          }
          message.success('已删除');
          refresh();
        } catch (e) {
          message.error(`删除失败: ${(e as Error).message}`);
        }
      },
    });
  };

  // ---------- 抓取提示词 ----------
  const [promptText, setPromptText] = useState('');
  const [promptSaving, setPromptSaving] = useState(false);

  useEffect(() => {
    if (ai) setPromptText(ai.prompt);
  }, [ai]);

  const savePrompt = async () => {
    setPromptSaving(true);
    try {
      await apiPut('/api/ai/crawl-prompt', { custom_prompt: promptText });
      message.success('抓取提示词已保存，下一轮抓取生效');
      refresh();
    } catch (e) {
      message.error(`保存失败: ${(e as Error).message}`);
    } finally {
      setPromptSaving(false);
    }
  };

  // ---------- 工具调用记录 ----------
  const [callsQuery, setCallsQuery] = useState<{
    page: number;
    pageSize: number;
    toolName: string | null;
    failed: boolean | null;
  }>({ page: 1, pageSize: 20, toolName: null, failed: null });

  // 筛选下拉的工具名选项（库中实际出现过的工具）
  const { data: toolNames } = useQuery({
    queryKey: ['aiCallNames'],
    queryFn: () => apiGet<string[]>('/api/ai/tool-calls/names'),
  });

  const { data: toolCalls } = useQuery({
    queryKey: ['aiCalls', callsQuery],
    placeholderData: keepPreviousData,
    queryFn: () => {
      const params = new URLSearchParams({
        page: String(callsQuery.page),
        page_size: String(callsQuery.pageSize),
      });
      if (callsQuery.toolName) params.set('tool_name', callsQuery.toolName);
      if (callsQuery.failed !== null) params.set('failed', String(callsQuery.failed));
      return apiGet<PageResponse<AiToolCall>>(`/api/ai/tool-calls?${params}`);
    },
  });

  // ---------- 清理历史（保留期管理）----------
  const [purgeOpen, setPurgeOpen] = useState(false);
  const [purgeMode, setPurgeMode] = useState<'before_days' | 'keep_latest'>('before_days');
  const [purgeDays, setPurgeDays] = useState(30);
  const [purgeKeep, setPurgeKeep] = useState(1000);

  const purgeRequest = (): AiToolCallPurgeRequest =>
    purgeMode === 'before_days'
      ? { before_days: purgeDays, keep_latest: null }
      : { before_days: null, keep_latest: purgeKeep };

  const refreshCalls = () => {
    queryClient.invalidateQueries({ queryKey: ['aiCalls'] });
    queryClient.invalidateQueries({ queryKey: ['aiCallNames'] });
  };

  const submitPurge = async () => {
    let preview: AiToolCallPurgePreviewResponse;
    try {
      preview = await apiPost<AiToolCallPurgePreviewResponse>(
        '/api/ai/tool-calls/purge/preview',
        purgeRequest(),
      );
    } catch (e) {
      message.error(`预览失败: ${(e as Error).message}`);
      return;
    }
    if (preview.matched === 0) {
      message.info('没有命中清理条件的记录');
      return;
    }
    modal.confirm({
      title: '清理工具调用记录？',
      content:
        purgeMode === 'before_days'
          ? `将删除 ${purgeDays} 天前的 ${preview.matched} 条调用记录，删除后不可恢复。`
          : `将仅保留最新 ${purgeKeep} 条，删除其余 ${preview.matched} 条调用记录，删除后不可恢复。`,
      okText: '确认删除',
      okButtonProps: { danger: true },
      cancelText: '取消',
      onOk: async () => {
        try {
          const res = await apiPost<AiToolCallPurgeResponse>(
            '/api/ai/tool-calls/purge',
            purgeRequest(),
          );
          message.success(`已清理 ${res.deleted} 条调用记录`);
          setPurgeOpen(false);
          refreshCalls();
        } catch (e) {
          message.error(`清理失败: ${(e as Error).message}`);
        }
      },
    });
  };

  return (
    <div>
      <PageHeader title="AI 配置" description="接口供应商、抓取提示词与工具调用审计" />
      <Card>
        <Tabs
          items={[
            {
              key: 'providers',
              label: '接口配置',
              children: (
                <Space direction="vertical" size={16} style={{ width: '100%' }}>
                  {status && !status.configured && (
                    <Alert
                      type="warning"
                      showIcon
                      message="尚未配置 AI 接口（请在下方添加或设置 AI_API_KEY 环境变量）"
                    />
                  )}
                  <Card size="small" title={editingId !== null ? '编辑配置' : '添加配置'}>
                    <Form
                      form={form}
                      layout="vertical"
                      initialValues={{ base_url: 'https://api.openai.com/v1', model: 'gpt-4o-mini', timeout_secs: 60 }}
                    >
                      <Row gutter={12}>
                        <Col xs={24} sm={12} lg={6}>
                          <Form.Item label="供应商模板" style={{ marginBottom: 12 }}>
                            <Select
                              allowClear
                              placeholder="选择模板自动填充"
                              onChange={applyPreset}
                              options={Object.entries(AI_PRESETS).map(([key, preset]) => ({
                                label: preset.name || key,
                                value: key,
                              }))}
                            />
                          </Form.Item>
                        </Col>
                        <Col xs={24} sm={12} lg={6}>
                          <Form.Item
                            name="name"
                            label="名称"
                            rules={[{ required: true, message: '必填' }]}
                            style={{ marginBottom: 12 }}
                          >
                            <Input placeholder="如：DeepSeek" />
                          </Form.Item>
                        </Col>
                        <Col xs={24} sm={12} lg={8}>
                          <Form.Item
                            name="base_url"
                            label="Base URL"
                            rules={[{ required: true, message: '必填' }]}
                            style={{ marginBottom: 12 }}
                          >
                            <Input />
                          </Form.Item>
                        </Col>
                        <Col xs={24} sm={12} lg={4}>
                          <Form.Item name="timeout_secs" label="超时（秒）" style={{ marginBottom: 12 }}>
                            <InputNumber min={1} style={{ width: '100%' }} />
                          </Form.Item>
                        </Col>
                        <Col xs={24} sm={12} lg={10}>
                          <Form.Item
                            name="api_key"
                            label="API Key"
                            style={{ marginBottom: 12 }}
                          >
                            <Input.Password
                              autoComplete="off"
                              placeholder={editingId !== null ? '留空不修改' : 'API Key'}
                            />
                          </Form.Item>
                        </Col>
                        <Col xs={24} sm={12} lg={8}>
                          <Form.Item
                            name="model"
                            label="模型"
                            rules={[{ required: true, message: '必填' }]}
                            style={{ marginBottom: 12 }}
                          >
                            <Input placeholder="如：gpt-4o-mini" />
                          </Form.Item>
                        </Col>
                        <Col xs={24} lg={6}>
                          <Form.Item label=" " colon={false} style={{ marginBottom: 12 }}>
                            <Space>
                              <Button type="primary" onClick={submit}>
                                {editingId !== null ? '保存修改' : '添加配置'}
                              </Button>
                              {editingId !== null && (
                                <Button
                                  onClick={() => {
                                    setEditingId(null);
                                    setThinking(DEFAULT_THINKING);
                                    form.resetFields();
                                  }}
                                >
                                  取消编辑
                                </Button>
                              )}
                            </Space>
                          </Form.Item>
                        </Col>
                      </Row>
                      <Form.Item label="思考模式" style={{ marginBottom: 0 }}>
                        <Space size={12} wrap>
                          <Switch
                            checked={thinking.on}
                            onChange={(on) => setThinking((t) => ({ ...t, on }))}
                            checkedChildren="开"
                            unCheckedChildren="关"
                          />
                          {thinking.on && !isQwenUrl(watchedBaseUrl) && (
                            <Segmented
                              size="small"
                              value={thinking.effort}
                              onChange={(v) =>
                                setThinking((t) => ({ ...t, effort: v as ThinkingEffort }))
                              }
                              options={[
                                { label: '低', value: 'low' },
                                { label: '高（默认）', value: 'high' },
                                { label: '最高', value: 'max' },
                              ]}
                            />
                          )}
                          <Typography.Text type="secondary" style={{ fontSize: 12 }}>
                            {(() => {
                              const params = buildThinkingParams(watchedBaseUrl, thinking);
                              if (!thinking.on) {
                                return `已关闭思考：筛选类任务最省 token、延迟最低。将发送：${params}`;
                              }
                              return params
                                ? `将发送：${params}`
                                : '供应商默认即开启思考（高强度），无需额外参数';
                            })()}
                          </Typography.Text>
                        </Space>
                      </Form.Item>
                    </Form>
                    <Typography.Text type="secondary" style={{ fontSize: 12 }}>
                      选择模板可自动填入推荐的 Base URL 与模型，也可手动修改。编辑时密钥留空表示不修改。
                    </Typography.Text>
                  </Card>
                  <Table<AiProvider>
                    rowKey="id"
                    size="small"
                    dataSource={providers}
                    pagination={false}
                    locale={{ emptyText: '暂无 AI 配置' }}
                    columns={[
                      { title: '名称', dataIndex: 'name' },
                      { title: 'Base URL', dataIndex: 'base_url', ellipsis: true },
                      { title: '模型', dataIndex: 'model' },
                      { title: '密钥', dataIndex: 'api_key', render: (v: string | null) => v || '-' },
                      {
                        title: '思考模式',
                        dataIndex: 'extra_params',
                        width: 110,
                        render: (v: string | null) => {
                          const t = parseThinkingParams(v);
                          if (t === null) {
                            // 无法识别的自定义参数（后端仍支持，前端已不再提供入口）
                            return (
                              <Tooltip title={v}>
                                <span className="num" style={{ fontSize: 12, opacity: 0.75 }}>{v}</span>
                              </Tooltip>
                            );
                          }
                          if (!t.on) return <Tag style={{ marginInlineEnd: 0 }}>关思考</Tag>;
                          if (!v) return <span style={{ opacity: 0.5 }}>开 · 默认</span>;
                          return (
                            <Tag color="blue" style={{ marginInlineEnd: 0 }}>
                              思考 · {EFFORT_LABEL[t.effort]}
                            </Tag>
                          );
                        },
                      },
                      {
                        title: '默认',
                        dataIndex: 'is_default',
                        width: 80,
                        render: (v: boolean) => (v ? <Tag color="gold">默认</Tag> : '-'),
                      },
                      {
                        title: '操作',
                        key: 'actions',
                        width: 220,
                        render: (_, p) => (
                          <Space split={<Typography.Text type="secondary">|</Typography.Text>} size={2}>
                            <Button type="link" size="small" style={{ padding: 0 }} onClick={() => startEdit(p.id)}>
                              编辑
                            </Button>
                            <Button type="link" size="small" style={{ padding: 0 }} onClick={() => test(p.id)}>
                              测试
                            </Button>
                            {!p.is_default && (
                              <Button type="link" size="small" style={{ padding: 0 }} onClick={() => setDefault(p.id)}>
                                设为默认
                              </Button>
                            )}
                            <Button type="link" size="small" danger style={{ padding: 0 }} onClick={() => confirmDelete(p)}>
                              删除
                            </Button>
                          </Space>
                        ),
                      },
                    ]}
                  />
                </Space>
              ),
            },
            {
              key: 'prompt',
              label: '抓取提示词',
              children: (
                <Space direction="vertical" size={12} style={{ width: '100%' }}>
                  <Space size={12} align="center" wrap>
                    <Typography.Text strong>抓取模式</Typography.Text>
                    <Segmented
                      value={ai?.crawlMode ?? 'direct'}
                      disabled={modeSaving}
                      onChange={switchCrawlMode}
                      options={[
                        { label: '单轮调用（省 token）', value: 'direct' },
                        { label: 'ReAct 工具循环', value: 'agent' },
                      ]}
                    />
                    <Typography.Text type="secondary" style={{ fontSize: 12 }}>
                      {ai?.crawlMode === 'agent'
                        ? 'AI 自主多轮调用搜索/提交工具，灵活但 token 消耗高'
                        : 'Rust 搜索 + AI 一次筛选，token 约为前者的 1/3'}
                    </Typography.Text>
                  </Space>
                  <Alert
                    type="info"
                    showIcon
                    message="自定义 AI 抓取时的筛选与定价规则，保存后下一轮抓取生效（无需重启）。"
                    description="定价规则以折扣系数表达时（如「打八折」），AI 会按系数计算该商品的回收价（回收价 = 中位数 × 系数）；未匹配到规则的商品使用默认系数。"
                  />
                  <Input.TextArea
                    autoSize={{ minRows: 5, maxRows: 14 }}
                    maxLength={2000}
                    showCount
                    placeholder={
                      '例：CPU 类商品回收价打九折（0.9），显示器类打八折（0.8）；\n求购帖、配件帖一律不选；只选个人卖家。'
                    }
                    value={promptText}
                    onChange={(e) => setPromptText(e.target.value)}
                  />
                  <Space>
                    <Button type="primary" loading={promptSaving} onClick={savePrompt}>
                      保存提示词
                    </Button>
                    <Button
                      type="link"
                      style={{ padding: 0 }}
                      onClick={() =>
                        setPromptText(
                          'CPU 类商品回收价打九折（0.9），显示器类打八折（0.8）；\n求购帖、配件帖一律不选；只选个人卖家。',
                        )
                      }
                    >
                      填入示例
                    </Button>
                  </Space>
                </Space>
              ),
            },
            {
              key: 'calls',
              label: `调用记录（${toolCalls?.total ?? 0}）`,
              children: (
                <Space direction="vertical" size={12} style={{ width: '100%' }}>
                  {/* 筛选栏：工具名 / 成败 / 清理历史 */}
                  <Space wrap>
                    <Select
                      allowClear
                      placeholder="全部工具"
                      style={{ width: 190 }}
                      value={callsQuery.toolName}
                      options={(toolNames ?? []).map((n) => ({ value: n, label: n }))}
                      onChange={(v) =>
                        setCallsQuery((q) => ({ ...q, page: 1, toolName: v ?? null }))
                      }
                    />
                    <Select
                      style={{ width: 120 }}
                      value={callsQuery.failed}
                      options={[
                        { value: null, label: '全部结果' },
                        { value: false, label: '成功' },
                        { value: true, label: '失败' },
                      ]}
                      onChange={(v) =>
                        setCallsQuery((q) => ({ ...q, page: 1, failed: v }))
                      }
                    />
                    <Button danger onClick={() => setPurgeOpen(true)}>
                      清理历史
                    </Button>
                  </Space>
                  <Table<AiToolCall>
                  rowKey="id"
                  size="small"
                  dataSource={toolCalls?.items ?? []}
                  locale={{ emptyText: '暂无工具调用记录' }}
                  expandable={{
                    // 展开行：格式化后的完整参数 JSON 与完整结果/错误
                    expandedRowRender: (c) => (
                      <div style={{ display: 'grid', gap: 10, padding: '4px 0' }}>
                        <div>
                          <Typography.Text type="secondary" style={{ fontSize: 12 }}>
                            参数
                          </Typography.Text>
                          <pre className="num" style={{ margin: '4px 0 0', fontSize: 12, whiteSpace: 'pre-wrap', wordBreak: 'break-all' }}>
                            {prettyJson(c.arguments)}
                          </pre>
                        </div>
                        {c.result && (
                          <div>
                            <Typography.Text type="secondary" style={{ fontSize: 12 }}>
                              结果
                            </Typography.Text>
                            <pre className="num" style={{ margin: '4px 0 0', fontSize: 12, whiteSpace: 'pre-wrap', wordBreak: 'break-all' }}>
                              {prettyJson(c.result)}
                            </pre>
                          </div>
                        )}
                        {c.error && (
                          <div>
                            <Typography.Text type="secondary" style={{ fontSize: 12 }}>
                              错误
                            </Typography.Text>
                            <div style={{ marginTop: 4 }}>
                              <Typography.Text type="danger" style={{ fontSize: 12 }}>
                                {c.error}
                              </Typography.Text>
                            </div>
                          </div>
                        )}
                      </div>
                    ),
                  }}
                  columns={[
                    {
                      title: '时间',
                      dataIndex: 'created_at',
                      width: 165,
                      render: (v: number) => (
                        <span className="num" style={{ fontSize: 12 }}>
                          {fmtTime(v)}
                        </span>
                      ),
                    },
                    {
                      title: '工具',
                      dataIndex: 'tool_name',
                      width: 170,
                      render: (v: string) => (
                        <Tag color={TOOL_TAG_COLOR[v] ?? 'default'} style={{ marginInlineEnd: 0 }}>
                          {v}
                        </Tag>
                      ),
                    },
                    {
                      title: '参数',
                      dataIndex: 'arguments',
                      ellipsis: true,
                      render: (v: string) => (
                        <span className="num" style={{ fontSize: 12, opacity: 0.75 }}>
                          {v}
                        </span>
                      ),
                    },
                    {
                      title: '结果',
                      key: 'result',
                      width: 90,
                      render: (_, c) =>
                        c.result ? (
                          <Tag color="success" style={{ marginInlineEnd: 0 }}>成功</Tag>
                        ) : c.error ? (
                          <Tag color="error" style={{ marginInlineEnd: 0 }}>失败</Tag>
                        ) : (
                          '-'
                        ),
                    },
                    {
                      title: '耗时',
                      dataIndex: 'duration_ms',
                      width: 90,
                      align: 'right',
                      render: (v: number) => (
                        <span className="num">{v >= 1000 ? `${(v / 1000).toFixed(1)} s` : `${v} ms`}</span>
                      ),
                    },
                    {
                      title: 'Token 入/出',
                      key: 'tokens',
                      width: 150,
                      align: 'right',
                      render: (_, c) =>
                        c.input_tokens == null ? (
                          '-'
                        ) : (
                          <Tooltip
                            title={
                              (c.cached_input_tokens ?? 0) > 0
                                ? `输入 ${c.input_tokens}（其中命中缓存 ${c.cached_input_tokens}）/ 输出 ${c.output_tokens ?? 0}`
                                : `输入 ${c.input_tokens} / 输出 ${c.output_tokens ?? 0}`
                            }
                          >
                            <span className="num" style={{ fontSize: 12 }}>
                              {c.input_tokens} / {c.output_tokens ?? 0}
                              {(c.cached_input_tokens ?? 0) > 0 && (
                                <span style={{ opacity: 0.6 }}>（缓存 {c.cached_input_tokens}）</span>
                              )}
                            </span>
                          </Tooltip>
                        ),
                    },
                  ]}
                  onChange={(pagination) =>
                    setCallsQuery((q) => ({
                      ...q,
                      page: pagination.current ?? 1,
                      pageSize: pagination.pageSize ?? 20,
                    }))
                  }
                  pagination={{
                    current: callsQuery.page,
                    pageSize: callsQuery.pageSize,
                    total: toolCalls?.total ?? 0,
                    showSizeChanger: true,
                    showTotal: (t) => `共 ${t} 条`,
                  }}
                />
                </Space>
              ),
            },
          ]}
        />
      </Card>

      {/* 清理历史：选择保留策略 → 预览命中数 → 确认后执行 */}
      <Modal
        title="清理工具调用记录"
        open={purgeOpen}
        onOk={submitPurge}
        onCancel={() => setPurgeOpen(false)}
        okText="预览并清理"
        cancelText="取消"
        width={420}
      >
        <Space direction="vertical" size={14} style={{ width: '100%', marginTop: 8 }}>
          <Radio.Group
            value={purgeMode}
            onChange={(e) => setPurgeMode(e.target.value)}
            options={[
              { value: 'before_days', label: '删除 N 天前的记录' },
              { value: 'keep_latest', label: '仅保留最新 N 条' },
            ]}
          />
          {purgeMode === 'before_days' ? (
            <Space>
              <span>删除</span>
              <InputNumber min={0} max={3650} value={purgeDays} onChange={(v) => setPurgeDays(v ?? 30)} />
              <span>天前的全部记录（0 = 清空全部）</span>
            </Space>
          ) : (
            <Space>
              <span>仅保留最新</span>
              <InputNumber min={0} max={1000000} value={purgeKeep} onChange={(v) => setPurgeKeep(v ?? 1000)} />
              <span>条，删除其余记录</span>
            </Space>
          )}
          <Typography.Text type="secondary" style={{ fontSize: 12 }}>
            审计记录只增不改，清理是保留期管理；删除后不可恢复。
          </Typography.Text>
        </Space>
      </Modal>
    </div>
  );
}
