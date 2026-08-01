import type { ReactNode } from 'react';
import { Space, Typography } from 'antd';

interface Props {
  title: string;
  description?: string;
  extra?: ReactNode;
}

/** 每页统一的页头：标题 + 描述 + 右侧主操作 */
export function PageHeader({ title, description, extra }: Props) {
  return (
    <div
      style={{
        display: 'flex',
        flexWrap: 'wrap',
        alignItems: 'flex-end',
        justifyContent: 'space-between',
        gap: 12,
        marginBottom: 16,
      }}
    >
      <div>
        <Typography.Title level={4} style={{ margin: 0 }}>
          {title}
        </Typography.Title>
        {description && (
          <Typography.Text type="secondary" style={{ fontSize: 13 }}>
            {description}
          </Typography.Text>
        )}
      </div>
      {extra && <Space wrap>{extra}</Space>}
    </div>
  );
}
