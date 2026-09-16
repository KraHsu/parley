# Windows 安装包

Windows x64 安装包由 GitHub Actions 的 `windows-2022` runner 构建，无需自备 Windows 编译机器。工作流见 [Windows installer](https://github.com/KraHsu/parley/actions/workflows/windows.yml)。

## 获取与安装

进入一次成功的工作流运行，下载 `parley-windows-x64` artifact 并解压。里面包含 `Parley_<版本>_x64-setup.exe` 和 `SHA256SUMS-windows-x64`。Actions 产物保留 14 天；正式发布的安装包从 [Releases](https://github.com/KraHsu/parley/releases) 下载。

双击安装器，按提示安装。默认只为当前用户安装，不修改 PATH。开始菜单的 Parley 文件夹包含桌面工作区、Terminal 和 Tutor 三个入口。缺少 WebView2 时，安装器会联网下载并安装运行时。当前构建未配置代码签名，Windows 可能显示“未知发布者”。

GUI 和 `parley-cli.exe` 安装在同一个目录。默认位置为 `%LOCALAPPDATA%\Parley`，自定义安装目录时请相应替换。PowerShell 中可以这样打开原生终端伴随窗口：

```powershell
& "$env:LOCALAPPDATA\Parley\parley-cli.exe"
& "$env:LOCALAPPDATA\Parley\parley-cli.exe" --backend claude-code
& "$env:LOCALAPPDATA\Parley\parley-cli.exe" --reconnect
```

先在 Parley 设置中选择自己安装的 CLI，或通过 `--codex` / `--claude` 指定绝对路径。安装包不附带模型 CLI 或账户。CLI 的安装和登录仍由用户自行管理。

关闭 Parley 后运行新安装器即可覆盖安装。学习数据位于 `%LOCALAPPDATA%\org.parley.desktop`；默认卸载保留数据，只有明确勾选卸载器的清除数据选项才会删除。升级数据库前仍应备份工作区。

## 构建与发布

本地构建需要 Windows x64、仓库 `.nvmrc` 对应的 Node、Rust MSVC 工具链、Visual Studio C++ Build Tools 和 WebView2。安装好依赖后运行：

```powershell
npm ci
npm run package:windows
```

产物位于 `target/release/bundle/nsis/`。Tauri 会自动合并 `tauri.windows.conf.json`，生成 NSIS 安装器，并把两个 Cargo binary 一起打包。打包命令检查 PE 架构和安装包存在性，写入 SHA256 校验文件。

PR、推送到 main 或手动运行 `Windows installer` 都会构建并验证安装包。手动运行 `Desktop release packages` 并填写已有版本 tag，会同时构建 Linux 和 Windows；两边检查通过后才创建或更新同名 **draft release**，不会自动公开发布。已公开的 release 不会被此流程覆盖。

## 验证范围

`scripts/test-windows-install.ps1` 只允许在可丢弃的 GitHub Windows runner 上运行，并拒绝接触已有 Parley 数据。它使用真实安装器检查：

- 带空格、中文的安装目录，以及安装后 GUI / CLI 的 SHA256。
- 开始菜单快捷方式及终端参数引用。
- CLI 参数和退出码传递；CLI 自动找到相邻 GUI 并打开原生窗口；Windows 直接打开 Tutor 快捷方式。
- 同版本覆盖安装，以及默认卸载后 SQLite 和哨兵文件保留。

结果保存在运行的 `windows-install-validation` artifact。测试使用 Windows 自带程序代替模型 CLI，不调用真实模型服务。

2026-09-16，提交 `54be637` 的[完整运行](https://github.com/KraHsu/parley/actions/runs/35092376836)全部通过，生成 `Parley_0.4.0-alpha.3_x64-setup.exe`。同一提交的三平台基础 CI 和 Web 检查也通过。运行环境为 Windows Server 2022 x64，不等于已完成 Windows 10/11 的人工验收。

文件校验计入 Tauri 打包时对主程序 NSIS 类型标记的修改，其余字节仍完整比对。快捷方式通过 Unicode Shell 接口读取，避免旧脚本接口把中文转换成问号。

这项检查不覆盖真实 Codex / Claude Code 交互、中文输入法、Windows Credential Manager 或模型服务。第一版 Windows 安装包尚无已发布的 Windows 旧包可用于跨版本升级测试，同版本覆盖安装不能证明数据库跨版本迁移。

安装器配置参考 [Tauri Windows Installer](https://v2.tauri.app/distribute/windows-installer/)。
