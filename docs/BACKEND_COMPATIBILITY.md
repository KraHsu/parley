# 多后端兼容记录

更新：2026-09-14。下表区分本地协议测试和真实服务调用。fixture 是根据官方字段结构构造的合成数据；其中的模型 ID 用于请求路由断言，不代表已调用该模型。模型列表以用户所选服务实际返回为准，也可以手动填写模型 ID。

| 后端                       | 本地验证                                                 | 真实模型调用                                                | 当前边界                                    |
| -------------------------- | -------------------------------------------------------- | ----------------------------------------------------------- | ------------------------------------------- |
| OpenAI Responses           | HTTP/SSE、加密续聊内容、取消、异常和保存                 | 未验收                                                      | 纯文本对话，不启用工具                      |
| Claude Messages            | HTTP/SSE、原生内容块、多轮与保存                         | 未验收                                                      | 使用独立 API Key                            |
| Gemini Interactions        | HTTP/SSE、原生步骤、无状态多轮与保存                     | 未验收                                                      | 当前固定 API revision；服务演进需要另行验证 |
| DeepSeek                   | 分片、末块用量、reasoning 与正文分离、多轮、流内错误     | 未验收                                                      | 保留模型默认思考与采样设置                  |
| 通义千问                   | 分片、独立用量块、null 字段、多轮、流内错误              | 未验收                                                      | 需填写对应区域 / 工作区的兼容地址           |
| Kimi                       | 分片、末块用量、历史 reasoning 回传、多轮、流内错误      | 未验收                                                      | 不强制通用 thinking / temperature 参数      |
| GLM / Z.AI                 | 分片、末块用量、空工具字段、多轮、错误信封               | 未验收                                                      | 测试针对 Z.AI 协议；中国区智谱端点未验收    |
| 自定义 Chat Completions    | 共享流解析、失败状态及上下文预算                         | 取决于目标服务                                              | 仅保证本文列出的文本协议子集                |
| Codex GUI / 原生终端       | CLI 0.154.0 双配置、通用 turn、有界通知与合并保存        | gpt-5.6-luna 双面板、重启续聊、首段后取消及另一面板完成通过 | 多配置共享所选 CLI 的本机登录目录           |
| Claude Code GUI / 原生终端 | CLI 2.1.269 回环 API、显式 session fork、HTTP hooks 共存 | 真实服务与完整交互式 TUI 待验收                             | GUI 使用 API Key；终端保留用户原生 CLI 环境 |

## 兼容接口的请求与历史

四个厂商均发送 `model`、纯文本 `messages`、`stream: true` 和 `max_tokens: 4096`。OpenAI 厂商的 Chat Completions 配置使用 `max_completion_tokens`。不提供工具定义，不显式覆盖采样或思考模式，也不以更换参数自动重试失败请求。

- DeepSeek：请求包含 `stream_options.include_usage`，接受末尾带 `finish_reason` 和用量的同一数据块。官方说明当前不会另外发一块仅包含用量的消息；测试按此结构构造。[Chat Completions 文档](https://api-docs.deepseek.com/api/create-chat-completion/)
- 千问：请求包含 `stream_options.include_usage`，接受结束块之后的空 `choices` 用量块；按实际模型默认思考模式调用，不向所有型号强行添加 `enable_thinking` 或 `preserve_thinking`。[深度思考文档](https://www.alibabacloud.com/help/en/model-studio/deep-thinking)
- Kimi：请求包含 `stream_options.include_usage`，保存并在后续 assistant 消息中原样回传 `reasoning_content`。不同型号对思考与采样参数的约束不同，保持默认值避免发送不受支持的通用覆盖参数。[Chat Completions](https://platform.kimi.ai/docs/api/chat)、[型号参数差异](https://platform.kimi.ai/docs/api/models-overview)
- Z.AI：末块直接提供用量；请求不发送其参考文档未列出的 `stream_options`。既接受空数组 / null 工具字段，也明确拒绝实际工具调用。支持识别 `code` + `message` 的错误信封，错误详情不直接回显。[流式消息](https://docs.z.ai/guides/capabilities/streaming)、[接口参考](https://docs.z.ai/api-reference/llm/chat-completion)

续聊状态和可见正文分别保存；reasoning 不进入聊天正文或词句快照。上下文按序列化后的内容计量，包含可见正文、续聊字段和 JSON 转义；超预算时丢弃最旧的完整问答对，保留最近完整对话与本次输入。本次输入本身超限会在请求前拒绝。

## 完成与失败

兼容协议需要成功的 `finish_reason: stop`、`[DONE]` 和非空可见正文才能完成。输出限制、内容过滤、工具调用、未知失败状态、无效正文增量和中途断流均不标为成功；保留已收到的正文，但失败轮次不用于下一轮续聊。思考内容不会被替换为最终回答。

`src-tauri/src/backends/fixtures/compatible.json` 覆盖四个厂商的不同数据块布局。测试逐字节边界切分每份合成 SSE；另经真实回环 HTTP、BackendState 和 Diesel 执行每个厂商两轮成功回复及一轮流内失败，验证请求参数、认证头、续聊、用量和部分正文保存。

真实服务验收还需要各厂商有效凭据，并逐项记录模型、日期、多轮、停止、错误和桌面学习流程。当前测试不证明模型质量、实际价格、账户权限或服务可用性。

## Linux 安装包验证

`0.4.0-alpha.1` 已在隔离的 Ubuntu 24.04 amd64 环境通过 APT 实际安装并运行。环境未安装 Node、Codex 或 Claude Code；真实 Tauri/WebKit 窗口配合本机 Responses fixture，完成系统凭据库保存与应用重启读取、两侧对话、一侧停止而另一侧完成、选词解释保存、离线复习、JSON v2 导出及去重导入、再次重启与来源跳转。数据库完整性和外键检查通过。

这证明安装包的本地学习与 API 接入链路，不代表已通过真实 OpenAI 服务验收。中文通过剪贴板输入，实际输入法、CLI 与 API 的完整安装版组合，以及 Windows/macOS 原生运行仍待验证。系统凭据库实测为 Linux GNOME Keyring Secret Service，其他平台凭据库不能据此视为已验证。
