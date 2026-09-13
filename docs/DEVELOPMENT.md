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

M0 没有业务单元测试；这些命令验证格式、类型、Rust 编译和完整桌面链接，不应把编译成功写成“功能测试通过”。M1 起在协议和会话行为引入 fixture 测试；真实账号测试与 CI 分开。

桌面开发启动会自动启动 Vite。若 `1420` 端口已占用，先关闭已有开发实例；固定端口用于与 Tauri 的 `devUrl` 保持一致。不要把 Vite 服务暴露到公网。

首次图标生成或修改 SVG 后：

```bash
npm run tauri -- icon public/app-icon.svg
```

将桌面图标资源纳入 Git。`src-tauri/gen/` 是自动生成的 schema，无需提交。

## M0 手工验收

1. 在浏览器和桌面分别打开界面，确认主对话、辅导区和生词本空状态存在。
2. 切换目标语言，确认主窗口语言标签和输入提示同步更新。
3. 点击“检查桌面连接”：桌面显示来自 Rust 的版本和平台；浏览器提示使用桌面启动方式。
4. 确认聊天和辅导操作处于不可用状态，不显示模拟模型回答。
5. 窄窗口下各区域可阅读，键盘可以聚焦语言选择和环境检查按钮。
6. 关闭开发进程，确认没有遗留本项目的 Vite/Tauri 进程。

实际验证结果记录在 [验证记录](VALIDATION.md)，不要将尚未运行的 CI 标记为通过。

## CI

`.github/workflows/ci.yml` 配置 Linux、Windows 和 macOS 的依赖安装、格式/类型/Clippy 检查和桌面调试构建。CI 不需要 Codex 登录，不调用模型，也不自动发布产物。

本地仓库初始化不会自动创建 GitHub 远程仓库。配置远程并推送后，才会运行对应托管平台上的工作流。

## 首个后续任务

按 M1 实现 Codex 进程与协议适配，先完成启动、握手和账号查询，再接入登录和双 thread 验证。不要通过读取用户现有认证文件来代替官方登录接口，也不要在 UI 骨架里加入未经调用的“已连接”状态。
