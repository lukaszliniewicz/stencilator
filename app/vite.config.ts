import { readFileSync } from 'node:fs';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { defineConfig, type Plugin } from 'vite';

/**
 * The app fetches nothing but its own files and runs WebAssembly. A meta tag carries the
 * policy so that it holds wherever the folder is copied; servers can add headers on top.
 */
const CONTENT_SECURITY_POLICY = [
  "default-src 'none'",
  "script-src 'self' 'wasm-unsafe-eval'",
  "worker-src 'self'",
  "connect-src 'self'",
  "img-src 'self' blob: data:",
  "style-src 'self'",
  "font-src 'self'",
  "manifest-src 'self'",
  "base-uri 'none'",
  "form-action 'none'"
].join('; ');

function contentSecurityPolicy(): Plugin {
  return {
    name: 'stencilator:content-security-policy',
    apply: 'build',
    transformIndexHtml: () => [
      {
        tag: 'meta',
        attrs: {
          'http-equiv': 'Content-Security-Policy',
          content: CONTENT_SECURITY_POLICY
        },
        injectTo: 'head-prepend'
      }
    ]
  };
}

function fontLicences(): Plugin {
  return {
    name: 'stencilator:font-licences',
    apply: 'build',
    generateBundle() {
      for (const name of ['OFL-NotoSans.txt', 'OFL-AllertaStencil.txt']) {
        const source = readFileSync(new URL(`../fonts/${name}`, import.meta.url));
        this.emitFile({ type: 'asset', fileName: `licenses/${name}`, source });
      }
    }
  };
}

export default defineConfig({
  // Relative paths, so the built folder works from any address.
  base: './',
  plugins: [svelte(), contentSecurityPolicy(), fontLicences()],
  worker: { format: 'es' },
  build: { target: 'es2022' }
});
