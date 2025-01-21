import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react()],
  server: {
    port: 3000,  // ✅ Ensure this matches tauri.conf.json
  },
  build: {
    outDir: 'dist'
  }
});