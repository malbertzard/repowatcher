# RepoWatcher

A local-first tool that watches code sources in the background and shows their state in a desktop UI. Rust everywhere: one shared model powers the daemon, the CLI, and a Leptos (WASM) frontend. Git runs in-process through libgit2; other VCS drivers use the CLI tools provided by the Nix wrapper.

## Layout

- `crates/rw-model` — config and API types shared with the WASM frontend.
- `crates/rw-core` — VCS drivers, watcher, config store, HTTP/WS daemon, provider clients.
- `crates/rw` — the `rw` CLI and `rw daemon`.
- `ui/` — Leptos CSR app using the Rust/UI components (`leptos_ui` + copied primitives from leptos.rust-ui.com) and Tailwind CSS; built with `ui/build.sh` into `ui/dist`.
- `apps/desktop` — thin Tauri shell that starts the daemon and opens its UI.
- `docs/SOURCES.md` — the projects currently in the default config.

## Nix

The flake builds the daemon/CLI and the UI, and wraps `rw` so every VCS CLI tool a plugin might call is on `PATH` (git, hg, svn, rcs, jj, pijul, ssh). Built-in plugin drivers for Mercurial, Subversion and Jujutsu use those tools; git and RCS run in-process.

```bash
nix build .#rw        # wrapped rw binary with VCS CLI tools on PATH
nix build .#rw-ui     # Tailwind CSS + WASM UI, built entirely in Nix
nix run .#rw -- --help
nix develop           # Rust, trunk, lld, npm and all VCS CLI tools
```

On NixOS, enable the user service:

```nix
{
  services.repowatcher = {
    enable = true;
    configFile = "%h/.config/rw/config.yaml";
    port = 3030;
  };
}
```

The daemon itself uses libgit2 in-process; the CLI tools are guaranteed for plugins and for manual work inside `nix develop`.

## Build and run (without Nix)

```bash
cargo test                         # backend + model tests
cd ui && ./build.sh                # npm run css + trunk build; needs lld
cd .. && cargo run -p rw -- daemon # serves the UI on port 3030
```

Run it:

```bash
rw daemon          # start watching the configured sources
rw url             # print the browser link with the token
```

Open the link printed by `rw url` (or `http://127.0.0.1:3030/` plus the token from `rw url`). The daemon writes a `0600` token next to the config and keeps it out of the HTML; the UI reads it from the URL fragment.

## Desktop application

RepoWatcher ships as a Tauri application. It starts its own `rw` daemon, opens the Leptos UI in a native window, adds a tray icon when `settings.tray` is on, and stops the daemon when the window closes.

With Nix (recommended — the wrapper supplies the webview libraries, the `rw` binary and all VCS CLI tools):

```bash
nix run .#rw-desktop                  # build and launch the app
nix build .#rw-desktop                # build only
./result/bin/repowatcher              # launch the built app
```

Without Nix, use the dev shell for the webkit/dbus libraries and point the shell at the CLI binary:

```bash
nix develop
cargo build -p rw
RW_BIN=$PWD/target/debug/rw cargo run --manifest-path apps/desktop/Cargo.toml
```

`RW_PORT` and `RW_CONFIG` override the port and config file for the shell. Application icons live in `apps/desktop/icons`.

## Config

Default path: `~/.config/rw/config.yaml`. It currently lists the Git projects under `~/code` — 36 sources; see [docs/SOURCES.md](docs/SOURCES.md). Each source carries its own VCS shape:

```yaml
rootFolder: ~/code
editor: code
sources:
  - id: neovim
    label: Neovim + forks
    intervalSecs: 120
    vcs: git
    remotes:
      - name: upstream
        role: upstream
        url: https://github.com/neovim/neovim
        provider:
          kind: github
          repo: neovim/neovim
      - name: mine
        role: fork
        url: git@github.com:me/neovim.git
    checkouts:
      - name: main
        path: neovim
        remote: mine
        branch: main
  - id: hosts
    vcs: rcs
    file: /etc/hosts
    rcsDir: /var/lib/rcs
```

### Background process

`settings` controls the daemon's watching behaviour:

```yaml
settings:
  background: while_window_open   # or always (tray/headless)
  tray: true
  autostart: false
  startMinimized: false
  open:
    command: code
    args: ["{path}"]
```

`open.command` and `open.args` control the **Open** button; `{path}` is replaced with the source path. Add more per-source actions by registering a `SourceActionPlugin`.

`while_window_open` pauses polling when no UI is connected; `always` keeps watching in the background. The Tauri shell reads `tray` and `startMinimized` (autostart is stored but not yet wired to the OS launcher).

