# 开发指南

## 工具与约定

- Node 24.x（至少 24.12），使用 `.nvmrc`；npm 安装依赖并提交锁文件。
- Rust stable，`rust-toolchain.toml` 要求 rustfmt 和 clippy。
- Rust workspace 位于仓库根目录，默认产物在 `target/`。
- TypeScript 使用严格检查；Vue 使用 `<script setup lang="ts">`。
- Rust/TypeScript 跨边界 DTO 使用 camelCase JSON；协议适配留在 Rust。
- 前端按功能目录组织；需要第二个页面时再引入 Router。
- 不提交账号信息、`.env`、日志、聊天数据库、个人 `.codex` 或 `.agents` 目录。

## 开发和验证

```bash
npm ci
npm run doctor
npm run desktop:dev
```

修改后执行：

```bash
npm run format
cargo fmt --all
npm run build
npm run check
npm run desktop:check -- -- --locked
```

`npm run test` 执行前端状态回归，`cargo test --workspace --locked` 执行 Rust 协议/会话测试。默认均不调用模型；真实账号测试与 CI 分开，见 [接入说明](CODEX_INTEGRATION.md)。构建检查与功能测试分别记录。

桌面开发启动会自动启动 Vite。若 `1420` 端口已占用，先关闭已有开发实例；固定端口用于与 Tauri 的 `devUrl` 保持一致。不要把 Vite 服务暴露到公网。

首次图标生成或修改 SVG 后：

```bash
npm run tauri -- icon public/app-icon.svg
```

将桌面图标资源纳入 Git。`src-tauri/gen/` 是自动生成的 schema，无需提交。

如果移动了仓库目录，旧的 Tauri 编译缓存可能包含原目录的权限文件绝对路径。遇到 `failed to read plugin permissions` 且错误路径指向旧目录时，运行 `cargo clean -p tauri -p parley` 后重新构建即可，无需重装依赖。

## M0 手工验收

1. 在浏览器和桌面分别打开界面，确认导航、主对话和辅导区存在；切换到词句收藏可查看搜索空状态。
2. 切换目标语言，确认标签和输入提示同步更新。选择话题会把目标语言提示填入草稿，切换视图不丢失草稿。
3. 从“未连接”或工作空间按钮打开学习设置，执行“检查桌面连接”：桌面显示来自 Rust 的版本和平台；浏览器提示使用桌面启动方式。Escape 可关闭设置。
4. 输入框支持编写草稿，辅导模式可切换；发送按钮仍禁用，不显示模拟模型回答。
5. 页面始终保持一个视口高度；长消息、辅导内容和词句列表只能在各自区域滚动，顶部工具栏和底部输入框不移动。
6. 780×640 最小桌面窗口保持双面板；760px 及以下切换为页签显示对话/助手/词句，避免向下堆叠。键盘能访问各个滚动区和设置控件。
7. 关闭开发进程，确认没有遗留本项目的 Vite/Tauri 进程。

布局约束详见 [UI 布局说明](UI_LAYOUT.md)。

实际验证结果记录在 [验证记录](VALIDATION.md)，不要将尚未运行的 CI 标记为通过。

## CI

`.github/workflows/ci.yml` 配置 Linux、Windows 和 macOS 的依赖安装、格式/类型/Clippy 检查和桌面调试构建。CI 不需要 Codex 登录，不调用模型，也不自动发布产物。

本地仓库初始化不会自动创建 GitHub 远程仓库。配置远程并推送后，才会运行对应托管平台上的工作流。

## 首个后续任务

Codex 首版已完成连接与双 thread 收发。后续推进划词上下文、语言规则验证与词句/注释持久化，同时补齐新账号授权和其他平台验收。使用官方账号接口，不直接读取认证文件。
