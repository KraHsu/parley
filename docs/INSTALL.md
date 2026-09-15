# 安装 Parley

当前预览版提供 **Ubuntu 24.04 x86_64（amd64）** 的 `.deb` 安装包。Windows、macOS 和其他 Linux 发行版尚未完成原生运行验收，未提供安装包；自动构建结果见[兼容记录](BACKEND_COMPATIBILITY.md)。

## 下载和安装

旧版 `Parley_0.3.0_amd64.deb` 使用了与 Ubuntu KDE Parley 相同的包名，会被 APT 误升级；请勿继续安装旧包。本仓库已修复为 **`parley-desktop`**，当前修复包为 `ParleyDesktop_0.4.0-alpha.1_amd64.deb`（多后端预览版）。

以下适用于首次安装；已经装过旧版时，先按文末的修复流程迁移。

```bash
sha256sum -c SHA256SUMS
sudo apt install ./ParleyDesktop_0.4.0-alpha.1_amd64.deb
```

从 [GitHub Releases](https://github.com/KraHsu/parley/releases) 获取安装包前，请确认文件名为 `ParleyDesktop_…`；尚未发布的版本可按本文末尾步骤构建。

APT 会安装所需的 GTK 3 和 WebKitGTK 4.1 运行库。此构建需要 glibc 2.39 或更新版本，不适用于 Ubuntu 22.04 / Debian 12。安装 Parley 本身不需要 Rust、Node.js 或 npm 开发环境。

## 选择自己的 Codex

Parley 使用你自行安装的官方 Codex CLI，复用本机登录。安装包不包含 Codex，也不包含账号。若 Codex 通过 npm 安装，它仍需要自己的 Node.js 运行环境。

1. 在正常使用 Codex 的终端执行 `codex --version`，确认 CLI 可以运行；当前已验证版本为 **0.154.0**。
2. 执行 `command -v codex`，取得可执行文件的绝对路径。
3. 从应用菜单打开 **Parley**，在设置中填写这个路径，点击“连接本机 Codex”。已登录的本机账号会直接读取；需要登录时再使用登录按钮。

通过 NVM 安装时，路径通常形如 `/home/你的用户名/.nvm/versions/node/v24.x.x/bin/codex`。Parley 为子进程补充所选可执行文件所在目录，使其能找到同目录的 Node.js；切换或删除 Node 版本后，请更新设置中的 Codex 路径。

## 官方终端 + 语法窗口

首次可以直接从终端运行：

```bash
parley-cli --codex "$(command -v codex)"
```

保存过路径后，只需运行 `parley-cli`，也可以从应用菜单打开 **Parley Terminal / Parley 终端伴随**。请先正常关闭已有 Parley 窗口，再启动终端伴随模式。

```bash
parley-cli
parley-cli -- resume --last
```

终端是官方 Codex TUI，保留原有快捷键、登录和历史。旁边的 GUI 自动读取当前会话已保存的消息，可直接解释、翻译，不需要复制文本。恢复旧会话或存在多个候选时，在语法窗口的下拉框选择会话。

应用菜单中的 **Parley Language Tutor / Parley 语法助手** 可以单独打开语法窗口。更多操作见 [终端伴随模式](https://github.com/KraHsu/parley/blob/main/docs/TERMINAL_COMPANION.md)。

## 词句学习

安装后无需连接 Codex 即可添加、检索和复习词句。对话选词、注释、标签及 JSON/CSV 备份操作见[词句学习指南](https://github.com/KraHsu/parley/blob/main/docs/VOCABULARY.md)。

## 升级和卸载

从 v0.1.0 升级前，关闭所有 Parley 窗口并备份工作区数据库。v0.3.0 会在事务中将 schema 1 顺序迁移到 3，保留已有会话；旧版不能读取或写入升级后的数据库。需要回退时使用升级前的备份，不能直接降级数据库。

升级时使用 `sudo apt install ./新版本安装包.deb`。卸载程序：

```bash
sudo apt remove parley-desktop
```

工作区默认保存在 `~/.local/share/org.parley.desktop/`（遵循 `XDG_DATA_HOME`），升级和卸载不会删除个人学习记录。Codex 的登录和历史由 Codex 自己管理。

## 从源码构建安装包

当前主分支版本为 `0.4.0-alpha.1`，包含尚未发布的多后端功能，使用方式见[模型服务设置](BACKENDS.md)。它会将旧工作区升级到 schema 7；运行前备份已有数据，旧版不能直接打开升级后的数据库。需要复现已发布的 v0.3.0 时，请使用对应 release tag，而不是当前主分支。

在配置好开发环境后执行：

```bash
npm ci
npm run package:linux
```

产物及对应 `SHA256SUMS` 位于 `target/release/bundle/deb/`，包含 `parley-desktop`、`parley-cli` 启动命令、应用菜单入口和安装文档。真实程序放在 `/usr/lib/parley-desktop/`，保留 CLI 与 GUI 的相邻路径；图标和文档使用独立名称。应用标识与学习数据目录不变。当前打包依赖下限针对 Ubuntu 24.04 构建环境；更换构建系统时应重新检查动态库依赖。

开发包文件名为 `ParleyDesktop_0.4.0-alpha.1_amd64.deb`；Debian 包内版本使用 `0.4.0~alpha.1`，保证版本顺序为 `0.3.0 < 0.4.0~alpha.1 < 0.4.0`。`npm run package:linux` 会在 Tauri 打包后完成版本转换并生成校验文件。开发包尚未发布，真实厂商与其他平台的验收状态见兼容记录。

## 修复旧包与 KDE Parley 的命名冲突

已确认的错误表现：APT 将我们的 `parley 0.3.0` 升级为 Ubuntu 的 `parley 4:23.08.5-0ubuntu3`，`parley-data` 又因同名图标文件覆盖失败，留下未完成的事务。这与 VS Code、Docker 或 ROS 等第三方源无关。

新包名为 `parley-desktop`，不再安装 `/usr/bin/parley` 或 `parley.png`；可以与 Ubuntu 的 KDE Parley 共存。仅针对旧版 `parley (< 0.5.0)` 声明 Conflicts/Replaces，配合修复脚本迁移旧包，不替代或提供 KDE 软件。不要直接在旧版 0.x 上运行普通安装命令：APT 可能通过升级到 KDE 来满足版本冲突条件，再次触发旧图标冲突。[Debian 包替换规则](https://www.debian.org/doc/debian-policy/ch-relationships.html#replacing-whole-packages-forcing-their-removal)

先关闭 Parley 窗口，以普通桌面用户在项目目录执行（脚本会在需要时使用 sudo）：

```bash
bash scripts/repair-installed-parley.sh "$PWD/target/release/bundle/deb/ParleyDesktop_0.4.0-alpha.1_amd64.deb"
```

脚本会检查已安装的 `parley` 版本：如果仍是我们的旧 0.x 版本，在安装修复包的同一事务中用 `parley-` 显式移除旧包，避免 APT 误选 KDE 升级；如果已经是 KDE 版本，先运行 `sudo apt-get install -f` 完成被打断的系统事务，再安装修复包并检查 APT 状态。若本机已有本项目的 WebKit VBlank 启动器补丁，会备份并更新其可执行路径与图标，保留性能设置。它不会自动清理软件包、禁用源或删除学习记录。`apt-get -f` 用于修复损坏的依赖关系，详见 [Ubuntu APT 手册](https://manpages.ubuntu.com/manpages/noble/man8/apt-get.8.html)。

修复包包含此前的 Web/GUI 文本选择与任务提示修复。当前版本会升级旧工作区到 schema 7，旧版不能直接打开升级后的数据库；第一次启动前保留数据备份。真实模型 API 调用仍按要求暂缓实测。

打包验收：隔离 Ubuntu 24.04 中复现了旧包与 KDE 图标的覆盖错误；依次修复依赖、安装新包后，KDE `parley` / `parley-data` 与 `parley-desktop` 共存且 `apt-get check` 通过。另验证了旧 0.3.0 包的显式迁移、保留学习数据目录、`parley-cli --help` 和安装文件校验。发布脚本检查独立包名、私有程序路径、图标、桌面入口及全部 payload 校验和；新包与 Ubuntu 两个同名包的文件路径交集为空。未在本机执行需要 sudo 密码的实际修复，也未调用真实模型 API。
