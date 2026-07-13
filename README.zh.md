# Loom

> 项目级工具链管理器，支持 Node.js 和 Python。类似 [mise]，但是**项目作用域** + **动态 shim**（单二进制，无中间包装）+ **零版本管理器依赖**。

Loom 是一个单文件二进制, 它管理 Node 和 Python 的项目级工具链，通过 **NTFS 硬链接**生成 PATH shim（零额外开销），永远不污染你的全局环境。

**不需要任何版本管理器**。loom 直接调用你配置在 `loom.toml` 里的 `node.exe` 和 `python.exe`。不需要 mise、nvm、fnm、uv、pyenv。

[mise]: https://mise.jdx.dev/

## 为什么用 Loom

- **不污染全局**。包都装到 `C:\Loom\<lang>app\`，不进系统目录。
- **不依赖版本管理器**。直接指 node.exe / python.exe。
- **项目级版本锁定**。改 `node.path` 就能换解释器。
- **单文件二进制**。~660 KB 静态链接 exe，零运行时依赖。
- **动态 shim**。`shim add codex` 会把 `codex.exe` 做成 `loom.exe` 的硬链接——每个工具一个 wrapper 脚本？不需要，代理注入？不需要，零维护。
- **跨平台**。同一份代码在 Windows / macOS / Linux 都能跑。

## 工作原理（调用链）

```
              PATH
               │
   ┌───────────┴────────────┐
   │                        │
   ▼                        ▼
 loom.exe            codex.exe   (loom.exe 的硬链接，
   │                     │         NTFS，0 字节额外)
   │ 普通 CLI 模式        │ shim 模式：
   │                     │ current_exe().parent() = nodeapp\shims
   ▼                     │ name = "codex"
  parse clap             ▼
  subcommand         NodeRuntime::run("codex", args)
                          │
                          ▼
                  <node.path>\..\npx <bin> %*
```

loom 永远直接调 `<node.path>` / `<python.path>` 指向的可执行文件。中间没有 `mise exec`，没有 `uv run`，没有版本管理器。

## 快速上手

```powershell
# 把 loom 和 shims 加到 PATH（一次性）
$env:Path = "C:\Loom\loom\target\release;C:\Loom\nodeapp\shims;C:\Loom\pythonapp\shims;$env:Path"

# 首次：初始化配置。loom 自动探测 PATH 上的 node.exe / python.exe
# （自动跳过任何版本管理器的 shim 目录）
loom config init

# 查看解析后的配置
loom info

# 装 Node 包（用 <node.path>\npm.cmd）
loom node install @biomejs/biome @openai/codex

# 装 Python 包（用 <python.path> 创建 venv + pip install）
loom python install requests flask

# 生成 shim 让二进制进 PATH
loom node shim add codex
loom node shim add biome
loom python shim add black

# 像真二进制一样用
codex --help
biome --version
black --version
```

## 命令列表

```
loom <lang> <command> [args]

loom node install <pkg>...
loom node uninstall <pkg>...
loom node list                       # 列出已装的二进制
loom node status                     # 显示可更新的包
loom node upgrade <pkg>...           # 自动补 @latest
loom node rebuild                    # 切 node 路径后重建 native modules
loom node shim add <name>            # 把 loom.exe 硬链接为 <name>.exe
loom node shim remove <name>
loom node shim list

loom python install <pkg>...         # 首次自动创建 .venv
loom python uninstall <pkg>...
loom python list
loom python status
loom python upgrade <pkg>... --force
loom python rebuild                  # 切 python 路径后重装所有包
loom python shim add <name>
loom python shim remove <name>
loom python shim list

loom config init                     # 生成 loom.toml，自动探测 node/python
loom config show                     # 显示生效的配置
loom config get <key>                # 例：loom config get node.path
loom config set <key> <value>        # 例：loom config set node.path C:\node-22\node.exe
loom config set <key> null           # 清空可选字段
loom config set <key> <value> -y     # 跳过切路径确认
loom config path                     # 打印 loom.toml 路径

