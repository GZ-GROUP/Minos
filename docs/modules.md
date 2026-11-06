# Minos modules (API 1)

A module is an app that lives in its own GitHub repository and is offered in the Minos Marketplace. Start from the working example in [`examples/minos-module-hello`](../examples/minos-module-hello).

> **Current status:** Minos validates and downloads modules, but it doesn't install or run them yet. A downloaded package stays on the device, unopened, until the install step ships. API 1 can still change until then, in particular the backend protocol.

## Repository layout

```
minos-module.json    the manifest, at the root
ui/                  your interface, already built
backend/             optional source of your backend, in any language
```

## The manifest

```json
{
  "id": "hello",
  "version": "1.0.0",
  "minosApi": 1,
  "name": { "en": "Hello", "es": "Hola" },
  "description": { "en": "What it does", "es": "Qué hace" },
  "author": "Your name",
  "category": "utilities",
  "platforms": ["windows", "macos", "linux"],
  "entry": "ui/index.html",
  "icon": "ui/icon.svg",
  "backend": {
    "exec": {
      "windows": "bin/hello-backend.exe",
      "macos": "bin/hello-backend",
      "linux": "bin/hello-backend"
    }
  }
}
```

| Field | Rules |
| --- | --- |
| `id` | 2 to 40 characters: lowercase letters and digits, separated by single hyphens. Can't be a reserved Windows name or the id of an app included in Minos. |
| `version` | A [semantic version](https://semver.org/), such as `1.0.0`. |
| `minosApi` | Must be `1`. |
| `name` | Up to 60 characters per language. `en` is required, and is used for languages you don't provide. |
| `description` | Up to 300 characters per language. `en` is required. |
| `author` | Up to 80 characters. |
| `category` | `documents`, `files`, `system`, `network` or `utilities`. |
| `platforms` | The systems the module works on: `windows`, `macos`, `linux`. Minos doesn't let anyone download a module on a system that isn't listed. |
| `entry` | Path to an `.html` file in the package. |
| `icon` | Path to an `.svg` or `.png` file in the package. |
| `backend` | Optional. `exec` needs exactly one path for each system in `platforms`. |

Paths are relative, use forward slashes, and can't contain `..`. Unknown fields are rejected, so a typo doesn't go unnoticed.

## Publishing

Create a GitHub Release for each version:

- The tag must be `v` plus the manifest's version: `v1.0.0`.
- Attach a package named `<id>-<version>-<os>.zip` for each system (`hello-1.0.0-windows.zip`), or a single `<id>-<version>.zip` if the same package works everywhere.

The example's [workflow](../examples/minos-module-hello/.github/workflows/release.yml) builds and attaches these packages when you push a tag.

### Package rules

- `minos-module.json` must be at the root of the `.zip`, not inside a folder, and must match the release's id and version.
- `entry`, `icon` and the `exec` for that system must be in the package.
- No symbolic links, unsafe paths or duplicate paths (paths that differ only in case count as duplicates).
- Limits: 512 MB compressed, 2 GB unpacked, 20,000 entries, 64 KB for the manifest.

## Testing your module

In Minos, open the menu on the logo, choose **Marketplace → Add**, and either choose your `.zip` or paste your repository link. If something is wrong, Minos shows the exact cause.

A link must look like `https://github.com/owner/repository`. Links to a page inside a repository (such as `/tree/main`) aren't accepted.

## Listing it in the Marketplace

Open a pull request that adds your module to [`marketplace/catalog.json`](../marketplace/catalog.json):

```json
{
  "id": "hello",
  "repo": "https://github.com/you/minos-module-hello",
  "name": { "en": "Hello", "es": "Hola" },
  "description": { "en": "What it does", "es": "Qué hace" },
  "author": "Your name",
  "category": "utilities",
  "platforms": ["windows", "macos", "linux"]
}
```

These fields follow the same rules as the manifest. CI rejects the pull request if an entry isn't valid. The catalog only lists a module: its manifest and package are checked again when someone downloads it.

## Backend protocol

A backend is an executable that Minos will start as a child process, without a shell. It exchanges one JSON object per line over stdin and stdout, and logs to stderr.

```jsonc
// Request, from Minos
{ "id": 1, "method": "greet", "params": { "name": "Minos" } }

// Response, success
{ "id": 1, "result": { "greeting": "Hello, Minos!" } }

// Response, failure
{ "id": 1, "error": { "code": "unknown_method", "message": "unknown method \"nope\"" } }

// Event, for streaming data (has no id)
{ "event": "line", "data": "..." }
```

Keep running until stdin closes, and don't stop on a bad request: answer it with an `error`.

## Security

Installing a module will mean running someone else's code. Its interface will be confined to a restricted window, but a backend can do whatever the user can, so only install modules from authors you trust. Minos can't sandbox a backend.
