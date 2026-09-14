# 终端伴随模式

Parley 的 `parley-cli` 默认是官方 Codex TUI 的启动入口，旁边的独立 Tauri 窗口负责语法解释、翻译和母语答疑。使用用户在设置中指定的 Codex，不内置 CLI 或固定版本。

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
- 只保留用户文本与助手回复，不读取推理和工具输出。提问自动携带最近十二条消息（六轮普通对话）中最多 5,000 个字符；有选中片段时，优先保留片段。
- 点击“解释当前回复”或“翻译”可直接提问。展开“查看终端最近回复”后，在 GUI 中选中词句即可针对片段答疑，无需操作剪贴板。终端本身的鼠标选区不跨窗口传递。
- 同步本身不调用模型；点击解释、翻译或发送问题才使用辅导模型。

实际联动截图：

![终端回复自动同步并由 GUI 解释](screenshots/terminal-tutor.png)

## 原生行为与数据

Codex 模式下，启动器不注入主对话 prompt 或配置，不解析终端输出，不改变 Codex 的工具、审批、快捷键、模型与历史。Linux/macOS 通过 `exec` 将终端进程交给官方 CLI，保留 stdin/stdout/stderr、退出码及信号行为。语法 GUI 独立运行，关闭任一端不会要求另一端同时退出。

终端会话由 Codex 管理；Parley 数据库保存 GUI 辅导消息、设置和草稿。自动上下文作为辅导模型本轮的引用材料发送，不作为新的主对话消息写入 Codex TUI，也不接管主会话的写入锁。关联选择本身不持久化，重启后按新启动时间重新匹配或手动选择。

如果已经有 Parley 工作区窗口打开，启动器会提示先正常关闭已有窗口，避免两个 GUI 同时写同一数据库。只需要启动终端时可使用 `--no-gui`。

协议依据：[Codex CLI](https://learn.chatgpt.com/docs/cli)、[App Server 的会话读取接口](https://learn.chatgpt.com/docs/app-server)。

## Claude Code（开发分支，尚未发布）

源码构建后可以运行用户安装的原生 Claude Code，并让语法窗口使用设置中单独选择的助手服务：

```bash
npm run cli:build
target/debug/parley-cli --backend claude-code --claude /path/to/claude
target/debug/parley-cli --backend claude-code --claude /path/to/claude -- --resume SESSION_ID
```

省略 `--claude` 时，读取设置中唯一的已启用 Claude Code 路径；存在多个不同路径时要求显式选择。原生终端保留官方登录、用户配置、工具及交互；GUI 助手使用 API 时仍消耗对应 API 额度。

启动器为这次运行添加一个临时 hooks 插件，用户已有的 `--settings` 与 `--plugin-dir` 参数保持有效。插件只向语法窗口的回环接收器发送会话、用户输入和完成回答，接收器不会向 Claude 注入文本或控制决定。没有修改全局 hooks，也不解析终端画面。

同步只覆盖本次启动后收到的最近十二条消息；恢复会话之前的历史暂不读取。选中词句后可以解释、翻译或保存，界面会显示即将接收引用内容的助手服务。来源用独立的 Claude Code 命名空间保存，避免与 Codex thread 混淆；跨后端来源格式及备份 v2 仍在后续阶段完善。

语法窗口关闭后，原生 CLI 继续运行；接收器停止，Claude 可能显示非阻塞的 hook 连接提示。需要重新同步时重新运行终端伴随启动器。`--no-gui` 完全不添加同步插件；bare / safe 模式或管理策略禁用 hooks 时，终端仍可使用，GUI 显示尚未接收到事件。

已验证 Linux 本机 Claude Code `2.1.269` 的实际 hooks 投递、与已有 Stop hook 共存，以及原生启动器版本输出。hooks 实测使用临时目录和回环模拟 API，没有真实服务调用；完整交互式 TUI、GUI 选词保存流程及 Windows/macOS 仍待验收。