loom info                            # 显示解析后的路径和配置
```

全局参数：`--config <path>`、`--dry-run`。

## 配置文件

`loom.toml` 放在 **`loom.exe` 旁边**——安装是自包含的。把二进制和 .toml 一起搬到任何目录，root 自动跟着走。

**所有可选字段默认注释掉**。loom 把自动探测到的值作为"参考注释"留下来，用户取消注释编辑即可。也可以用 `loom config set <key> <value>` 设置。

```toml
# Loom configuration
# 取消注释并编辑可以覆盖默认值。保持注释（或不写）就用默认行为。

# root = "C:\Loom"
# proxy_url = "http://127.0.0.1:7897"

[node]
# path = "C:\node-v22\node.exe"
project_dir = "nodeapp"
shims_dir = "shims"

[python]
# path = "C:\Python314\python.exe"
project_dir = "pythonapp"
shims_dir = "shims"
venv = ".venv"
```

上面的 toml 对应的 `info` 输出：

```
Loom configuration

  root           (from C:\Loom fallback)
  proxy          (from environment)

  node
    path         <unset — using PATH>
    project      nodeapp
    shims        shims

  python
    path         <unset — using PATH>
    project      pythonapp
    shims        shims
```

`path` 不设时，loom 运行时从 PATH 找 `node` / `python`。`proxy_url` 不设时，loom 不碰代理环境变量，调用 shell 里的 `HTTP_PROXY` / `HTTPS_PROXY`（这是所有 CLI 工具的标准行为）。`root` 字段显示来源而不是绝对路径，让同一份配置可以跨机器共享，不暴露主机特定路径。

## shim 怎么工作

每个语言有自己的 shim 子目录：

- `<root>/<node.project_dir>/<node.shims_dir>/` — Node shim（如 `biome.exe`）
- `<root>/<python.project_dir>/<python.shims_dir>/` — Python shim（如 `black.exe`）

shim 启动（是 loom.exe 的硬链接）后，loom 检查 `current_exe().parent()` 确定属于哪个 runtime。PATH 上需要加两个目录：

```powershell
$env:Path = "C:\Loom\nodeapp\shims;C:\Loom\pythonapp\shims;$env:Path"
```

### Root 解析顺序

loom 按以下顺序选 root（第一个匹配生效）：

1. **`loom.toml` 里的 `root` 字段** — 显式锁定。用 `loom config set root <path>` 设置，用 `loom config set root null` 清空。
2. **`$LOOM_DIR`** — 运行时覆盖。路径不存在则警告并穿透。
3. **`loom.exe` 所在目录** — **默认**。loom.exe 永远存在（必须存在，因为正在运行），所以这一层永远会成功。把 `loom.exe`（带它的 `loom.toml`）搬到任何地方，安装自动跟着。

如果 `$LOOM_DIR` 设置了但路径不存在，loom 打印警告并穿透到下一级。

### 切换运行时解释器

```bash
loom config set node.path C:\node-v24\node.exe
# ⚠ About to switch runtime interpreter — this is a BREAKING change.
#   node.path C:\node-v22\node.exe → C:\node-v24\node.exe
#   After this change, run `loom node rebuild` to recompile
#   native modules (.node files) against the new V8 ABI.
# Continue? [y/N] y
loom node rebuild              # 切 node 后重建 native modules
```

```bash
loom config set python.path C:\Python312\python.exe -y
loom python rebuild            # 切 python 后重装所有包
```

切 `node.path` 或 `python.path` 总会弹确认（除非加 `-y` / `--yes`）。

### 改项目目录名

`project_dir` 就是个文件夹名——想叫什么都行：

```bash
loom config set node.project_dir frontend
loom config set python.project_dir data-tools
loom info                      # 显示新路径
```

`install`、`shim`、`rebuild` 等都会用新路径。旧 `nodeapp\shims/` 下的 shim 不受影响（它们就是 loom.exe 的硬链接）—— 想挪到新位置就在新路径下重新 `loom node shim add <name>`。

### 关于 `null`

`path` 和 `venv` 是可选字段。用 `loom config set <key> null` 清空它们：

- `set node.path null` → loom 退回 PATH 上的 `node`
- `set python.path null` → loom 退回 PATH 上的 `python`
- `set python.venv null` → 用 `pythonapp/.venv`（默认值）

## 架构

```
                loom.exe  (单文件静态二进制，~660 KB)
                          │
              ┌───────────┴───────────┐
              │                       │
       current_exe() parent      clap 子命令
       是 nodeapp/shims 或         (node / python /
       pythonapp/shims?           config / info)
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