### Add wizard and source defaults

The Overview's **Add a source** box takes a location or URL: paste and click **Add source** (or press Enter). Names, folders, branches and providers are inferred from settings. **Preview plan** is always available; the source-type chooser is hidden under a disclosure fold for less common cases (RCS, plugin drivers and ingest systems).

Settings separates **global** options (background process) from advanced defaults and plugin-owned configuration, both under disclosure folds. Per-kind rules such as `type:git` or `tag:work` set fetch interval, auto-fetch and token env; tag rules override type rules. `settings.plugins` is a JSON object owned by plugins, not by the core. Built-in plugins currently ignore it; it exists so plugin authors have a stable place for global options.

### Project page

Project management is available in a side drawer opened from any source card, and on its own **Projects** page, separate from **Sources** (configuration). Pick a project there for notes, tags, registered actions (for example `git-prune`) and three forge integrations: **GitHub**, **GitLab** and **Codeberg**. They list pull requests and offer **Get code** to check out a PR ref through libgit2. New integrations implement `IntegrationPlugin`.

### Projects, notes and tags

Each source has `notes` (free-form todos) and `tags`. The Overview has a search box (id, label, tags, notes) plus tag filter chips; **Open project** opens the project drawer with removable tag chips, suggested tags from other projects, notes that autosave, and registered actions (e.g. `git-prune`). These fields are part of the config and are included in export bundles.

### Quick add (v5)

The Overview has a single URL box. Paste a location or URL and click **Add source** (or **Preview** first). Name, owner, folder, branch and provider are inferred:

```yaml
settings:
  importRoot: imports
  folderTemplate: "{owner}/{repo}"
  defaultIntervalSecs: 120
  hosts:
    github.com: { provider: github, tokenEnv: RW_GITHUB_TOKEN }
```

The generated source is normal, editable config — the UI just stops asking for derived fields.

### Move to another device

Settings → **Move to another device** exports a JSON bundle containing the portable config and only sources that have a remote (`can_export()`), so they can be re-cloned. Local-only sources (RCS, plain files) are excluded. Paste the bundle on the new device and click **Import configuration**.

### Imports

Any source can declare imports handled by a registered ingest system:

```yaml
  - id: importer
    vcs: plugin
    plugin: plain-file
    paths: []
    imports:
      - ingest: git-ingest
        url: https://github.com/owner/project.git
        path: project
        options: { branch: main, label: My project }
```

The Overview's **Import** button runs them: each ingest system fetches the remote into `rootFolder/path` and returns a new source config, which is added to the system. Implement `plugins::IngestPlugin` to add an ingest system for another VCS.

Secrets stay out of this file: provider tokens come from `RW_GITHUB_TOKEN`, `RW_GITLAB_TOKEN` or `RW_CODEBERG_TOKEN`. For HTTPS remotes, set `RW_GIT_USER_<HOST>` and `RW_GIT_TOKEN_<HOST>` (uppercase host, e.g. `RW_GIT_TOKEN_GIT_EXAMPLE_COM`); the generic `RW_GIT_USER`/`RW_GIT_TOKEN` pair works only for GitHub, GitLab and Codeberg.

### Plugins (v4)

A source can use a registered driver instead of a built-in VCS:

```yaml
  - id: files
    vcs: plugin
    plugin: plain-file
    options: {}
    paths:
      - a.txt
      - missing.txt
```

Drivers implement `plugins::VcsPlugin`; integrations implement `plugins::IntegrationPlugin`. Both are registered by name in the in-process registry, and `GET /api/plugins` lists the registered driver names. Loading plugin code from disk is not implemented yet.

## CLI

```bash
rw url
rw list
rw sync <source>
rw diff <source> [unit]
rw open <source>
rw daemon
```

Global flags `--config` and `--port` must come before the subcommand.

## Status

Implemented: v0 foundation, v1 git driver (multiple remotes/forks + checkouts) with libgit2, polling watcher, daemon API, CLI, Leptos dashboard; v2 RCS driver; v3 GitHub/GitLab providers; v4 in-process plugin registry (VCS drivers and integrations) with a built-in `plain-file` driver; v5 one-field quick import and portable export/import bundles; v6 project notes and tags; v7 type-first add wizard and integration cards; v8 separate Projects page, Mercurial/Subversion/Jujutsu CLI drivers and GitHub/GitLab/Codeberg integrations; v9 populated config from `~/code`; v10 Tauri desktop app built and packaged with Nix. Planned: loading plugin code from disk, notifications, more native Rust drivers (pijul). See `docs/PLAN.md`.
