<p align="center">
  <img src="src/assets/logo.png" alt="Minos logo" width="128" />
</p>

<h1 align="center">Minos</h1>

<p align="center">A modular desktop toolbox, built with Tauri + React.</p>

---

Minos is a desktop app that bundles small everyday tools (PDF conversion, file sharing, and more) behind a single app-drawer interface. Each tool is a self-contained module, so new ones can be added without touching the rest of the app.

## Tools

| Tool | Status |
| --- | --- |
| PDF to Word | Available |
| Word to PDF | Available |
| Crop PDF | Available |
| Merge PDF | Available |
| Share over Network | Available |
| Share over SCP | Available |
| Processes | Available |
| My PC | Available |
| My Network | Available |
| My Disks | Available |
| My Files | Available |
| PingPong! | Available |

PDF ↔ Word conversion uses [LibreOffice](https://www.libreoffice.org/) when it is installed (full formatting); without it Minos falls back to a built-in converter that keeps the text only.

## Adding a tool

Every tool lives in its own folder under `src/apps/` and is discovered automatically:

```
src/apps/my-tool/
  manifest.ts        id, icon and drawer order
  index.tsx          the tool's UI (default export)
  locales/en.json    { "name": "My Tool", "description": "..." }
  locales/es.json
```

Icons come from [Phosphor Icons](https://phosphoricons.com/) and inherit the theme color:

```ts
import { Wrench } from "@phosphor-icons/react";
import type { AppManifest } from "../types";

export default {
  id: "my-tool",
  icon: Wrench,
  order: 7,
} satisfies AppManifest;
```

## Development

Requirements: [Node.js](https://nodejs.org/), [Rust](https://www.rust-lang.org/tools/install) and the [Tauri prerequisites](https://tauri.app/start/prerequisites/).

```bash
npm install
npm run tauri dev     # run the desktop app
npm run tauri build   # build an installer
```

## Stack

Tauri 2 · React 19 · TypeScript · Vite · i18next (English, Spanish) · Phosphor Icons
