# 安装包升级验证

当前源码版本为 `0.4.0-alpha.3`。本地构建产物尚未发布到 GitHub Releases；最新公开安装包仍为 alpha.2。

验证使用隔离 Ubuntu 24.04、真实 `.deb`、APT、Tauri/WebKit GUI 和 GNOME Secret Service。所有模型请求发往容器内的本地 HTTP/SSE 服务，未调用真实厂商 API；未改动个人工作区或本机安装。

## 2026-09-15 结果

| 升级起点          | 旧包来源                                               | 数据、备份与凭据续聊 |
| ----------------- | ------------------------------------------------------ | -------------------- |
| alpha.1 → alpha.3 | 本机已安装程序的只读副本，保留原文件校验值后重建测试包 | 通过                 |
| alpha.2 → alpha.3 | GitHub Release 原包，通过发布的 SHA-256 校验           | 通过                 |

两条路径均保留 19 张原有表的全部记录。原生 GUI 测试还验证了两个 Claude 伴随终端之间的选择、原生 PID 保持、退出状态自动更新、运行中缓存删除被拒绝、已结束缓存清理，以及取消关联后清空引用。

自动检查：104 项前端单元测试、117 项 Rust/CLI 测试、8 项 Chromium 浏览器测试通过；Web 实际生成的表达评分／撤销导出文件另交给 Rust 导入并逐字段比较。9 项需要显式环境的 Rust 测试默认跳过，跨语言检查单独执行。真实厂商 API 和其他平台仍未实测。

## 验证内容

- 在旧版 GUI 中通过应用命令创建服务配置、系统凭据、实际对话、未发送草稿、词句原句与标签、双向复习卡、评分及撤销、回收站和词句编辑草稿。
- 关闭旧版，用 APT 原位安装新版，检查 `apt-get check` 与 `dpkg --audit`。
- 打开新版，核对 schema 7 → 9；逐表比较旧数据库的所有记录，检查完整性和外键。
- 逐表比较首次升级自动生成的数据库备份；再次打开不会重复生成升级备份。
- 从真实系统凭据库读取旧密钥，在原会话续聊；改为仅本次会话后重启，确认系统密钥已删除；重填相同密钥继续原会话。
- 检查密钥明文未写入数据库。复习排程、撤销记录和所有原句保留。

## 复现

测试脚本位于 `tests/native/installed_upgrade.py`。只能在隔离用户目录运行，不要指向个人工作区。

1. 在 Ubuntu 24.04 测试容器安装旧包以及 `xvfb xauth dbus-x11 gnome-keyring webkit2gtk-driver python3`。
2. 以普通用户启动隔离 D-Bus / Xvfb，解锁测试 Secret Service，在相同环境运行 `WebKitWebDriver --port=4444 --replace-on-new-session`。设置 `TAURI_WEBVIEW_AUTOMATION=true`；软件渲染环境变量只用于测试。
3. 执行 `python3 tests/native/installed_upgrade.py seed --artifacts /audit/old-version`。
4. 保留用户目录和凭据库，用 APT 安装新包。
5. 执行 `python3 tests/native/installed_upgrade.py verify --artifacts /audit/old-version`。

脚本通过 WebDriver 操作实际安装的 `/usr/lib/parley-desktop/parley`，调用其正常 IPC；端口 4891 是本地模型服务。生成的基线与结果放在指定 artifacts 目录，不提交到 Git。

`tests/native/companion_management.py` 另行验证 GUI 切换两个仍在运行的原生终端、退出后的状态更新、禁止删除运行中缓存、GUI 清理和取消关联；终端由本地脚本模拟。

Windows/macOS 的系统凭据、窗口和安装升级尚未在原生环境验收；此记录只证明 Ubuntu 24.04 amd64 的升级路径。
