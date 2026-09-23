# xiangwriter远程器

基于 **Tauri 2 + React + Rust + SQLite** 的桌面应用。仓库：[xiangwriter-dev/tailscale-ui](https://github.com/xiangwriter-dev/tailscale-ui)。

这是 0.1.0 基础测试版：查看 Tailscale 设备、保存设备偏好，以及执行本机应用数据的限定修复。远程任务分发、任意命令、终端和设备操控尚未开放，不能把本版本视为完整远程任务产品。

## 桌面应用功能

- 读取官方 Tailscale 的真实设备状态，区分网络在线、离线、未知和历史观测。
- 按名称、别名、IP 搜索，收藏和保存本地别名，按网络上下文隔离数据。
- 添加设备引导；设备须先安装官方 Tailscale、登录目标网络并满足网络策略。
- 用户主动发起数据库完整性检查或应用索引重建；不接受命令、脚本或任意路径。
- SQLite 保存任务状态、结果和阶段事件；重复请求去重，中断任务保留记录且不自动重放。
- 简约科技风界面：设备、任务记录、本机修复、设置。设计阶段使用 GPT Image，参考图及提示词保存在 docs/design。

“网络在线”只代表 Tailscale 观测状态，不代表已授予远程修复权限。选择设备不会改变本机修复的目标。

## 安装与平台状态

Windows 安装器为 **xiangwriter远程器_0.1.0_x64-setup.exe**，采用 NSIS 当前用户安装，不要求机器范围安装。缺少 WebView2 时，安装程序会尝试从微软下载运行时。

下载入口：[GitHub Releases](https://github.com/xiangwriter-dev/tailscale-ui/releases)。仅实际上传的资产表示已交付；构建产物、安装启动实测和签名状态分别记录在 [验证记录](docs/validation.md)。

| 平台 | 目标格式 | 说明 |
| --- | --- | --- |
| Windows x64 | setup.exe | 本地构建与验证平台 |
| macOS Apple Silicon / Intel | dmg | 独立架构 CI 构建；安装启动与 Tailscale 集成仍须对应环境实测 |
| Ubuntu 24.04 x64 | AppImage、deb | CI 构建基线；不承诺所有 Linux 发行版 |

当前产物为**无签名测试构建**，没有 Windows 代码签名或 Apple 公证。请核对每个平台随附的 SHA-256 清单。应用不打包或代替官方 Tailscale；请先自行安装并登录它。

## 本地开发

工具链固定为 Node.js 24.18.0、Rust 1.98.1，依赖由 package-lock.json 和 Cargo.lock 锁定。Windows 需要 MSVC C++ 构建工具和 WebView2，其他平台按 [Tauri 官方前置依赖](https://v2.tauri.app/start/prerequisites/) 准备环境。

```sh
npm ci
npm run desktop:dev
```

此命令会打开原生桌面窗口。单独执行 npm run dev 仅用于浏览器界面预览，无法读取真实设备或 SQLite，修复操作会禁用。

```sh
npm run check
npm test
npm run build
cargo fmt --all --check
cargo test --workspace --locked
npm run tauri build -- --bundles nsis -- --locked
```

Windows 构建产物位于 target/release/bundle/nsis/。macOS 使用 --bundles dmg；Linux 使用 --bundles appimage,deb。GitHub Actions 自动运行四种架构的检查与构建，仅保存测试产物，不自动发布正式版本。

## 数据与修复权限

应用标识为 com.xiangwriter.remote。本地数据位置可在“设置”页查看；数据库文件为 controller.db，升级前备份保存在同目录 backups/。卸载程序不会主动清除这些记录。

数据库有应用身份、迁移校验和与独占锁。发现无关、损坏或高版本数据库时拒绝打开并保留原文件，不静默创建空库。仅调试构建允许用 TAILTASK_TEST_DATA_DIR 指定独立测试目录；发布版忽略该变量。

开发用 CLI 位于 crates/agent，可操作指定目录下本产品的 agent.db，仅提供 check-database、rebuild-indexes、history 三个子命令。它不是远程 Agent 服务。

详见 [权限边界](docs/permission-boundary.md)、[项目上下文](CONTEXT.md) 与 [OpenSpec 任务](openspec/changes/bootstrap-tailtask-repair-foundation/tasks.md)。

项目最初内部代号为 TailTask；源代码模块和历史设计稿保留此代号，应用显示名为 xiangwriter远程器。
