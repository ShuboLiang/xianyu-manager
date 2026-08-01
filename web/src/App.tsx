import { useEffect, useState } from 'react';
import { App as AntApp, ConfigProvider, theme as antdTheme } from 'antd';
import zhCN from 'antd/locale/zh_CN';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { HashRouter, Navigate, Route, Routes } from 'react-router';
import { AppShell } from '@/AppShell';
import { ThemeModeContext, type ThemeMode } from '@/lib/theme-mode';
import { AiPage } from '@/pages/AiPage';
import { ItemsPage } from '@/pages/ItemsPage';
import { OverviewPage } from '@/pages/OverviewPage';
import { ProductsPage } from '@/pages/ProductsPage';
import { TagsPage } from '@/pages/TagsPage';
import { TrendsPage } from '@/pages/TrendsPage';

const queryClient = new QueryClient({
  defaultOptions: {
    queries: { retry: 1, refetchOnWindowFocus: false, staleTime: 5_000 },
  },
});

export default function App() {
  const [mode, setMode] = useState<ThemeMode>(() =>
    localStorage.getItem('theme') === 'dark' ? 'dark' : 'light',
  );

  useEffect(() => {
    localStorage.setItem('theme', mode);
    document.documentElement.style.colorScheme = mode;
  }, [mode]);

  return (
    <ConfigProvider
      locale={zhCN}
      theme={{
        cssVar: true,
        // 全局紧凑密度：数据密集型后台，表格/表单都偏紧凑
        algorithm:
          mode === 'dark'
            ? [antdTheme.darkAlgorithm, antdTheme.compactAlgorithm]
            : [antdTheme.defaultAlgorithm, antdTheme.compactAlgorithm],
        token: {
          // 琥珀主色，延续旧版「闲鱼黄」品牌感
          colorPrimary: '#d97706',
          colorInfo: '#d97706',
          // 文字链接与主色分离：链接用经典蓝，主按钮/选中态用琥珀，避免满屏橙色
          colorLink: '#1677ff',
          colorLinkHover: '#4096ff',
          colorLinkActive: '#0958d9',
          borderRadius: 6,
          fontFamily: `-apple-system, BlinkMacSystemFont, 'Segoe UI', 'PingFang SC', 'Hiragino Sans GB', 'Microsoft YaHei', 'Helvetica Neue', Helvetica, Arial, sans-serif`,
        },
      }}
    >
      <AntApp>
        <QueryClientProvider client={queryClient}>
          <ThemeModeContext.Provider
            value={{ mode, toggle: () => setMode((m) => (m === 'dark' ? 'light' : 'dark')) }}
          >
            <HashRouter>
              <Routes>
                <Route element={<AppShell />}>
                  <Route index element={<OverviewPage />} />
                  <Route path="products" element={<ProductsPage />} />
                  <Route path="tags" element={<TagsPage />} />
                  <Route path="items" element={<ItemsPage />} />
                  <Route path="trends" element={<TrendsPage />} />
                  <Route path="ai" element={<AiPage />} />
                  <Route path="*" element={<Navigate to="/" replace />} />
                </Route>
              </Routes>
            </HashRouter>
          </ThemeModeContext.Provider>
        </QueryClientProvider>
      </AntApp>
    </ConfigProvider>
  );
}
