# Release plan and status

| Version | Scope | Status |
|---|---|---|
| v0 | Rust workspace, shared model, config store, loopback daemon with token + lock, CLI skeleton | done |
| v1 | git driver via `git2` (no CLI), multi-remote/fork checkouts, polling watcher, HTTP/WS API, CLI, Leptos UI with Rust/UI primitives | done |
| v2 | RCS driver with its own config shape (`file` + `rcsDir`), in-house `,v` reader | done |
| v3 | Provider registry: GitHub pull requests / workflow runs, GitLab merge requests / pipelines, per-remote opt-in, token from env | done |
| v4 | Runtime plugin registry for VCS drivers, integrations and ingest systems; built-in `plain-file` driver and `git-ingest`; background/tray settings; plugin-aware config and UI | done |
| v5 | One-field quick import with URL inference and preview, settings-driven folders/intervals, export/import bundle for remote-backed sources | done |
| v6 | Project notes and tags, dashboard search and tag filters, project detail panel | done |
| v7 | Type-first add wizard, per-type/per-tag source defaults, plugin-driven integration cards and PR checkout | done |
| v8 | Separate Projects page from Sources config, CLI-backed VCS plugins (Mercurial, Subversion, Jujutsu), GitHub/GitLab/Codeberg forge integrations | done |
| v9 | Default config populated from all Git projects under `~/code`; run commands and managed sources documented | done |
| v10 | Tauri desktop application with tray, wrapped daemon, real icons and a Nix package/app | done |

## Constant across versions

- One YAML config with a tagged `VcsConfig` enum: each VCS owns its fields.
- Endpoints: `GET /state`, `POST /command`, `GET /events`, `GET /providers`, `GET /plugins`, `GET /actions`, `GET /kinds`.
- One shared Rust model compiled to WASM for Leptos.
- No backwards-compatibility or migration code.

## Deliberately skipped

- Native Rust hg/svn/cvs/perforce drivers: hg, svn and jujutsu use CLI plugins instead; cvs and perforce are not supported.
- Dynamic form schema: typed Leptos forms cover compiled-in drivers; plugin options are passed through as JSON.
- Loading plugin code from disk (dylib/WASM): v4 registers plugins in-process via a trait-object registry; external loading is a later step.
- OS keychain: env vars for provider tokens in v3.
