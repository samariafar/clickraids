import { defineConfig } from '@rsbuild/core';
import { pluginSass } from '@rsbuild/plugin-sass';

const frontendPort = Number(process.env.FRONTEND_PORT ?? 3000);
const backendPort = process.env.BACKEND_PORT ?? '8000';

export default defineConfig({
  plugins: [
    pluginSass()
  ],
  html: {
    template: './src/client/index.html'
  },
  server: {
    host: '::',
    port: frontendPort,
    publicDir: {
      name: './src/client/static'
    },
    historyApiFallback: true,
    proxy: {
      '/ws': {
        target: `ws://localhost:${backendPort}`,
        ws: true
      }
    }
  },
  source: {
    entry: {
      index: './src/client/index.ts'
    }
  },
  output: {
    distPath: {
      root: './public',
      js: 'scripts',
      css: 'styles'
    }
  }
});
