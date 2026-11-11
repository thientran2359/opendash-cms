import { defineConfig, type ProxyOptions } from 'vite';
import react from '@vitejs/plugin-react';
import tailwindcss from '@tailwindcss/vite';

function localServer(port: number) {
  const target = 'http://127.0.0.1:18766';
  const origin = `http://127.0.0.1:${port}`;
  const backend: ProxyOptions = {
    target,
    changeOrigin: true,
    configure(proxy) {
      proxy.on('proxyReq', (outgoing, incoming) => {
        if (incoming.headers.origin === origin) outgoing.setHeader('origin', target);
        const referer = incoming.headers.referer;
        if (referer?.startsWith(origin + '/')) {
          outgoing.setHeader('referer', target + referer.slice(origin.length));
        }
      });
    },
  };
  return {
    host: '127.0.0.1',
    port,
    strictPort: true,
    proxy: { '^/(api/|auth/|registry$|page$|theme$|preview$|save$|$)': backend },
  };
}

export default defineConfig({
  plugins: [react(), tailwindcss()],
  base: '/',
  server: localServer(5173),
  preview: localServer(4173),
  build: { outDir: 'dist', emptyOutDir: true },
});
