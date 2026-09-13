# Parley

**在真实对话中学习任何语言。**

Parley 是一个开源、以本地数据为主的语言学习桌面客户端，使用 **Vue 3 + TypeScript + Rust + Tauri 2**。主对话用于目标语言交流，旁边的语言助手负责母语答疑、表达提示、翻译和语法解释，生词本保存从对话中积累的词句。

计划通过官方 **Codex App Server** 接入：每位用户登录自己的 ChatGPT 账号，并使用自己的 Codex 额度。项目不提供共享账号或集中转发服务。Parley 是独立项目。

## 当前状态

当前为 **M0：基础工作区**，并非可以使用的语言学习成品。

- Vue 三区域界面：主对话、语言助手、生词本空状态。
- 母语与目标语言选择，暂时只在当前会话中保存。
- Tauri 桌面壳及 Vue → Rust 的运行环境查询。
- 开发、构建、格式检查、环境诊断和 CI 配置。
- 尚未实现：Codex 登录、真实模型对话、划词、数据库、词句收藏与复习。

当前骨架不会主动登录 Codex 或调用模型。浏览器预览可用于开发界面；Codex 进程集成计划在桌面端实现。

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

打开 `http://127.0.0.1:1420`。浏览器中的“检查桌面连接”会提示使用桌面启动命令，不会伪造 Rust 连接结果。

| 命令                                   | 用途                                          |
| -------------------------------------- | --------------------------------------------- |
| `npm run doctor`                       | 检查开发环境；Codex 暂为可选项                |
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
