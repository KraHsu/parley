<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/media/readme-hero-dark.svg" />
  <source media="(prefers-color-scheme: light)" srcset="docs/media/readme-hero-light.svg" />
  <img src="docs/media/readme-hero-light.svg" alt="Parley：从一段对话，到一个值得留下的表达。" width="100%" />
</picture>

<div align="center">

# 边聊，边懂，边记住。

用目标语言对话，随时理解词句，把想学的表达留给下一次复习。

**[打开 Web ↗](https://krahsu.github.io/parley/)** · [桌面安装](docs/INSTALL.md)

[![MIT License](https://img.shields.io/badge/license-MIT-7664b2?style=flat-square)](LICENSE) [![Build checks](https://github.com/KraHsu/parley/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/KraHsu/parley/actions/workflows/ci.yml) [![Web](https://img.shields.io/badge/try-Parley_Web-7664b2?style=flat-square)](https://krahsu.github.io/parley/)

软件免费开源。Web 使用你自己的 API，模型请求使用所选服务的凭据和额度。

</div>

## 从“这是什么意思”，到“下次我也想用”

主聊负责交流，语言助手负责理解与表达，词句本把今天遇到的内容留给下一次练习。

| 交流时                                           | 遇到不懂的地方                                         | 想留下一个表达                           | 下一次练习                                 |
| ------------------------------------------------ | ------------------------------------------------------ | ---------------------------------------- | ------------------------------------------ |
| 用目标语言聊自己的生活，主聊与答疑各有一个面板。 | 在“怎么说 / 解释 / 翻译”之间切换，带着选中的内容提问。 | 保存词句、原句、自己的释义、注释和标签。 | 从词句本进入复习，先回忆，再看答案和自评。 |

## 选择你习惯的方式

|              | Web                                            | 桌面工作台                        | 终端伴随                                     |
| ------------ | ---------------------------------------------- | --------------------------------- | -------------------------------------------- |
| **怎么开始** | [打开浏览器](https://krahsu.github.io/parley/) | [安装说明](docs/INSTALL.md)       | `parley-cli`                                 |
| **模型连接** | 仅 API                                         | API、本机 Codex、Claude Code GUI¹ | 用户自己的 Codex / Claude Code，助手独立配置 |
| **学习数据** | 当前浏览器 IndexedDB                           | 本机 SQLite                       | 本机学习记录；原生历史由 CLI 管理            |
| **适合**     | 在浏览器里试着练几句                           | 长期积累对话与词句                | 使用原生终端时随手理解语言                   |

¹ Claude Code GUI 后端需要独立 API Key。原生 CLI 的登录与 GUI 助手连接是不同入口。Web 和桌面的备份格式目前不直接互通。

### 先从 Web 开始

1. 打开 [Parley Web](https://krahsu.github.io/parley/)，在设置中添加自己的 API 服务。
2. 为主聊和语言助手分别选择服务与模型，填写目标语言和母语。
3. 聊一句，选中想理解的词句，试着解释并收藏。

Web 密钥仅保留在当前标签页内存，刷新后需重新填写。浏览器直连需要服务允许 CORS。没有密钥也可以手动添加词句、查看本地资料和复习。[查看 Web 使用说明 →](docs/WEB.md)

### 原生终端，旁边多一个语言助手

```bash
parley-cli
```

保留官方 CLI 的登录、快捷键和历史。GUI 同步已保存的回复，你可以在同步正文中选词、解释、翻译和收藏；有多个会话候选时自行选择。

[Codex / Claude Code 终端伴随指南 →](docs/TERMINAL_COMPANION.md)

## 现在能用到什么

Web 已上线；桌面主分支是 **0.4.0-alpha.1** 预览版。Ubuntu 24.04 amd64 安装与学习流程已验证；macOS / Windows 通过构建检查，原生运行与安装包仍待验收。

提供 OpenAI、Claude、Gemini 原生 API，以及 DeepSeek、千问、Kimi、Z.AI 与自定义兼容服务预设。**提供协议适配不等于所有厂商已实测通过**，当前真实 API 厂商验收暂缓。[查看兼容记录 →](docs/BACKEND_COMPATIBILITY.md)

> **旧版 Linux 用户：**旧的 `Parley_0.3.0_amd64.deb` 与 KDE Parley 包名冲突。修复包使用 `parley-desktop` 包名及 `ParleyDesktop_…` 文件名；安装或升级前请按[安装说明](docs/INSTALL.md)操作。

<details>
<summary><strong>关于模型、数据与学习方式</strong></summary>

- **软件开源，模型服务独立。** Parley 不提供共享账号、统一余额或免费无限请求。API 使用服务商凭据；本机 CLI 按其自身登录与使用方式运行。
- **学习记录保存在本地。** Web 使用当前浏览器，桌面使用本机数据库；模型请求会发送到你选择的服务。备份方式见 [Web 指南](docs/WEB.md)和[桌面持久化说明](docs/PERSISTENCE.md)。
- **先做好文字练习。** 当前提供文字交流、表达提示、解释、翻译与词句复习；尚未实现语音对练、发音评分或跨设备同步。
- **语言可以自己选。** 目标语言与母语可配置；不同语言和模型的回答质量取决于所选服务，Parley 不承诺学习效果。
- **收藏与复习有清楚的边界。** 词句保留原句快照；Web 与桌面功能细节和备份格式有所不同。[词句学习指南 →](docs/VOCABULARY.md)

</details>

## 为自己的学习方式动手

**Vue 3 · TypeScript · Tauri 2 · Rust · Diesel / SQLite**

Node.js 24.12+（24.x）。桌面开发还需要 Rust 和 [Tauri 系统依赖](https://v2.tauri.app/start/prerequisites/)。

```bash
nvm use
npm ci
npm run web:dev       # 可连接 API 的 Web 开发环境
# npm run desktop:dev  # Tauri 桌面开发环境
```

`npm run dev` 是桌面界面的浏览器预览；Web 产品入口使用 `npm run web:dev`。全部命令、目录结构和构建步骤见[开发指南](docs/DEVELOPMENT.md)。

[模型服务](docs/BACKENDS.md) · [词句与复习](docs/VOCABULARY.md) · [架构](docs/ARCHITECTURE.md) · [开发计划](docs/MULTI_BACKEND_PLAN.md) · [宣传计划与视频脚本](docs/marketing/README.md)

欢迎用 [Issues](https://github.com/KraHsu/parley/issues) 分享一个具体的学习场景或可复现的问题，也欢迎提交改进。反馈连接问题时请说明系统、使用入口和错误现象，不要附上 API Key。

---

[MIT License](LICENSE) · Parley 是独立开源项目，与同名 KDE Parley 无关，也不是任何模型服务商的官方客户端。
