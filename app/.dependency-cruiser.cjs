/**
 * The engine boundary: only the worker side touches the WebAssembly build, and the
 * interface reaches the engine only through the client.
 * @type {import('dependency-cruiser').IConfiguration}
 */
module.exports = {
  forbidden: [
    {
      name: 'no-circular',
      severity: 'error',
      from: {},
      to: { circular: true }
    },
    {
      name: 'no-orphans',
      severity: 'error',
      from: {
        orphan: true,
        pathNot: ['\\.d\\.ts$', '\\.test\\.ts$', '(^|/)src/engine/wasm/']
      },
      to: {}
    },
    {
      name: 'wasm-only-in-the-worker',
      comment: 'The WebAssembly build is loaded and called only inside the Web Worker.',
      severity: 'error',
      from: { pathNot: '^src/engine/(worker|handlers)(\\.test)?\\.ts$' },
      to: { path: '^src/engine/wasm/' }
    },
    {
      name: 'interface-uses-the-client',
      comment: 'The interface talks to the engine through src/engine/client.ts only.',
      severity: 'error',
      from: { path: '^src/', pathNot: '^src/engine/' },
      to: { path: '^src/engine/', pathNot: '^src/engine/(client|protocol)\\.ts$' }
    }
  ],
  options: {
    doNotFollow: { path: 'node_modules' },
    exclude: { path: '(^|/)src/engine/wasm/' },
    tsPreCompilationDeps: true,
    tsConfig: { fileName: 'tsconfig.json' },
    enhancedResolveOptions: { extensions: ['.ts', '.js', '.svelte'] }
  }
};
