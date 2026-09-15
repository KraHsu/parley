# Parley 当前架构

Parley 有两个入口：Vue/Tauri 桌面工作台和纯浏览器 Web。桌面通过 Rust 连接 API、Codex App Server 或 Claude Code；Web 只连接 API。两个入口都有独立的主聊和语言助手，学习数据留在各自设备。

## 代码从哪里读起

| 目录                                        | 职责                                                             |
| ------------------------------------------- | ---------------------------------------------------------------- |
| `src/features/chat`                         | 桌面会话协调、消息展示、轮次事件过滤                             |
| `src/features/workspace`                    | 当前面板、草稿、偏好和历史导航，控制保存与关闭顺序               |
| `src/features/backends`                     | 服务配置、模型列表、凭据状态和后端切换                           |
| `src/features/codex`                        | Codex 的连接、账号与模型能力；旧 store 导出保留兼容性            |
| `src/features/vocabulary`                   | 桌面词句编辑、来源、检索、回收站和离线复习                       |
| `src/shared`                                | 两端共用的任务提示、字段校验、会话设置规则，以及桌面通用 UI 工具 |
| `src/web/store.ts`                          | Web 的会话和模型操作，对组件提供业务入口                         |
| `src/web/learning.ts`                       | Web 的词句校验、去重、删除和复习操作                             |
| `src/web/persistence.ts`                    | 记录修改集合、合并保存、失败重试和持久化确认                     |
| `src/web/storage.ts`                        | IndexedDB 分记录读写、事务和旧版本迁移                           |
| `src/web/validation.ts` / `backup.ts`       | 数据边界校验、旧 JSON 与逐行备份格式                             |
| `src-tauri/src/backends`                    | API/Claude 适配、凭据、HTTP/SSE、请求生命周期                    |
| `src-tauri/src/codex`                       | Codex 连接注册、流式事件和轮次控制                               |
| `src-tauri/src/chat`                        | 各桌面适配器共用的持久化轮次与发布事件                           |
| `src-tauri/src/storage`                     | Diesel 查询、SQLite 事务、词句与会话存储                         |
| `src-tauri/src/launcher.rs` / `terminal.rs` | 官方 CLI 启动、终端上下文和伴随 GUI                              |
| `fixtures` / `tests/e2e`                    | 共用错误分类，以及本地服务驱动的浏览器回归；不进入产品构建       |

## 一次桌面提问

1. 主聊或助手组件调用 `useChatStore().send()`。store 固定当前会话、服务版本、模型、任务和引用来源，并先保存草稿。
2. `backend_send` 校验服务绑定与认证作用域，创建稳定的本地用户/助手消息 ID。已停用配置、错误版本或未停止的请求不能开始新轮次。
3. Rust 按后端选择 Codex、Claude Code 或 HTTP 适配。API 从成功轮次重建上下文；Codex 使用用户自己的 CLI 和 thread；Claude GUI 使用其受限 headless 接口。
4. 流式结果交给 `chat::Publisher`。先在 SQLite 中保存投影，再向 Vue 发布带配置版本、会话、请求和序号的事件。
5. 前端只接收当前轮次的递增事件；停止、失败和正常完成分别记录。关闭工作区时等待保存与取消，失败时保留重试入口。

“怎么说／解释／翻译”是每轮任务。切换任务保留会话；更换模型或语言仍建立新的会话边界。已有签名兼容此规则，无需清空历史。Codex 的基础指令描述通用辅导职责，每轮输入单独声明当前任务。

## 一次 Web 提问或词句保存

Web 的网络请求由浏览器直接发送，API Key 只留在当前标签页内存。组件通过 store/learning 方法修改词句，不自行实现去重或复习排程。

草稿、设置采用小范围监听，词条与流式消息由业务操作显式标记修改。`persistence` 合并这些记录，`storage` 在一个事务中写入对应的 IndexedDB object stores；输入草稿不会序列化词库。新建/删除时才更新目录顺序。当前词句列表每页展示 50 条。

IndexedDB v2 使用 `workspace`、`profiles`、`conversations`、`messages`、`words`。旧 v1 快照先校验，再在完整事务中迁移；迁移保留原快照。Web Locks 控制标签页编辑权，存储版本号防止没有 Web Locks 时的陈旧覆盖。

明确的“保存词句”会等待写入成功。恢复备份先提交完整替换，再切换内存工作区和清除密钥；失败时保留原数据和预览。已有过长标签可以先导出原始数据，再进行局部修复。

## 数据和备份边界

桌面 SQLite schema 由 SQL 迁移和 Diesel schema 定义。迁移前备份、事务、记录版本及来源快照负责保护数据。PRAGMA、VACUUM 和迁移脚本保留必要的 SQL；普通业务查询走 Diesel。

Web 新备份使用 `.jsonl`：元数据、服务、会话、消息、词句分别成行，以带记录计数的结束行检测截断；导入逐行读取。旧 `.json` 仍可导入，不再采用不对称的 32 MiB 总量限制。两种备份都校验字段和记录数量，排除密钥与协议私有续聊数据。

词句迁移共用 `parley-vocabulary` JSON v3，读入兼容 v1/v2。`src/shared/learning-exchange.ts` 校验来源、卡片和历史，`src/web/learning-exchange.ts` 负责词句投影、预览与合并。Web 在词条内保留完整学习记录，编辑和识义评分同步更新这些记录，其他来源、表达卡片和回收站不会在往返中丢失。完整 Web 工作区备份仍使用独立入口，词句迁移不会改动对话和凭据。

## 凭据辨认与伴随窗口

`backend_credential_salts` 和 `backend_credential_scopes` 用每个服务独立的随机盐与 SHA-256 辨认重填的密钥，指纹包含服务 ID 和地址，绑定沿用原 scope。原始密钥仍只在内存或系统凭据库。旧绑定只能在核对原密钥后补录，不根据历史记录猜测账号。

`launcher.rs` 启动官方 CLI；`companion.rs` 保存重连所需的元数据；`terminal.rs` 的短时回调将 Claude 可见上下文原子写入独立缓存。GUI 只是读取方，关闭与重开不影响回调，也无需监听 HTTP 端口。`--reconnect` 复用会话 ID、启动时间、工作目录和 CLI 路径。

## 测试与发布

- Vitest 检查 store、校验、备份和 IndexedDB 事务；Rust 检查协议、存储和进程生命周期。
- `npm run test:web` 构建真实 Web 产物，再用 Playwright 和本地 HTTP/SSE 服务检查保存刷新、导入失败、任务切换、输入法合成事件、旧数据恢复和分页。没有真实模型请求。
- 三平台 CI 执行构建和静态检查。Pages 只允许 `main` 部署，PR 运行浏览器回归。
- Linux 安装包必须通过 `finalize-deb` 与 `check-deb`，使用独立包名和文件路径，避免覆盖 KDE Parley。原生运行与真实服务验收范围见[兼容记录](BACKEND_COMPATIBILITY.md)。

[早期设计](archive/ARCHITECTURE_EARLY.md)保留决策背景。[多后端实现记录](MULTI_BACKEND_IMPLEMENTATION.md)用于查阅历史验证过程，不作为当前模块职责的入口。
