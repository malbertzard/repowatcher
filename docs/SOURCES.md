# Managed sources

The default config at `~/.config/rw/config.yaml` lists every Git repository found under `~/code`.
Root folder: `/home/malbertzard/code`. Total sources: **36**.

Each source uses the in-process Git driver. Remotes and branches were read from the repositories when the config was generated; nothing was cloned or moved.

| ID | Path | Branch | Remote |
|---|---|---|---|
| `Chronurgy` | `proj/Chronurgy` | `main` | `https://github.com/malbertzard/Chronurgy.git` |
| `Eddy` | `proj/Eddy` | `main` | `git@github.com:malbertzard/Eddy.git` |
| `FrankenFast` | `proj/FrankenFast` | `main` | `https://github.com/malbertzard/FrankenFast.git` |
| `Locus` | `proj/Locus` | `main` | `https://github.com/malbertzard/Locus.git` |
| `Malleus` | `proj/Malleus` | `main` | `git@github.com:malbertzard/Malleus.git` |
| `TGE.el` | `proj/TGE.el` | `main` | `https://github.com/malbertzard/TGE.el.git` |
| `chronomancer` | `proj/chronomancer` | `main` | `https://github.com/malbertzard/chronomancer.git` |
| `deltadays` | `proj/deltadays` | `master` | `https://github.com/malbertzard/deltadays` |
| `dotfiles` | `proj/dotfiles` | `dev` | `https://github.com/malbertzard/dotfiles.git` |
| `flag.pl` | `proj/flag.pl` | `main` | `—` |
| `inline-fold.nvim` | `proj/inline-fold.nvim` | `feat/extmarks-and-improvements` | `git@github.com:malbertzard/inline-fold.nvim.git` |
| `lem` | `proj/lem` | `main` | `https://github.com/malbertzard/lem` |
| `malb_dotfiles` | `proj/malb_dotfiles` | `main` | `—` |
| `malbertzard.github.io` | `proj/malbertzard.github.io` | `rewrite` | `https://github.com/malbertzard/malbertzard.github.io` |
| `obsidian-to-ical-plugin` | `proj/obsidian-to-ical-plugin` | `feat/timezone` | `https://github.com/malbertzard/obsidian-to-ical-plugin` |
| `olddog` | `proj/olddog` | `main` | `https://github.com/malbertzard/olddog.git` |
| `pidstop` | `proj/pidstop` | `main` | `https://github.com/malbertzard/pidstop.git` |
| `poller.el` | `proj/poller.el` | `main` | `https://github.com/malbertzard/poller.el.git` |
| `quire` | `proj/quire` | `main` | `git@github.com:malbertzard/modulus.git` |
| `repowatcher` | `proj/repowatcher` | `main` | `git@github.com:malbertzard/repowatcher.git` |
| `rusty_brain` | `proj/rusty_brain` | `master` | `https://github.com/malbertzard/rusty_brain.git` |
| `scheduler.el` | `proj/scheduler.el` | `main` | `https://github.com/malbertzard/scheduler.el.git` |
| `taiga_cli` | `proj/taiga_cli` | `dev` | `https://github.com/malbertzard/taiga_cli` |
| `FlipperAmiibo` | `third-party/FlipperAmiibo` | `main` | `https://github.com/Gioman101/FlipperAmiibo` |
| `activities.el` | `third-party/activities.el` | `master` | `https://github.com/malbertzard/activities.el.git` |
| `devdocs` | `third-party/devdocs` | `main` | `https://github.com/malbertzard/devdocs.git` |
| `eldoc-box` | `third-party/eldoc-box` | `master` | `https://github.com/casouri/eldoc-box` |
| `godot-demo-projects` | `third-party/godot-demo-projects` | `master` | `https://github.com/godotengine/godot-demo-projects.git` |
| `gridfinity-rebuilt-openscad` | `third-party/gridfinity-rebuilt-openscad` | `main` | `https://github.com/kennetek/gridfinity-rebuilt-openscad` |
| `jj` | `third-party/jj` | `main` | `https://github.com/malbertzard/jj.git` |
| `lem-2` | `third-party/lem` | `main` | `https://github.com/lem-project/lem` |
| `obsidian-day-planner` | `third-party/obsidian-day-planner` | `main` | `https://github.com/malbertzard/obsidian-day-planner.git` |
| `sev` | `third-party/sev` | `main` | `https://github.com/dylancobb/sev.git` |
| `taiga-docker` | `third-party/taiga-docker` | `main` | `https://github.com/taigaio/taiga-docker.git` |
| `tetris` | `third-party/tetris` | `main` | `https://github.com/Lordloss90/OpenPuzzleGame` |
| `goth` | `try/goth` | `main` | `—` |

`lem-2` is `third-party/lem`; the first `lem` is `proj/lem`.

## Add another project

- Paste its URL into **Overview → Add a source**, or
- Add a source on the **Sources** page and choose the VCS that manages it.

Non-Git directories under `~/code` are not listed because there is no VCS state to watch.
