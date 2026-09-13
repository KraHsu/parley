# Codex 接入

Parley 通过 Rust 启动本机 `codex app-server --stdio`，使用官方 ChatGPT 登录和用户自己的 Codex 额度。当前验证版本为 **Codex CLI 0.154.0 / Linux x86_64**。其他版本、Windows/macOS 尚未完成接入实测。

## 开始使用

1. 安装 [Codex CLI](https://learn.chatgpt.com/docs/cli)，确认终端中的 `codex --version` 能运行。
2. 运行 `npm run desktop:dev`，打开右上角连接设置，点击“连接本机 Codex”。
3. 已通过 Codex 登录 ChatGPT 的用户会直接看到账号与可用模型；否则点击“登录 ChatGPT”，在 OpenAI 浏览器页面完成登录。登录过程可取消或重新打开页面。
4. 选择对话模型、辅导模型及语言。辅导模型优先选择账号列表里的 Luna；未提供 Luna 时使用账号默认模型，可自行更改。
5. 主面板用目标语言交流，辅导面板允许母语提问。Enter 发送，Shift+Enter 换行；生成期间点击停止按钮可中断。

若桌面应用的 PATH 找不到 CLI，可在启动应用前设置 `PARLEY_CODEX_BIN`，值为真实可执行文件的绝对路径。Windows 建议指定 `codex.exe`，不支持通过 shell 执行 `.cmd` 包装器。应用不会自动安装或更新全局 CLI。

## 当前行为

- 主聊、辅导拥有独立 thread，可并行生成。按 thread/item ID 路由，最终消息用于校准流式文本。
- 每个面板同时最多一个活动 turn；停止也覆盖 thread/turn 尚在创建的情况。
- 修改语言、模型或辅导模式后，下一条消息创建新 thread，并显示会话分隔提示。“新对话”/“重置”创建新的历史条目，保留旧记录。
- 账号、模型列表和额度窗口来自 App Server。主模型与辅导模型都计入对应账号额度；额度查询不可用时不显示虚构数值。
- 消息、草稿与设置保存在本地 SQLite。重连或重启后可恢复原 thread，不自动重发旧消息；详见 [持久化说明](PERSISTENCE.md)。
- 登录由 Codex 管理。Parley 不接收 access/refresh token，不读取网页登录 Cookie，不提供共享账号或 API Key 接口。断开连接保留本机 Codex 登录；登录操作可能改变同一台机器其他 Codex 客户端使用的账号。
- 主面板的目标语言规则通过模型指令实现，尚无发送前的语言识别器，因此不能保证每条输入都是目标语言。
- 当前显示纯文本，尚未实现 Markdown、划词传给辅导、词句收藏或断线后自动补齐未收到的输出。

## 进程与权限配置

Rust 使用参数数组启动进程，工作目录固定在应用本地数据目录的 `conversation-workspace`。不通过 shell 拼接用户输入。

启动覆盖禁用 shell/unified exec、Apps、插件、MCP、hooks、浏览器/电脑工具、图像工具、技能搜索、多智能体、记忆及其他已知的环境工具；MCP 配置整体替换为空。连接握手后读取有效配置，确认 MCP 和指定 feature 开关已经关闭。`project_doc_max_bytes=0` 禁止加载 AGENTS 文本，`notify=[]` 禁止外部通知命令。上游新增工具或协议行为仍需升级时重新验证。

thread 使用 `read-only` sandbox、`approvalPolicy=never`、`environments=[]` 和独立的语言指令。App Server 发来的工具审批一律取消，其他服务端工具/权限请求返回不支持。前端只能调用明确注册的账号、连接和对话命令；只允许 Rust 打开 `https://auth.openai.com` 登录地址。

使用了当前协议的实验能力握手和环境选择字段；不兼容版本会显示连接失败，当前没有移除限制后自动重试的降级路径。运行时不把 CLI stderr、原始配置或推理过程写入前端日志。

## 测试

```bash
npm run test
cargo test --workspace --locked
```

默认测试不调用模型。可选的真实连接测试使用本机已登录的 ChatGPT 账号，发送三条短消息并消耗少量 Codex 额度：

```bash
cargo test --workspace --locked live_codex_dual_conversation -- --ignored --nocapture
```

该测试需要账号提供 Luna，验证同一 Rust 传输层的握手、模型目录、独立 thread、并行 turn、流式增量、完成事件及重启后的上下文恢复；不输出账号邮箱或令牌。

协议参考：[官方 App Server 文档](https://learn.chatgpt.com/docs/app-server)、[官方配置参考](https://learn.chatgpt.com/docs/config-file/config-reference)。字段以实际 CLI 生成的 `generate-ts --experimental` 结果核对。技术接入能力不等同于对所有具体使用方式作条款保证。
