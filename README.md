# Parley

**在真实对话中学习任何语言。**

Parley 是一个开源、以本地数据为主的语言学习桌面客户端，使用 **Vue 3 + TypeScript + Rust + Tauri 2**。主对话用于目标语言交流，旁边的语言助手负责母语答疑、表达提示、翻译和语法解释，生词本保存从对话中积累的词句。

通过官方 **Codex App Server** 接入：每位用户登录自己的 ChatGPT 账号，并使用自己的 Codex 额度。项目不提供共享账号或集中转发服务。Parley 是独立项目。

## 下载安装

从 [GitHub Releases](https://github.com/KraHsu/parley/releases) 下载 Linux 安装包。当前提供 Ubuntu 24.04 x86_64 预览版，包含桌面程序和 `parley-cli`；安装后无需编译源码。步骤见 [安装说明](docs/INSTALL.md)。

## 终端 Codex + 语法助手

```bash
parley-cli
```

源码开发可先运行 `npm run cli:build`，再运行 `npm run cli`。

启动器读取设置中保存的 Codex 路径，在当前终端直接运行官方 TUI，同时打开语法助手窗口。首次使用也可以指定路径：

```bash
parley-cli --codex "$(command -v codex)"
parley-cli -- resume --last
```

新对话自动关联到 GUI，无需复制文本。点击“解释当前回复”或“翻译”，或在 GUI 中选中同步的片段提问。恢复历史会话或候选不唯一时，从下拉框选择会话。同步读取 Codex 已保存的对话，通常在回复完成后更新。

终端保持 Codex 原有登录、快捷键、工具和历史；GUI 保存辅导记录。操作和边界见 [终端伴随模式](docs/TERMINAL_COMPANION.md)。

## 当前状态

当前支持 **官方 Codex TUI + 独立语法助手 GUI**，也保留双面板桌面工作台；词句学习功能仍在开发。

- 固定视口的桌面工作台：左侧导航、中间对话/词句视图、右侧语言助手。页面本身不滚动，内容区域各自滚动。
- 可编辑的双区草稿、目标语言话题提示、三种辅导模式和词句搜索空状态。
- 语言、模型、草稿和历史消息自动保存到本地 SQLite；重启后可离线查看、继续对话。
- 本机 Codex 连接、官方 ChatGPT 登录、真实模型列表与额度状态。
- 主聊和语言助手独立流式回复、停止生成、重置会话与连接错误提示。
- Tauri 桌面壳及 Vue → Rust 的运行环境查询。
- 开发、构建、格式检查、环境诊断和 CI 配置。
- 历史会话切换与删除、中断回复恢复、写入失败提示。
- 尚未实现：划词、词句收藏与复习；实施范围和阶段见 [词句学习开发计划](docs/VOCABULARY_PLAN.md)。

普通桌面工作台启动时不自动连接；终端伴随窗口会连接所选的本机 Codex。连接本身不发送模型请求。点击设置中的“连接本机 Codex”开始使用；完整操作、版本要求与限制见 [Codex 接入说明](docs/CODEX_INTEGRATION.md)，数据位置与恢复行为见 [持久化说明](docs/PERSISTENCE.md)。浏览器预览只提供界面，真实接入需要桌面端。

![Parley 桌面工作台](docs/screenshots/workspace.png)

## 本地开发

需要 Node.js **24.12+（24.x）**、npm、Rust stable，以及操作系统对应的 [Tauri 系统依赖](https://v2.tauri.app/start/prerequisites/)。仓库以 `package-lock.json` 和 `Cargo.lock` 锁定依赖。

```bash
nvm use
npm ci
npm run doctor
npm run desktop:dev
```

首次 Rust 构建会下载和编译桌面依赖，耗时通常明显长于前端构建。Linux 需要 GTK 3 和 WebKitGTK 4.1 开发包；Windows 需要 C++ Build Tools 和 WebView2；macOS 需要 Xcode Command Line Tools。

只查看界面：

```bash
npm run dev
```

打开 `http://127.0.0.1:1420`。点击左下角工作空间或右上角“未连接”进入学习设置。“检查桌面连接”在浏览器中会提示使用桌面启动命令，不会伪造 Rust 连接结果。

| 命令                                   | 用途                                          |
| -------------------------------------- | --------------------------------------------- |
| `npm run doctor`                       | 检查开发环境；聊天接入需要 Codex CLI          |
| `npm run dev`                          | Vite 浏览器界面预览                           |
| `npm run desktop:dev`                  | 启动 Tauri 开发窗口                           |
| `npm run build`                        | TypeScript 检查与前端生产构建                 |
| `npm run check`                        | Prettier、TypeScript、Rust 格式与 Clippy 检查 |
| `npm run desktop:check -- -- --locked` | 构建桌面调试程序，不制作安装包                |
| `npm run desktop:build -- -- --locked` | 构建当前平台的发行程序和安装包                |
| `npm run format`                       | 格式化前端、配置和文档                        |
| `cargo fmt --all`                      | 格式化 Rust                                   |

单独在干净检出上运行 `npm run check` 前，先运行 `npm run build`，为 Tauri 提供前端产物。

## 工作区

```text
.
├── src/
│   ├── features/
│   │   ├── codex/            # 连接状态、模型与流式消息
│   │   ├── conversation/     # 目标语言主对话
│   │   ├── tutor/            # 母语辅导区
│   │   ├── vocabulary/       # 词句收藏区
│   │   └── settings/         # Pinia 语言设置
│   ├── shared/              # 类型与 Tauri 调用边界
│   └── styles/              # 全局样式
├── src-tauri/               # Rust 桌面程序、命令及能力权限
├── public/                  # 自有 SVG 图标
├── scripts/                 # 环境诊断
├── docs/                    # 产品计划、架构和开发说明
├── Cargo.toml               # Rust workspace
└── .github/workflows/       # 三平台构建检查，尚未远端执行
```

详细安排见 [开发计划](docs/DEVELOPMENT_PLAN.md)，实现边界见 [架构设计](docs/ARCHITECTURE.md)，开发操作见 [开发指南](docs/DEVELOPMENT.md)。

## 开源许可

采用 [MIT License](LICENSE)。项目当前以免费开源方式开发；MIT 同时允许他人按许可条款使用和修改代码，包括商业使用。代码许可不代替 OpenAI 服务条款，用户仍需拥有适用的服务访问资格。
