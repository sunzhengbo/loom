# Loom

> Project-local toolchain manager for Node.js and Python. Like [mise], but project-scoped, with **dynamic shims** (one binary, no wrappers) and **zero version-manager dependency**.

Loom is a single-binary. It manages per-project toolchains for Node and
Python, generates PATH shims via **NTFS hard links** (zero overhead), and never
touches your global environment.

**No version manager required.** loom calls `node.exe` and `python.exe` directly via
the path you configure in `loom.toml`. No mise, no nvm, no fnm, no uv, no pyenv.

[mise]: https://mise.jdx.dev/

## Why

- **No global pollution.** Packages install into `C:\Loom\<lang>app\`, not your system.
- **No version-manager coupling.** Just point loom at your `node.exe` / `python.exe`.
- **Version-locked per project.** Change `node.path` in `loom.toml` to switch.
- **Single binary.** ~660 KB statically-linked exe, no runtime dependencies.
- **Dynamic shims.** `shim add codex` makes `codex.exe` a hard link to `loom.exe` —
  no per-tool wrapper scripts, no proxy-injection shims, no maintenance.
- **Cross-platform.** Same code on Windows, macOS, Linux.

## How it works (the call chain)

```
              PATH
               │
   ┌───────────┴────────────┐
   │                        │
   ▼                        ▼
 loom.exe            codex.exe   (hard link to loom.exe,
   │                     │         NTFS, 0 bytes extra)
   │ normal CLI          │ shim mode:
   │                     │ current_exe() parent = nodeapp\shims
   ▼                     │ name = "codex"
  parse clap             ▼
  subcommand         NodeRuntime::run("codex", args)
                          │
                          ▼
                  <node.path>\..\npx <node.path>\..\..\node_modules\.bin\codex %*
```

`loom` always calls the binaries at `<node.path>` / `<python.path>` directly.
No `mise exec`, no `uv run`, no version manager sitting in the middle.

## Quick start

```powershell
# Add loom root to PATH (one-time) — shims are hard links to loom.exe,
# so the same single entry covers both `loom` itself and all installed
# binaries like `opencode`, `black`, `http`, `idna`, ...
$env:Path = "C:\Loom\loom;$env:Path"

# First time: init the config. loom auto-detects node.exe / python.exe
# on your PATH (skipping any version-manager shims).
loom config init

# Show the resolved config
loom info

# Install Node packages (uses <node.path>\npm.cmd)
loom node install @biomejs/biome @openai/codex

# Install Python packages (uses <python.path> to create .venv, then pip)
loom python install requests flask

# Generate shims so binaries are on PATH
loom node shim add codex
loom node shim add biome
loom python shim add black

# Use them — they look and feel like real binaries
codex --help
biome --version
black --version
```

## Commands

```
loom <lang> <command> [args]

loom node install <pkg>...
loom node uninstall <pkg>...
loom node list                       # List installed binaries
loom node status                     # Show outdated packages
loom node upgrade <pkg>...           # @latest appended automatically
loom node rebuild                    # Rebuild native modules (after Node path change)
loom node shim add <name>            # Hard-link loom.exe as <name>.exe
loom node shim remove <name>
loom node shim list

loom python install <pkg>...         # Auto-creates .venv on first use
loom python uninstall <pkg>...
loom python list
loom python status
loom python upgrade <pkg>... --force
loom python rebuild                  # Reinstall all packages (after Python path change)
loom python shim add <name>
loom python shim remove <name>
loom python shim list

loom config init                     # Generate loom.toml, auto-detect node/python
loom config show                     # Show effective configuration
loom config get <key>                # e.g. loom config get node.path
loom config set <key> <value>        # e.g. loom config set node.path C:\node-22\node.exe
loom config set <key> null           # Clear optional fields (node.path, python.path, python.venv)
loom config set <key> <value> -y     # Skip path-change confirmation
loom config path                     # Print loom.toml path

