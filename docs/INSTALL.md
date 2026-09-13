# 安装 Parley

当前预览版提供 **Ubuntu 24.04 x86_64（amd64）** 的 `.deb` 安装包。Windows、macOS 和其他 Linux 发行版尚未验证或提供安装包。

## 下载和安装

从 [GitHub Releases](https://github.com/KraHsu/parley/releases) 下载 `Parley_0.1.0_amd64.deb` 和 `SHA256SUMS`，放在同一目录，执行：

```bash
sha256sum -c SHA256SUMS
sudo apt install ./Parley_0.1.0_amd64.deb
```

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

## 升级和卸载

升级时使用 `sudo apt install ./新版本安装包.deb`。卸载程序：

```bash
sudo apt remove parley
```

工作区默认保存在 `~/.local/share/org.parley.desktop/`（遵循 `XDG_DATA_HOME`），升级和卸载不会删除个人学习记录。Codex 的登录和历史由 Codex 自己管理。

## 从源码构建安装包

在配置好开发环境后执行：

```bash
npm ci
npm run package:linux
```

产物位于 `target/release/bundle/deb/`，包含 `parley`、`parley-cli`、应用菜单入口和安装文档。当前打包依赖下限针对 Ubuntu 24.04 构建环境；更换构建系统时应重新检查动态库依赖。
