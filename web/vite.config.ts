import path from "path"
import react from "@vitejs/plugin-react"
import { defineConfig } from "vite"
import { inspectAttr } from 'kimi-plugin-inspect-react'

// https://vite.dev/config/
export default defineConfig({
  base: './',
  plugins: [inspectAttr(), react()],
  server: {
    // 后端 axum 默认占用 3000，dev server 用 5173 避让
    port: 5173,
    proxy: {
      '/api': {
        target: 'http://127.0.0.1:3000',
        changeOrigin: true,
      },
    },
  },
  build: {
    // 构建产物直接输出到 axum 托管的 static/（STATIC_DIR 默认值）
    outDir: '../static',
    emptyOutDir: true,
    rollupOptions: {
      output: {
        // 大依赖独立分包，利用浏览器长缓存
        manualChunks: {
          antd: ['antd', '@ant-design/icons'],
          charts: ['recharts'],
          react: ['react', 'react-dom', 'react-router'],
        },
      },
    },
  },
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
    },
  },
});