## 从老的 PowerShell 脚本迁移

| 旧（`nodeapp.ps1`）                  | 新（`loom`）                          |
| ----------------------------------- | --------------------------------------- |
| `nodeapp install foo`               | `loom node install foo`               |
| `nodeapp uninstall foo`             | `loom node uninstall foo`             |
| `nodeapp list`                      | `loom node list`                      |
| `nodeapp status`                    | `loom node status`                    |
| `nodeapp upgrade foo`               | `loom node upgrade foo`               |
| `nodeapp shim add foo`              | `loom node shim add foo`              |
| `nodeapp shim remove foo`           | `loom node shim remove foo`           |
| *(没有对应)*                          | `loom python install foo`             |
| *(没有对应)*                          | `loom python shim add foo`            |
| *(没有对应)*                          | `loom config get / set / init`        |
| *(没有对应)*                          | `loom node rebuild`                   |

**注意 shim 格式变了**：老的 `nodeapp.ps1 shim add` 生成 .cmd wrapper，能在传 `--proxy` 时注入 `HTTP_PROXY`/`HTTPS_PROXY`。loom 的新 shim 是直接硬链接到 `loom.exe` 的，没这个 wrapper。如果你需要老的行为，从老脚本重新生成 shim，或者在 shell 里手动设环境变量再调 shim。

## 切版本后依赖能不能用？

| Runtime | 切解释器后会发生什么                                | 怎么修 |
| ------- | ---------------------------------------------- | ---- |
| **Node**  | 纯 JS 包没事；native modules（sqlite3、bcrypt 这种）报错或崩溃 | `loom node rebuild`（跑 `npm rebuild`，重编 .node 文件） |
| **Python** | venv 里的 wheel 全废了（绑定到 Python ABI）              | `loom python rebuild`（freeze 所有包 + `pip install --force-reinstall`） |

Python 比 Node 痛的原因是 venv 绑定解释器；Node 的 `node_modules` 大部分跨版本通用。

## 从源码编译

```bash
cargo build --release
# 产物：target/release/loom.exe  (~660 KB)
```

要求 Rust 1.75+。

## Roadmap

- [x] **动态 shim** — 单个 `loom.exe` 通过 argv[0] / `current_exe()` 分发所有命令，无 wrapper。
- [x] **config init / get / set** — 不离开 CLI 就能管理 loom.toml。
- [x] **node rebuild / python rebuild** — 切解释器后重建 native modules。
- [x] **LOOM_DIR** — 显式覆盖 root。
- [x] **切路径确认机制** — `config set <lang>.path` 弹警告 + y/N 确认。
- [x] **零版本管理器依赖** — `node.path` / `python.path` 直接指向解释器可执行文件。
- [ ] **代理自动注入** — `--proxy <url>` per-invocation 覆盖。
- [ ] **Rust runtime** — 第三个 `runtime/*.rs` 模块，基于 `rustup` + `cargo`，跟 Node / Python 同结构。
- [ ] **`loom doctor`** — 自检：node/python 在 PATH 吗？shim 完整吗？config 有效吗？

## 许可

MIT
