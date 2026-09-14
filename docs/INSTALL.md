# 安装 Parley

当前预览版提供 **Ubuntu 24.04 x86_64（amd64）** 的 `.deb` 安装包。Windows、macOS 和其他 Linux 发行版尚未验证或提供安装包。

## 下载和安装

从 [GitHub Releases](https://github.com/KraHsu/parley/releases) 下载 `Parley_0.3.0_amd64.deb` 和 `SHA256SUMS`，放在同一目录，执行：

```bash
sha256sum -c SHA256SUMS
sudo apt install ./Parley_0.3.0_amd64.deb
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

## 词句学习

安装后无需连接 Codex 即可添加、检索和复习词句。对话选词、注释、标签及 JSON/CSV 备份操作见[词句学习指南](https://github.com/KraHsu/parley/blob/main/docs/VOCABULARY.md)。

## 升级和卸载

从 v0.1.0 升级前，关闭所有 Parley 窗口并备份工作区数据库。v0.3.0 会在事务中将 schema 1 顺序迁移到 3，保留已有会话；旧版不能读取或写入升级后的数据库。需要回退时使用升级前的备份，不能直接降级数据库。

升级时使用 `sudo apt install ./新版本安装包.deb`。卸载程序：

```bash
sudo apt remove parley
```

工作区默认保存在 `~/.local/share/org.parley.desktop/`（遵循 `XDG_DATA_HOME`），升级和卸载不会删除个人学习记录。Codex 的登录和历史由 Codex 自己管理。

## 从源码构建安装包

当前主分支包含尚未发布的多后端功能，使用方式见[模型服务设置](BACKENDS.md)。它会将旧工作区升级到 schema 7；运行前备份已有数据，旧版不能直接打开升级后的数据库。需要复现已发布的 v0.3.0 时，请使用对应 release tag，而不是当前主分支。

在配置好开发环境后执行：

```bash
npm ci
npm run package:linux
```

产物位于 `target/release/bundle/deb/`，包含 `parley`、`parley-cli`、应用菜单入口和安装文档。当前打包依赖下限针对 Ubuntu 24.04 构建环境；更换构建系统时应重新检查动态库依赖。
