import { defineConfig } from '@rsbuild/core';
import { pluginSass } from '@rsbuild/plugin-sass';
import { minify } from 'html-minifier-terser';

const frontendPort = Number(process.env.FRONTEND_PORT ?? 3000);
const backendPort = process.env.BACKEND_PORT ?? '8000';
const siteUrl = process.env.SITE_URL?.replace(/\/+$/, '') ?? '';

export default defineConfig({
  plugins: [
    pluginSass()
  ],
  html: {
    template: './src/client/index.html',
    templateParameters: {
      siteUrl
    }
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
  tools: {
    cssLoader: {
      url: {
        filter: (url: string) => !url.startsWith('/')
      }
    },
    htmlPlugin: {
      minify: (html: string) => minify(html, {
        collapseWhitespace: true,
        collapseBooleanAttributes: true,
        removeComments: true,
        removeRedundantAttributes: true,
        removeScriptTypeAttributes: true,
        removeStyleLinkTypeAttributes: true,
        removeOptionalTags: true,
        useShortDoctype: true,
        minifyCSS: true,
        minifyJS: true
      })
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
