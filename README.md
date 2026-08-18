# Burp SQLite Viewer

A Tauri 2 desktop and mobile application with a vanilla Svelte 5 + TypeScript frontend, styled with design tokens and
shadcn-svelte components on Tailwind CSS 4.

    npm install
    npm run dev          # frontend alone, in a browser
    npm run tauri dev    # the actual app
    npm run check        # svelte-check + tsc
    npm run build        # frontend production build
    npm run tauri build  # installers for this platform

Mobile targets are initialised per platform and are not set up yet:

    npm run tauri android init
    npm run tauri ios init

## UI components

    npm run ui add button dialog     # adds shadcn-svelte components under src/lib/components/ui

**Never run `shadcn-svelte init`** — it overwrites `src/app.css`, which holds the design
tokens every component depends on. Everything `init` would create is already in place.

shadcn components read their colours from the `@theme inline` bridge at the foot of
`src/app.css`, so retheming is still one edit to the `:root` tokens.

## Layout

    src/app.css              design tokens; the single source of visual truth
    src/lib/ipc.ts           typed wrappers over invoke — one per Rust command
    src/lib/                 components
    src-tauri/src/lib.rs     commands and builder setup, in run()
    src-tauri/capabilities/  ACL grants for plugin and core commands

Components consume tokens and never hard-code colours or sizes; they call Rust through
`src/lib/ipc.ts` and never `invoke` directly.

The placeholder artwork in `src-tauri/icons/` should be replaced before release with
`npm run tauri icon <path>`.
