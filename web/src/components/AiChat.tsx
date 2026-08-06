import { useCallback, useEffect, useRef, useState } from 'react';
import {
  App as AntApp,
  Alert,
  Button,
  Descriptions,
  Drawer,
  Dropdown,
  Empty,
  Input,
  List,
  Space,
  Spin,
  Tag,
  theme as antdTheme,
  Tooltip,
  Typography,
} from 'antd';
import {
  AppstoreOutlined,
  DeleteOutlined,
  EditOutlined,
  EllipsisOutlined,
  PlusOutlined,
  RobotOutlined,
  SendOutlined,
} from '@ant-design/icons';
import { keepPreviousData, useQuery, useQueryClient } from '@tanstack/react-query';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { apiDelete, apiGet, apiPost, apiPut } from '@/lib/api';
import type {
  AiChatResponse,
  AiToolInfoResponse,
  ConversationDetailResponse,
  ConversationResponse,
} from '@/types/api';

interface ChatMessage {
  role: 'user' | 'assistant';
  content: string;
}

const SUGGESTIONS = [
  '系统里有多少商品？',
  '最近 24 小时抓取了多少条数据？',
  '现在有哪些队列在运行？',
  '给「显卡」标签下的商品创建一个抓取队列',
  '看看价格最高的几个商品',
];

/** AI 回答的 markdown 渲染（GFM：表格/任务列表/删除线）。样式贴合 antd 主题 token */
function MarkdownContent({ text, token }: { text: string; token: ReturnType<typeof antdTheme.useToken>['token'] }) {
  return (
    <ReactMarkdown
      remarkPlugins={[remarkGfm]}
      components={{
        p: ({ children }) => <p style={{ margin: '4px 0' }}>{children}</p>,
        a: ({ children, href }) => (
          <a href={href} target="_blank" rel="noreferrer" style={{ color: token.colorLink }}>
            {children}
          </a>
        ),
        code: ({ className, children }) => {
          const inline = !className;
          return inline ? (
            <code
              style={{
                background: token.colorFillTertiary,
                borderRadius: 4,
                padding: '1px 5px',
                fontFamily: "ui-monospace, 'JetBrains Mono', Consolas, monospace",
                fontSize: '0.92em',
              }}
            >
              {children}
            </code>
          ) : (
            <pre
              style={{
                background: token.colorBgLayout,
                borderRadius: token.borderRadius,
                padding: '10px 12px',
                overflowX: 'auto',
                margin: '6px 0',
              }}
            >
              <code className={className} style={{ fontFamily: "ui-monospace, 'JetBrains Mono', Consolas, monospace", fontSize: 12 }}>
                {children}
              </code>
            </pre>
          );
        },
        table: ({ children }) => (
          <div style={{ overflowX: 'auto', margin: '6px 0' }}>
            <table style={{ borderCollapse: 'collapse', width: '100%', fontSize: 12.5 }}>{children}</table>
          </div>
        ),
        th: ({ children }) => (
          <th
            style={{
              border: `1px solid ${token.colorBorderSecondary}`,
              padding: '5px 9px',
              background: token.colorFillTertiary,
              textAlign: 'left',
              whiteSpace: 'nowrap',
            }}
          >
            {children}
          </th>
        ),
        td: ({ children }) => (
          <td style={{ border: `1px solid ${token.colorBorderSecondary}`, padding: '4px 9px' }}>{children}</td>
        ),
        ul: ({ children }) => <ul style={{ margin: '4px 0', paddingLeft: 20 }}>{children}</ul>,
        ol: ({ children }) => <ol style={{ margin: '4px 0', paddingLeft: 20 }}>{children}</ol>,
        li: ({ children }) => <li style={{ margin: '2px 0' }}>{children}</li>,
        h1: ({ children }) => <h1 style={{ fontSize: 17, margin: '8px 0 4px' }}>{children}</h1>,
        h2: ({ children }) => <h2 style={{ fontSize: 15, margin: '8px 0 4px' }}>{children}</h2>,
        h3: ({ children }) => <h3 style={{ fontSize: 13.5, margin: '8px 0 4px' }}>{children}</h3>,
        blockquote: ({ children }) => (
          <blockquote
            style={{
              margin: '6px 0',
              paddingLeft: 10,
              borderLeft: `3px solid ${token.colorBorder}`,
              color: token.colorTextSecondary,
            }}
          >
            {children}
          </blockquote>
        ),
        hr: () => <hr style={{ border: `1px solid ${token.colorBorderSecondary}`, margin: '8px 0' }} />,
      }}
    >
      {text}
    </ReactMarkdown>
  );
}

