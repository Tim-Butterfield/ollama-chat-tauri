import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react()],
  server: {
    port: 3000,  // ✅ Ensure this matches tauri.conf.json
  },
  build: {
    outDir: 'dist',
    // Tauri ships a modern system WebView, so target modern engines. This also
    // avoids esbuild 0.28's unsupported destructuring downlevel for old Safari.
    target: ['es2022', 'chrome105', 'safari15']
  }
});