# Hello: an example Minos module

Copy this folder to a new repository to create your own module. The format and its rules are in [`docs/modules.md`](../../docs/modules.md).

> Minos validates and downloads modules today, but it doesn't run them yet, so this module won't open inside Minos for now.

```
minos-module.json              id, version, supported systems…
ui/                            the interface
backend/                       optional backend, written in Rust here
.github/workflows/release.yml  builds the packages when you push a tag
```

## Make it yours

1. Change `id`, `name`, `description`, `author`, `category` and `platforms` in `minos-module.json`.
2. Replace `ui/` with your interface, and `ui/icon.svg` with your icon.
3. Replace the backend, or delete `backend/`, the `backend` field in the manifest, and the build steps in the workflow.

## Try it

Run the backend and type a request:

```sh
cd backend
cargo run
{"id":1,"method":"greet","params":{"name":"Minos"}}
```

Build a package for your system, with the manifest at the root of the `.zip`. On Linux or macOS:

```sh
cargo build --release --manifest-path backend/Cargo.toml
mkdir -p dist/package/bin
cp minos-module.json dist/package/
cp -r ui dist/package/
cp backend/target/release/hello-backend dist/package/bin/
(cd dist/package && zip -r ../hello-1.0.0-linux.zip .)
```

Then load it in Minos: open the menu on the logo and choose **Marketplace → Add → Choose .zip**.

## Publish it

Push a tag that matches the manifest's version. The workflow builds a package per system and attaches them to a GitHub Release:

```sh
git tag v1.0.0
git push origin v1.0.0
```

Your repository link now works in **Marketplace → Add**. To list your module for everyone, see [Listing it in the Marketplace](../../docs/modules.md#listing-it-in-the-marketplace).
