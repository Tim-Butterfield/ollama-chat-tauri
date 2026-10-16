import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import tailwindcss from '@tailwindcss/vite';

export default defineConfig({
  plugins: [react(), tailwindcss()],
  server: {
    port: 3000,  // ✅ Ensure this matches tauri.conf.json
  },
  build: {
    outDir: 'dist',
    // Tauri ships a modern system WebView, so target modern engines.
    // Tailwind CSS v4 requires Safari 16.4+ and Chrome 111+.
    target: ['es2022', 'chrome111', 'safari16.4']
  }
});
