import { useEffect, useRef, useState } from 'react';
import {
  App as AntApp,
  Alert,
  Button,
  Input,
  Space,
  Tag,
  theme as antdTheme,
  Typography,
} from 'antd';
import { RobotOutlined, SendOutlined } from '@ant-design/icons';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { apiPost } from '@/lib/api';
import type { AiChatResponse } from '@/types/api';

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
  const { message: toast } = AntApp.useApp();
  const { token } = antdTheme.useToken();
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [input, setInput] = useState('');
  const [loading, setLoading] = useState(false);
  const endRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    endRef.current?.scrollIntoView({ behavior: 'smooth', block: 'end' });
  }, [messages, loading]);

  const send = async (text: string) => {
    const trimmed = text.trim();
    if (!trimmed || loading) return;
    setMessages((m) => [...m, { role: 'user', content: trimmed }]);
    setInput('');
    setLoading(true);
    try {
      // 后端 run_agent：AI 自主多轮调用管理工具（商品/标签/队列/统计等）后给出回答
      const data = await apiPost<AiChatResponse>('/api/ai/chat', { message: trimmed });
      setMessages((m) => [...m, { role: 'assistant', content: data.reply }]);
    } catch (e) {
      const err = (e as Error).message;
      toast.error(err);
      setMessages((m) => [...m, { role: 'assistant', content: `出错：${err}` }]);
    } finally {
      setLoading(false);
    }
  };

  return (
    <div
      style={{
        display: 'flex',
        flexDirection: 'column',
        border: `1px solid ${token.colorBorderSecondary}`,
        borderRadius: token.borderRadius,
        height: 'calc(100vh - 240px)',
        minHeight: 420,
        background: token.colorBgContainer,
      }}
    >
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
        <Typography.Text strong>管理助手</Typography.Text>
        <Typography.Text type="secondary" style={{ fontSize: 12 }}>
          基于 19 个管理工具，AI 自主查改数据（商品 / 标签 / 队列 / 统计）
        </Typography.Text>
        <span style={{ flex: 1 }} />
        {messages.length > 0 && (
          <Button
            type="link"
            size="small"
            style={{ padding: 0 }}
            onClick={() => setMessages([])}
          >
            清空对话
          </Button>
        )}
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
              输入你想问的或想做的事，比如：
            </Typography.Paragraph>
            <Space direction="vertical" size={8} style={{ width: '100%' }}>
              {SUGGESTIONS.map((s) => (
                <Button
                  key={s}
                  block
                  disabled={!configured || loading}
                  onClick={() => send(s)}
                >
                  {s}
                </Button>
              ))}
            </Space>
          </div>
        )}

        {messages.map((m, i) => (
          <div
            key={i}
            style={{
              display: 'flex',
              justifyContent: m.role === 'user' ? 'flex-end' : 'flex-start',
            }}
          >
            <div
              style={{
                maxWidth: '76%',
                padding: '8px 12px',
                borderRadius: token.borderRadius,
                whiteSpace: m.role === 'user' ? 'pre-wrap' : 'normal',
                wordBreak: 'break-word',
                background:
                  m.role === 'user' ? token.colorPrimary : token.colorFillTertiary,
                color: m.role === 'user' ? '#fff' : token.colorText,
                fontSize: 13,
                lineHeight: 1.6,
              }}
            >
              {m.role === 'user' ? (
                m.content
              ) : (
                <MarkdownContent text={m.content} token={token} />
              )}
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
            configured ? '输入指令，如：给「CPU」标签下的商品建一个队列' : '请先配置 AI 接口'
          }
          disabled={!configured || loading}
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
          disabled={!configured || loading || !input.trim()}
          onClick={() => send(input)}
        >
          发送
        </Button>
      </div>
    </div>
  );
}
