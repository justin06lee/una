import { writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { svelte } from '@sveltejs/vite-plugin-svelte';
import tailwindcss from '@tailwindcss/vite';
import { defineConfig, type Plugin } from 'vite';

const here = dirname(fileURLToPath(import.meta.url));
const outDir = resolve(here, '../src/una_server/web');

/** emptyOutDir wipes the .gitkeep that holds web/ in git; put it back after every build. */
const keepGitkeep: Plugin = {
  name: 'keep-gitkeep',
  closeBundle() {
    writeFileSync(resolve(outDir, '.gitkeep'), '');
  },
};

export default defineConfig({
  plugins: [svelte(), tailwindcss(), keepGitkeep],
  base: '/',
  build: {
    outDir,
    emptyOutDir: true,
  },
  server: {
    // Point at a real server while hacking on the UI:
    //   UNA_SERVER=http://tenet:8100 bun run dev
    proxy: {
      '/v1': process.env.UNA_SERVER || 'http://localhost:8100',
    },
  },
});