export function AiChat({ configured }: { configured: boolean }) {
  const { message: toast, modal } = AntApp.useApp();
  const { token } = antdTheme.useToken();
  const queryClient = useQueryClient();

  // 会话列表
  const { data: sessions = [], isLoading: sessionsLoading } = useQuery({
    queryKey: ['aiSessions'],
    queryFn: () => apiGet<ConversationResponse[]>('/api/ai/chat/sessions'),
  });

  // 当前激活会话 id：优先 URL 记忆（localStorage），否则最新会话
  const [activeId, setActiveId] = useState<number | null>(null);
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [input, setInput] = useState('');
  const [loading, setLoading] = useState(false);
  const [toolsOpen, setToolsOpen] = useState(false);
  const endRef = useRef<HTMLDivElement>(null);

  // 会话列表加载完成后：无激活则选最新（或第一个）
  useEffect(() => {
    if (sessions.length > 0 && activeId === null) {
      setActiveId(sessions[0].id);
    }
  }, [sessions, activeId]);

  // 切换会话时加载消息
  const { data: detail, isFetching: detailLoading } = useQuery({
    queryKey: ['aiConversation', activeId],
    queryFn: () => apiGet<ConversationDetailResponse>(`/api/ai/chat/sessions/${activeId}`),
    enabled: activeId !== null,
  });

  useEffect(() => {
    if (detail) {
      setMessages(
        detail.messages.map((m) => ({ role: m.role as 'user' | 'assistant', content: m.content })),
      );
    } else if (!detailLoading) {
      setMessages([]);
    }
  }, [detail, detailLoading]);

  // 可用工具清单（打开抽屉时懒加载，点击前不请求）
  const { data: tools, isFetching: toolsLoading } = useQuery({
    queryKey: ['aiTools'],
    queryFn: () => apiGet<AiToolInfoResponse[]>('/api/ai/tools'),
    placeholderData: keepPreviousData,
    enabled: toolsOpen,
  });

  const refreshSessions = useCallback(() => {
    queryClient.invalidateQueries({ queryKey: ['aiSessions'] });
  }, [queryClient]);

  useEffect(() => {
    endRef.current?.scrollIntoView({ behavior: 'smooth', block: 'end' });
  }, [messages, loading]);

  const createConversation = async () => {
    try {
      const c = await apiPost<ConversationResponse>('/api/ai/chat/sessions');
      setActiveId(c.id);
      setMessages([]);
      refreshSessions();
    } catch (e) {
      toast.error(`新建会话失败: ${(e as Error).message}`);
    }
  };

  const send = async (text: string) => {
    const trimmed = text.trim();
    if (!trimmed || loading || activeId === null) return;
    setMessages((m) => [...m, { role: 'user', content: trimmed }]);
    setInput('');
    setLoading(true);
    try {
      // 后端：落库用户消息 → 带会话历史跑 agent → 落 AI 回复
      const data = await apiPost<AiChatResponse>(`/api/ai/chat/sessions/${activeId}/messages`, {
        message: trimmed,
      });
      setMessages((m) => [...m, { role: 'assistant', content: data.reply }]);
      refreshSessions();
    } catch (e) {
      const err = (e as Error).message;
      toast.error(err);
      setMessages((m) => [...m, { role: 'assistant', content: `出错：${err}` }]);
    } finally {
      setLoading(false);
    }
  };

  const renameConversation = (c: ConversationResponse) => {
    modal.confirm({
      title: '重命名会话',
      icon: null,
      content: (
        <Input
          defaultValue={c.title}
          maxLength={40}
          onPressEnter={() => {
            // Enter 即确认
            const btn = document.querySelector(
              '.ant-modal-confirm .ant-modal-confirm-btns .ant-btn-primary',
            ) as HTMLButtonElement | null;
            btn?.click();
          }}
        />
      ),
      okText: '保存',
      cancelText: '取消',
      onOk: async () => {
        const inputEl = document.querySelector(
          '.ant-modal-confirm input',
        ) as HTMLInputElement | null;
        const title = inputEl?.value?.trim();
        if (!title) {
          toast.warning('名称不能为空');
          return;
        }
        try {
          await apiPut(`/api/ai/chat/sessions/${c.id}/title`, { title });
          refreshSessions();
        } catch (e) {
          toast.error(`重命名失败: ${(e as Error).message}`);
        }
      },
    });
  };

  const deleteConversation = (c: ConversationResponse) => {
    modal.confirm({
      title: `删除会话「${c.title}」？`,
      content: `该会话及其 ${c.message_count} 条消息将被永久删除。`,
      okText: '确认删除',
      okButtonProps: { danger: true },
      cancelText: '取消',
      onOk: async () => {
        try {
          await apiDelete(`/api/ai/chat/sessions/${c.id}`);
          if (activeId === c.id) {
            setActiveId(null);
            setMessages([]);
          }
          refreshSessions();
          toast.success('已删除');
        } catch (e) {
          toast.error(`删除失败: ${(e as Error).message}`);
        }
      },
    });
  };

  const activeConversation = sessions.find((s) => s.id === activeId) ?? null;

  return (
    <div
      style={{
        display: 'flex',
        border: `1px solid ${token.colorBorderSecondary}`,
        borderRadius: token.borderRadius,
        height: 'calc(100vh - 240px)',
        minHeight: 420,
        background: token.colorBgContainer,
      }}
    >
      {/* 会话列表侧栏 */}
      <div
        style={{
          width: 240,
          flexShrink: 0,
          borderRight: `1px solid ${token.colorBorderSecondary}`,
          display: 'flex',
          flexDirection: 'column',
        }}
      >
        <div style={{ padding: '10px 12px', borderBottom: `1px solid ${token.colorBorderSecondary}` }}>
          <Button type="primary" block icon={<PlusOutlined />} onClick={createConversation}>
            新建会话
          </Button>
        </div>
        <div style={{ flex: 1, overflowY: 'auto', padding: 6 }}>
          {sessionsLoading ? (
            <div style={{ textAlign: 'center', padding: 30 }}>
              <Spin size="small" />
            </div>
          ) : sessions.length === 0 ? (
            <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description="暂无会话" style={{ marginTop: 40 }} />
          ) : (
            <List
              size="small"
              dataSource={sessions}
              renderItem={(c) => (
                <List.Item
                  onClick={() => setActiveId(c.id)}
                  style={{
                    cursor: 'pointer',
                    padding: '8px 10px',
                    borderRadius: token.borderRadius,
                    background: c.id === activeId ? token.colorPrimaryBg : 'transparent',
                    display: 'flex',
                    alignItems: 'center',
                    gap: 6,
                  }}
                >
                  <span
                    style={{
                      flex: 1,
                      overflow: 'hidden',
                      textOverflow: 'ellipsis',
                      whiteSpace: 'nowrap',
                      fontSize: 13,
                    }}
                    title={c.title}
                  >
                    {c.title}
                  </span>
                  <Dropdown
                    trigger={['click']}
                    menu={{
                      items: [
                        { key: 'rename', icon: <EditOutlined />, label: '重命名' },
                        { key: 'delete', icon: <DeleteOutlined />, label: '删除', danger: true },
                      ],
                      onClick: ({ key, domEvent }) => {
                        domEvent.stopPropagation();
                        if (key === 'rename') renameConversation(c);
                        if (key === 'delete') deleteConversation(c);
                      },
                    }}
                  >
                    <Button type="text" size="small" style={{ width: 24, height: 24 }} onClick={(e) => e.stopPropagation()}>
                      <EllipsisOutlined style={{ fontSize: 12, opacity: 0.6 }} />
                    </Button>
                  </Dropdown>
                </List.Item>
              )}
            />
          )}
        </div>
      </div>

      {/* 聊天区 */}
      <div style={{ flex: 1, display: 'flex', flexDirection: 'column', minWidth: 0 }}>
        {/* 顶部提示 */}
        <div
          style={{
            padding: '10px 14px',
            borderBottom: `1px solid ${token.colorBorderSecondary}`,
            display: 'flex',
            alignItems: 'center',
            gap: 8,
            flexWrap: 'wrap',
          }}
        >
          <RobotOutlined style={{ color: token.colorPrimary }} />
          <Typography.Text strong>
            {activeConversation?.title || '管理助手'}
          </Typography.Text>
          <Typography.Text type="secondary" style={{ fontSize: 12 }}>
            {activeConversation ? `${activeConversation.message_count} 条消息` : '请选择或新建会话'}
          </Typography.Text>
          <span style={{ flex: 1 }} />
          <Tooltip title="查看全部可用工具">
            <Button
              type="link"
              size="small"
              style={{ padding: 0 }}
              icon={<AppstoreOutlined />}
              onClick={() => setToolsOpen(true)}
            >
              可用工具（{(tools ?? []).length || 19}）
            </Button>
          </Tooltip>
        </div>

        {!configured && (
          <Alert
            type="warning"
            showIcon
            style={{ margin: 10 }}
            message="尚未配置 AI 接口，助手无法使用"
            description="请先在「接口配置」页添加 AI 供应商或设置默认配置。"
          />
        )}

        {/* 消息区 */}
        <div
          style={{
            flex: 1,
            overflowY: 'auto',
            padding: '16px 18px',
            display: 'flex',
            flexDirection: 'column',
            gap: 14,
          }}
        >
          {messages.length === 0 && !loading && (
            <div
              style={{
                margin: 'auto',
                textAlign: 'center',
                color: token.colorTextSecondary,
                maxWidth: 460,
              }}
            >
              <RobotOutlined style={{ fontSize: 36, marginBottom: 12 }} />
              <Typography.Paragraph style={{ marginBottom: 16 }}>
                {activeConversation ? '输入你想问的或想做的事，比如：' : '先新建一个会话开始对话'}
              </Typography.Paragraph>
              {activeConversation && (
                <Space direction="vertical" size={8} style={{ width: '100%' }}>
                  {SUGGESTIONS.map((s) => (
                    <Button key={s} block disabled={!configured || loading} onClick={() => send(s)}>
                      {s}
                    </Button>
                  ))}
                </Space>
              )}
            </div>
          )}

          {messages.map((m, i) => (
            <div key={i} style={{ display: 'flex', justifyContent: m.role === 'user' ? 'flex-end' : 'flex-start' }}>
              <div
                style={{
                  maxWidth: '76%',
                  padding: '8px 12px',
                  borderRadius: token.borderRadius,
                  whiteSpace: m.role === 'user' ? 'pre-wrap' : 'normal',
                  wordBreak: 'break-word',
                  background: m.role === 'user' ? token.colorPrimary : token.colorFillTertiary,
                  color: m.role === 'user' ? '#fff' : token.colorText,
                  fontSize: 13,
                  lineHeight: 1.6,
                }}
              >
                {m.role === 'user' ? m.content : <MarkdownContent text={m.content} token={token} />}
              </div>
            </div>
          ))}

          {loading && (
            <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
              <Typography.Text type="secondary" style={{ fontSize: 13 }}>
                <Tag color="processing" style={{ marginInlineEnd: 0 }}>
                  AI 思考中
                </Tag>
              </Typography.Text>
              <Typography.Text type="secondary" style={{ fontSize: 12 }}>
                正在调用工具查询/操作，可能需要几十秒…
              </Typography.Text>
            </div>
          )}
          <div ref={endRef} />
        </div>

        {/* 输入区 */}
        <div
          style={{
            padding: '10px 14px',
            borderTop: `1px solid ${token.colorBorderSecondary}`,
            display: 'flex',
            gap: 8,
          }}
        >
          <Input.TextArea
            autoSize={{ minRows: 1, maxRows: 4 }}
            placeholder={
              !configured
                ? '请先配置 AI 接口'
                : activeConversation
                  ? '输入指令，如：给「CPU」标签下的商品建一个队列'
                  : '先新建一个会话'
            }
            disabled={!configured || loading || activeId === null}
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onPressEnter={(e) => {
              if (!e.shiftKey) {
                e.preventDefault();
                send(input);
              }
            }}
            style={{ flex: 1 }}
          />
          <Button
            type="primary"
            icon={<SendOutlined />}
            disabled={!configured || loading || activeId === null || !input.trim()}
            onClick={() => send(input)}
          >
            发送
          </Button>
        </div>
      </div>

      {/* 可用工具清单抽屉 */}
      <Drawer title={`可用工具（${tools?.length ?? 0}）`} open={toolsOpen} onClose={() => setToolsOpen(false)} width={520}>
        {toolsLoading && !tools ? (
          <div style={{ textAlign: 'center', padding: 40 }}>
            <Spin />
          </div>
        ) : !tools || tools.length === 0 ? (
          <Empty description="暂无可用工具" />
        ) : (
          <Space direction="vertical" size={12} style={{ width: '100%' }}>
            {tools.map((t) => (
              <div
                key={t.name}
                style={{
                  border: `1px solid ${token.colorBorderSecondary}`,
                  borderRadius: token.borderRadius,
                  padding: '10px 12px',
                }}
              >
                <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 6 }}>
                  <Tag color={WRITE_TOOLS.has(t.name) ? 'gold' : 'blue'} style={{ marginInlineEnd: 0 }}>
                    {t.name}
                  </Tag>
                  {WRITE_TOOLS.has(t.name) && (
                    <Typography.Text type="secondary" style={{ fontSize: 12 }}>
                      写操作
                    </Typography.Text>
                  )}
                </div>
                <Typography.Text style={{ fontSize: 13 }}>{t.description}</Typography.Text>
                <Descriptions
                  size="small"
                  column={1}
                  style={{ marginTop: 8 }}
                  items={[
                    {
                      key: 'params',
                      label: '参数',
                      children: (
                        <pre
                          className="num"
                          style={{
                            margin: 0,
                            fontSize: 11,
                            whiteSpace: 'pre-wrap',
                            wordBreak: 'break-all',
                            maxHeight: 160,
                            overflowY: 'auto',
                          }}
                        >
                          {JSON.stringify(t.parameters, null, 2)}
                        </pre>
                      ),
                    },
                  ]}
                />
              </div>
            ))}
          </Space>
        )}
      </Drawer>
    </div>
  );
}

// 写操作工具名集合：入队/创建/更新/删除等会真实改库的工具
const WRITE_TOOLS = new Set([
  'create_product',
  'update_product',
  'delete_product',
  'batch_create_products',
  'create_tag',
  'update_tag',
  'delete_tag',
  'enqueue',
  'pause_queue',
  'resume_queue',
  'cancel_queue',
]);