loom info                            # Show resolved paths and config
```

Global flags: `--config <path>`, `--dry-run`.

## Configuration

`loom.toml` lives **next to `loom.exe`** — the install is self-contained.
Move the binary + the .toml to a new directory and the root follows automatically.

Every override-able field is **commented out by default**. loom writes the
auto-detected values as commented hints so you can uncomment and edit. To set
a value, either uncomment the line or use `loom config set <key> <value>`.

```toml
# Loom configuration
# Uncomment any line below and edit to override the default. Anything
# left commented (or absent) keeps the default behavior.

# root = "C:\Loom"
# proxy_url = "http://127.0.0.1:7897"

[node]
# path = "C:\node-v22\node.exe"
project_dir = "nodeapp"

[python]
# path = "C:\Python314\python.exe"
project_dir = "pythonapp"
venv = ".venv"

[shims]
# Empty (default) means shims live in <root>/ alongside loom.exe —
# just add <root>/ to PATH once and everything works. Set to a
# non-empty value (e.g. "shims") to keep shims in their own dir.
dir = ""
```

`info` output for the file above:

```
Loom configuration

  root           (from C:\Loom fallback)
  proxy          (from environment)

  node
    path         <unset — using PATH>
    project      nodeapp

  python
    path         <unset — using PATH>
    project      pythonapp

  shims
    dir          <loom root>
    resolved     C:\Loom
```

When `path` is unset, loom walks `PATH` to find `node` / `python` at
runtime. When `proxy_url` is unset, loom doesn't touch proxy env vars —
the calling shell's `HTTP_PROXY` / `HTTPS_PROXY` are used (the standard
behavior of every CLI tool). The `root` field shows where it came from
rather than the absolute path, so the same config can be shared across
machines without revealing host-specific paths.

## How shims work

All shims are hard links to `loom.exe` itself, sitting in the same
directory. By default (when `shims.dir` is empty in `loom.toml`)
they land in `<root>/` next to `loom.exe`:

```
C:\Loom\loom\
├── loom.exe          ← the manager
├── loom.toml
├── nodeapp\
├── pythonapp\
├── tools\
├── opencode.exe        ← shim (hard link to loom.exe)
├── black.exe           ← shim (hard link to loom.exe)
├── http.exe            ← shim (hard link to loom.exe)
└── ...
```

When a shim runs, loom inspects `current_exe()` — the filename
tells it whether this is the manager (`loom.exe`) or a shim
(everything else). For a shim, loom figures out the runtime by
looking for the binary in each runtime's bin directory
(`node_modules/.bin/` first, then `.venv/Scripts/`) and dispatching
accordingly.

One PATH entry covers both the manager and every shim:

```powershell
$env:Path = "C:\Loom\loom;$env:Path"
```

NTFS hard links share the same on-disk data, so when you upgrade
`loom.exe` the shims automatically pick up the new binary — no
need to recreate them.

### Root resolution order

loom picks the project root in this order (first match wins):

1. **`root` field in `loom.toml`** — explicit lock. Set with
   `loom config set root <path>`. Clear with `loom config set root null`.
2. **`$LOOM_DIR`** — runtime override. If the path doesn't exist,
   loom prints a warning and falls through.
3. **`loom.exe`'s directory** — the default. loom.exe is always
   there (it must be, since we're running), so this branch always
   succeeds. Move `loom.exe` (with its `loom.toml`) anywhere and the
   install follows.

### Switching the runtime interpreter

```bash
loom config set node.path C:\node-v24\node.exe
# ⚠ About to switch runtime interpreter — this is a BREAKING change.
#   node.path C:\node-v22\node.exe → C:\node-v24\node.exe
#   After this change, run `loom node rebuild` to recompile
#   native modules (.node files) against the new V8 ABI.
# Continue? [y/N] y
loom node rebuild              # recompile native modules against new Node
```

```bash
loom config set python.path C:\Python312\python.exe -y
loom python rebuild            # reinstall all packages against new interpreter
```

Switching `node.path` or `python.path` always prompts for confirmation
(unless you pass `-y` / `--yes`).

### Renaming project directories

`project_dir` is just a folder name — change it to whatever you like:

```bash
loom config set node.project_dir frontend
loom config set python.project_dir data-tools
loom info                      # shows new paths
```

loom will use the new paths for `install`, `shim`, `rebuild`, etc.
Existing shims in the old `shims/` subdirectory are unaffected
(they're just hard links to loom.exe) — move them into `<root>/`
and loom will pick them up on the next invocation.

### What "rebuild" does

| Language | Mechanism | Why |
|---|---|---|
| Node  | `npm rebuild`               | Recompiles `.node` native addons (sqlite3, bcrypt, etc.) against the new V8 ABI. Pure-JS packages are unaffected. |
| Python | `python -m pip install --force-reinstall <frozen list>` | Wheels are tied to the Python ABI. Cleanest path: re-resolve all wheels against the new interpreter. venv path stays the same, contents are replaced. |

## Architecture

```
                    loom.exe  (single static binary, ~660 KB)
                          │
              ┌───────────┴───────────┐
              │                       │
       current_exe() parent      clap subcommand
        is <root>/shims?            (node / python /
               │ yes                config / info)
              │ yes                   │
              ▼                       ▼
         shim 模式              普通模式
              │                       │
              ▼                       ▼
       runtime::run()          install / uninstall /
              │                list / status / upgrade
              │                / rebuild / shim
              │                / config
              ▼
     <node.path>\..\npx       <node.path>\..\npm install
     <bin>                      <pkg>...
                          ─── 或 ───
                              <python.path> -m venv .venv
                              .venv\Scripts\python -m pip install
                              <pkg>...
