<p align="center">
  <img src="src/assets/logo.png" alt="Minos logo" width="128" />
</p>

<h1 align="center">Minos</h1>

<p align="center">A modular desktop toolbox, built with Tauri + React.</p>

---

Minos is a desktop app that bundles small everyday tools (PDF conversion, file sharing, and more) behind a single app-drawer interface. Each tool is a self-contained module, so new ones can be added without touching the rest of the app.

Minos starts empty: open the menu on the logo and choose **Marketplace** to install the tools you want.

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

PDF ↔ Word conversion uses the PDF engine in [`pdf-engine/`](pdf-engine), included in the installers, so it needs nothing else installed. Word → PDF accepts `.docx` files. If the engine isn't available, Minos falls back to a built-in converter that keeps the text only.

To use the engine during development or in a local installer, build it first with `python pdf-engine/build.py`, using a Python environment with `pdf-engine/requirements.in` and `requirements-dev.txt` installed, and [Pango](https://doc.courtbouillon.org/weasyprint/stable/first_steps.html) on the machine (on Windows, on `PATH`). Without the engine, Minos still builds and runs, using the fallback.

## Creating a module

Modules are apps published from their own repository and offered in the Marketplace. Minos can validate and download them today; installing and running them comes next. See the [module format](docs/modules.md) and the [example module](examples/minos-module-hello).

## Adding a tool

Every tool included in Minos lives in its own folder under `src/apps/` and is discovered automatically. Add its id to `INTEGRATED_APP_IDS` in `src-tauri/src/marketplace/manifest.rs` so that it can be installed:

```
src/apps/my-tool/
  manifest.ts        id, icon, category and supported systems
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
  category: "utilities",
  // Optional: leave it out if the tool works on every system.
  platforms: ["windows", "linux"],
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
