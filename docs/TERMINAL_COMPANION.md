# 终端伴随模式

Parley 的 `parley-cli` 是官方 Codex TUI 的启动入口，旁边的独立 Tauri 窗口负责语法解释、翻译和母语答疑。使用用户在设置中指定的 Codex，不内置 CLI 或固定版本。

## 启动

在源码工作区先构建两个程序：

```bash
npm run cli:build
npm run cli
```

`cli:build` 构建包含前端产物的桌面程序和控制台启动器。修改源码后重新运行此命令。GUI 开发可单独使用 `npm run tutor:dev`；完整工作台仍使用 `npm run desktop:dev`。

未配置路径时，macOS / Linux 可以直接指定：

```bash
npm run cli -- --codex "$(command -v codex)"
```

启动器把原生参数从 `--` 之后或第一个非 Parley 参数开始原样传给 Codex：

```bash
npm run cli -- resume --last
npm run cli -- -- -m gpt-5.6-luna
npm run cli -- --no-gui -- --help
```

直接在任意工作目录调用构建后的 `target/debug/parley-cli`，即可使用该目录作为 Codex 工作目录；`-C` / `--cd` 仍可指定工作目录。控制台启动器应与 `parley` GUI 可执行文件放在一起，也可用 `--gui-bin` 指定 GUI 文件。Windows 使用对应 `.exe`；跨平台配置已提供，本次只实测 Linux。

## 自动上下文

- GUI 使用官方 App Server 的 `thread/list` 和 `thread/turns/list` 读取当前工作目录的 CLI 会话，每次请求完成后间隔约 2 秒再刷新。
- 只有本次启动后创建、且唯一匹配的新会话才自动关联。不会自动猜测多个终端中的目标。恢复旧会话时，在下拉框选择一次；TUI 内切换 `/new` 或恢复其他会话后，也可从该列表改选。
- 同步 Codex 已落盘的最近六轮，不镜像逐 token 的终端画面。短暂延迟是正常的；关闭历史记录、远端会话、不同 `CODEX_HOME` 或不兼容的历史分页接口可能无法同步。
- 只保留用户文本与助手回复，不读取推理和工具输出。提问自动携带最近六条消息中最多 5,000 个字符；有选中片段时，优先保留片段。
- 点击“解释当前回复”或“翻译”可直接提问。展开“查看终端最近回复”后，在 GUI 中选中词句即可针对片段答疑，无需操作剪贴板。终端本身的鼠标选区不跨窗口传递。
- 同步本身不调用模型；点击解释、翻译或发送问题才使用辅导模型。

实际联动截图：

![终端回复自动同步并由 GUI 解释](screenshots/terminal-tutor.png)

## 原生行为与数据

启动器不注入主对话 prompt 或配置，不解析终端输出，不改变 Codex 的工具、审批、快捷键、模型与历史。Linux/macOS 通过 `exec` 将终端进程交给官方 CLI，保留 stdin/stdout/stderr、退出码及信号行为。语法 GUI 独立运行，关闭任一端不会要求另一端同时退出。

终端会话由 Codex 管理；Parley 数据库保存 GUI 辅导消息、设置和草稿。自动上下文作为辅导模型本轮的引用材料发送，不作为新的主对话消息写入 Codex TUI，也不接管主会话的写入锁。关联选择本身不持久化，重启后按新启动时间重新匹配或手动选择。

如果已经有 Parley 工作区窗口打开，启动器会提示先正常关闭已有窗口，避免两个 GUI 同时写同一数据库。只需要启动终端时可使用 `--no-gui`。

协议依据：[Codex CLI](https://learn.chatgpt.com/docs/cli)、[App Server 的会话读取接口](https://learn.chatgpt.com/docs/app-server)。