```

## Migration from the old PowerShell scripts

| Old (`nodeapp.ps1`)             | New (`loom`)                          |
| ------------------------------- | --------------------------------------- |
| `nodeapp install foo`           | `loom node install foo`               |
| `nodeapp uninstall foo`         | `loom node uninstall foo`             |
| `nodeapp list`                  | `loom node list`                      |
| `nodeapp status`                | `loom node status`                    |
| `nodeapp upgrade foo`           | `loom node upgrade foo`               |
| `nodeapp shim add foo`          | `loom node shim add foo`              |
| `nodeapp shim remove foo`       | `loom node shim remove foo`           |
| *(no equivalent)*               | `loom python install foo`             |
| *(no equivalent)*               | `loom python shim add foo`            |
| *(no equivalent)*               | `loom config get / set / init`        |
| *(no equivalent)*               | `loom node rebuild`                   |

**Heads-up on shim format change:** the old `nodeapp.ps1 shim add` produced a
`.cmd` wrapper that injected `HTTP_PROXY`/`HTTPS_PROXY` when `--proxy` was
passed. Loom's new shims are direct hard links to `loom.exe` with no
such wrapper. If you need the old behavior, re-create the shim from the
legacy script, or set the env vars manually in your shell before invoking
the shim.

## Building from source

```bash
cargo build --release
# Output: target/release/loom.exe  (~660 KB)
```

Requires Rust 1.75+.

## Roadmap

- [x] **Dynamic shims** — single `loom.exe` dispatches all commands via
      argv[0] / `current_exe()`; no per-binary wrappers.
- [x] **config init / get / set** — manage loom.toml without leaving the CLI.
- [x] **node rebuild / python rebuild** — re-resolve native modules after
      an interpreter change.
- [x] **LOOM_DIR** — explicit root override.
- [x] **Path-change confirmation** — `config set <lang>.path` warns about
      dependency rebuilds and asks for confirmation.
- [x] **Zero version-manager dependency** — `node.path` / `python.path`
      point directly at the interpreter executable.
- [ ] **Configurable proxy** — `--proxy <url>` per-invocation override.
- [ ] **Rust runtime** — third `runtime/*.rs` module on top of `rustup` +
      `cargo`, mirroring the Node / Python shape.
- [ ] **`loom doctor`** — self-check: node/python on PATH? shims intact?
      config valid?

## License

MIT
