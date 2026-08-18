import { defineConfig } from 'vite'
import { svelte } from '@sveltejs/vite-plugin-svelte'
import tailwindcss from '@tailwindcss/vite'
import path from 'node:path'
import process from 'node:process'

// Set by `tauri android dev` / `tauri ios dev` when targeting a physical device.
// Without it the dev server binds to 127.0.0.1 and the handset cannot reach it.
const host = process.env.TAURI_DEV_HOST

// Svelte emits each component <style> block as a virtual `*.svelte?svelte&type=style`
// module. Tailwind's pre-transform can race Svelte's loader in dev mode and receive the
// component source instead of its CSS. These scoped styles use only ordinary CSS and our
// design tokens, so Tailwind has no work to do on them; app.css remains fully processed.
const tailwindPlugins = tailwindcss()
type TailwindTransformFilter = {
  filter?: { id?: { exclude?: Array<RegExp | string> } }
}
for (const plugin of tailwindPlugins) {
  const transform = plugin.transform as TailwindTransformFilter | undefined
  transform?.filter?.id?.exclude?.push(/\.svelte\?svelte&type=style/)
}

// https://vite.dev/config/
export default defineConfig({
  // Tailwind before svelte: it must see the markup the Svelte plugin passes through.
  plugins: [...tailwindPlugins, svelte()],

  // SvelteKit provides `$lib` for free; a vanilla Vite project has to declare it, in three
  // places that must agree — here, tsconfig.json and tsconfig.app.json. shadcn-svelte
  // generates imports against this alias, so a mismatch breaks every added component.
  resolve: {
    alias: { $lib: path.resolve('./src/lib') },
  },

  // Tauri-specific options. Applied always, but only matter under `tauri dev`/`tauri build`.
  //
  // 1. Do not let Vite clear the screen — it eats Rust compiler errors.
  clearScreen: false,
  // 2. Tauri reads a fixed port from tauri.conf.json > build.devUrl. Fail loudly rather
  //    than silently serving on a port Tauri will not open.
  server: {
    port: 1420,
    strictPort: true,
    host: host || '127.0.0.1',
    hmr: host
      ? {
          protocol: 'ws',
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. src-tauri is Cargo's business; watching it triggers pointless frontend reloads.
      ignored: ['**/src-tauri/**'],
    },
  },
})
