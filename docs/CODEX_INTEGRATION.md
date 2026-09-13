# Codex 接入

Parley 通过 Rust 启动本机 `codex app-server --stdio`，使用官方 ChatGPT 登录和用户自己的 Codex 额度。当前验证版本为 **Codex CLI 0.154.0 / Linux x86_64**。其他版本、Windows/macOS 尚未完成接入实测。

## 开始使用

1. 安装 [Codex CLI](https://learn.chatgpt.com/docs/cli)，确认终端中的 `codex --version` 能运行。
2. 运行 `npm run desktop:dev`，打开右上角连接设置，在“Codex 可执行文件路径”中填写自己安装的 Codex 的绝对路径，再点击“连接本机 Codex”。路径自动保存到本机，重启后恢复；更换前请先断开连接。
3. 连接后先读取本机 Codex 已有的账号，再读取模型列表；已登录用户无需再次登录。只有确认没有 ChatGPT 登录时才显示“登录 ChatGPT”。登录后未更新时，可点击“读取本机登录 / 刷新”。登录过程可取消或重新打开页面。
4. 选择对话模型、辅导模型及语言。辅导模型优先选择账号列表里的 Luna；未提供 Luna 时使用账号默认模型，可自行更改。
5. 主面板用目标语言交流，辅导面板允许母语提问。Enter 发送，Shift+Enter 换行；生成期间点击停止按钮可中断。

macOS / Linux 可在正常终端中运行 `command -v codex`，将输出路径粘贴到设置。Windows 应填写 `codex.exe` 的完整路径，不支持 `.cmd` shell 包装器。可以使用包含空格的路径，无需加引号或命令参数。应用直接启动所填文件，不自动从 PATH 选取，也不读取 `PARLEY_CODEX_BIN`；不内置、安装或更新 CLI，不锁定具体版本。旧工作区新增该设置时默认为空，需要首次填写。

若遇到进程退出，连接面板显示实际版本、执行文件路径和错误详情。已复现 **0.145.0** 不认识 `features.view_image` 启动覆盖项；当前已验证版本为 **0.154.0**。请自行更新所选 CLI，或在设置中改选兼容版本。用户配置本身有错误时，应根据详情修正配置。

## 当前行为

- 主聊、辅导拥有独立 thread，可并行生成。按 thread/item ID 路由，最终消息用于校准流式文本。
- 每个面板同时最多一个活动 turn；停止也覆盖 thread/turn 尚在创建的情况。
- 修改语言、模型或辅导模式后，下一条消息创建新 thread，并显示会话分隔提示。“新对话”/“重置”创建新的历史条目，保留旧记录。
- 账号、模型列表和额度窗口来自 App Server，分开查询。账号与额度查询各等待最多 5 秒，模型列表最多 15 秒；只读查询超时可刷新重试，不主动断开健康连接。额度异常不阻塞账号或模型展示。主模型与辅导模型都计入对应账号额度；额度查询不可用时不显示虚构数值。
- 消息、草稿与设置保存在本地 SQLite。重连或重启后可恢复原 thread，不自动重发旧消息；详见 [持久化说明](PERSISTENCE.md)。
- App Server 继承启动 Parley 时的账号环境，保留 `HOME` 和 `CODEX_HOME`。未设置 `CODEX_HOME` 时使用 Codex 默认目录；选择 CLI 可执行文件不会复制或更换账号文件。若终端使用自定义 `CODEX_HOME`，应从相同环境启动应用。
- 登录由 Codex 管理。Parley 不接收 access/refresh token，不读取网页登录 Cookie，不提供共享账号或 API Key 接口。断开连接保留本机 Codex 登录；登录操作可能改变同一台机器其他 Codex 客户端使用的账号。
- 主面板的目标语言规则通过模型指令实现，尚无发送前的语言识别器，因此不能保证每条输入都是目标语言。
- 当前显示纯文本，尚未实现 Markdown、划词传给辅导、词句收藏或断线后自动补齐未收到的输出。

## 进程与权限配置

Rust 使用参数数组启动进程，工作目录固定在应用本地数据目录的 `conversation-workspace`。不通过 shell 拼接用户输入。

启动覆盖禁用 shell/unified exec、Apps、插件、MCP、hooks、浏览器/电脑工具、图像工具、技能搜索、多智能体、记忆及其他已知的环境工具；MCP 配置整体替换为空。连接握手后读取有效配置，确认 MCP 和指定 feature 开关已经关闭。`project_doc_max_bytes=0` 禁止加载 AGENTS 文本，`notify=[]` 禁止外部通知命令。上游新增工具或协议行为仍需升级时重新验证。

thread 使用 `read-only` sandbox、`approvalPolicy=never`、`environments=[]` 和独立的语言指令。App Server 发来的工具审批一律取消，其他服务端工具/权限请求返回不支持。前端只能调用明确注册的账号、连接和对话命令；只允许 Rust 打开 `https://auth.openai.com` 登录地址。

使用了当前协议的实验能力握手和环境选择字段；不兼容版本会显示连接失败，当前没有移除限制后自动重试的降级路径。连接时会检查并显示实际 CLI 版本和执行文件路径。异常退出时，连接面板显示退出状态和最多 8 KiB 的 stderr 尾部；包含常见凭据标记或邮箱的行会隐藏。诊断仅供本机显示，不保存到数据库。原始配置和推理过程不转发至前端。

## 测试

```bash
npm run test
cargo test --workspace --locked
```

默认测试不调用模型。仅验证启动、握手和配置兼容性，可运行 `PARLEY_TEST_CODEX_BIN="$(command -v codex)" cargo test --workspace --locked live_codex_connection -- --ignored`（macOS / Linux）。该测试变量只用于测试，不影响应用的路径设置。

可选的真实连接测试使用本机已登录的 ChatGPT 账号，发送三条短消息并消耗少量 Codex 额度：

```bash
PARLEY_TEST_CODEX_BIN="$(command -v codex)" cargo test --workspace --locked live_codex_dual_conversation -- --ignored --nocapture
```

该测试需要账号提供 Luna，验证同一 Rust 传输层的握手、模型目录、独立 thread、并行 turn、流式增量、完成事件及重启后的上下文恢复；不输出账号邮箱或令牌。

协议参考：[官方 App Server 文档](https://learn.chatgpt.com/docs/app-server)、[官方配置参考](https://learn.chatgpt.com/docs/config-file/config-reference)。字段以实际 CLI 生成的 `generate-ts --experimental` 结果核对。技术接入能力不等同于对所有具体使用方式作条款保证。
