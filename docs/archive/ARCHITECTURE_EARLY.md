# 早期 Codex 架构设计（历史记录）

此文档保留初期决策背景。当前实现请看[架构说明](../ARCHITECTURE.md)。

```text
Vue / Pinia
  主会话 | 语言助手 | 生词本 | 设置
              │ 受限 Tauri commands / typed events
Rust application layer
  会话协调 | Codex adapter | 学习数据 repository
              │                  │
  官方 codex app-server         SQLite（本地）
  stdio / JSON-RPC              版本迁移、备份、导出
              │
  用户自己的 ChatGPT 登录与 Codex 额度
```

前端负责呈现与输入；Rust 负责生命周期、协议路由和数据一致性。模型输出是展示数据，不是可执行指令。

## Codex 适配边界

使用官方 App Server 扩展点，不复制网页登录 Cookie，不直接逆向订阅推理端点。初期使用用户安装的 CLI；将来是否打包官方二进制，需要单独确认分发、许可和升级策略。

接入流程：

1. Rust 解析可执行文件路径并使用参数数组启动进程，不拼接 shell 命令。
2. `initialize` 指明 `parley` 客户端身份，收到结果后发送 `initialized`。
3. 查询账号；必要时执行官方 `account/login/start` 流程。
4. 分页读取 `model/list`，提供主模型与辅导模型选择。
5. 创建或恢复两个独立持久 thread；从 `turn/start` 到 `turn/completed` 按 ID 路由。
6. 在 Rust 中将协议事件转换为应用事件；Vue 不依赖完整上游协议。
7. 退出时取消活动工作并回收子进程；崩溃后提示恢复，不自动重放用户输入。

协议的具体字段应以实际支持版本生成的 schema 为准。优先使用稳定能力；需要实验字段时必须在兼容文档中标明并覆盖降级行为。

对话所需工具能力应最小化：不应因为底层是编码智能体，就让语言学习自动获得任意文件读写和命令执行。M1 实测官方支持的禁用/隔离配置；应用工作目录使用专用目录，不继承用户当前代码仓库。保留凭据兼容性的同时避免无意继承全局插件和指令，具体覆盖方式需按 CLI 版本验证。

## 双会话与上下文

`LearningSession` 关联一个主 `threadId`；辅导会话关联自己的 `threadId`。事件统一携带 `sessionId`、`channel`、`threadId`、`turnId`、`itemId`，防止两区串流。连接状态、认证状态和每个 turn 的状态分别维护。

主对话 prompt 包含目标语言、学习水平、回答风格和纠错偏好。辅导 prompt 包含母语、目标语言和请求类型。prompt 文件未来放入 `src-tauri/prompts/`，版本号随请求元数据记录。

辅导请求使用结构化上下文：选中文字、所在原句、来源消息 ID、前后少量消息和用户问题。选中文本作为引用材料处理，不作为更高优先级指令。避免默认复制完整主对话，减少重复上下文和额度消耗。

原始 Codex thread 是上游运行记录；本地消息表保存用户可见投影和学习引用。使用唯一上游 item ID 更新投影，避免重连后重复追加。只把已确认完成的状态当作完成，崩溃中的消息标为 interrupted。

## 早期计划数据模型（M3，非当前数据库结构）

| 表                       | 关键字段                                                                       | 关系/约束                                |
| ------------------------ | ------------------------------------------------------------------------------ | ---------------------------------------- |
| `settings`               | key, value_json, updated_at                                                    | 语言、主题、模型偏好；不存凭据           |
| `learning_sessions`      | id, title, native_language, target_language, level, created_at                 | 语言标识支持 BCP 47                      |
| `conversations`          | id, session_id, channel, codex_thread_id, model_id, prompt_version             | channel 为 main/tutor，thread 映射独立   |
| `messages`               | id, conversation_id, upstream_item_id, turn_id, role, text, status, created_at | 上游 item 标识幂等；未完成消息明确标记   |
| `tutor_requests`         | id, conversation_id, source_message_id, selected_text, context_snapshot, kind  | 来源可空，保留上下文快照                 |
| `vocabulary_entries`     | id, language, text, lookup_key, meaning, note, mastery, created_at             | 原始拼写不破坏；归一化仅用于查询         |
| `vocabulary_occurrences` | id, entry_id, source_message_id, sentence_snapshot, selection_metadata         | 同一词句可有多个语境；会话删除可保留快照 |

所有写入通过 Rust repository 和事务。正式表结构已经建立，见 `src-tauri/migrations/`；上表仅保留初期设计背景。

选择区间需明确字符偏移单位，不能混用 JavaScript UTF-16 与 Rust UTF-8 字节索引。首版保存原句与选中字符串；精确定位同时记录经验证的偏移和原文哈希，文本变化时回退到快照。

数据文件位于 Tauri 提供的 app data directory。支持用户显式备份、导出和删除；Codex 自身的认证/历史记录位置需在数据说明中另列，删除 Parley 数据不等于删除 Codex 数据。

## Tauri 权限与通信

当前注册运行信息及明确的 Codex 连接、登录、发送、停止和重置命令，通过 AppManifest 生成权限，并只授予 `main` 窗口。流式事件使用 Tauri IPC Channel；不暴露任意 JSON-RPC 转发。生产 CSP 仅加载本地资源；开发 CSP 额外允许 Vite 样式注入和本地 HMR WebSocket。

后续新增命令使用明确 DTO，例如 `start_conversation`、`ask_tutor`、`save_vocabulary`；不暴露“执行任意命令”“读取任意文件”之类通用接口。数据库操作和 Codex 凭据不直接交给 WebView。外部链接通过受控打开方式处理，Markdown 不允许原始 HTML 或任意协议链接。

## 文档依据与待验证项

以下官方文档在本次初始化时核对；在线内容会更新，应在 M1 选定 CLI 版本后记录兼容矩阵。

- [Codex App Server](https://learn.chatgpt.com/docs/app-server)：产品集成、握手、thread/turn、模型查询与认证流程。
- [Codex Authentication](https://learn.chatgpt.com/docs/auth)：ChatGPT 登录与 API Key 两种认证方式。
- [Codex Pricing](https://learn.chatgpt.com/docs/pricing)：订阅使用限制和模型额度；主/辅调用均消耗额度。
- [OpenAI Terms of Use](https://openai.com/policies/row-terms-of-use/)：用户账号、服务访问和使用约束。
- [Tauri + Vite](https://v2.tauri.app/start/frontend/vite/)：构建目录、开发服务配置。
- [Tauri Capabilities](https://v2.tauri.app/security/capabilities/)：窗口权限和应用命令白名单。
- [Vue Quick Start](https://vuejs.org/guide/quick-start)：Vue/TypeScript/Vite 开发基础。

官方支持 App Server 产品集成和 ChatGPT 登录是已确认事实；“Parley 的全部具体使用方式符合所有适用条款”不是这些技术文档单独能证明的结论。当前已在 Linux 验证配置覆盖与两会话并发。凭据沿用本机 Codex 官方存储；全新账号浏览器授权及其他平台仍需人工实测。
