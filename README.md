# xiangwriter远程器

基于 **Tauri 2 + React + Rust + SQLite** 的原生桌面应用，使用官方 Tailscale 网络连接设备。

0.2.0 测试版包含设备管理、执行端配对、远程命令与脚本、持久任务记录、日志、取消、超时和结果文件下载。用户已撤回原“Agent 仅能修复”的限制。本机数据库维护仍保持独立入口。

## 安装

[安装包与 SHA-256 清单](https://github.com/xiangwriter-dev/tailscale-ui/releases) · [构建状态](https://github.com/xiangwriter-dev/tailscale-ui/actions) · [验证记录](docs/validation.md)

- Windows x64：`xiangwriter远程器_0.2.0_x64-setup.exe`，NSIS 当前用户安装，缺少 WebView2 时从微软下载运行时。
- macOS：Apple Silicon 与 Intel 分别提供 DMG。
- Linux：Ubuntu 24.04 x64 基线，AppImage 和 deb。

所有产物均为无签名测试构建，尚无 Windows 代码签名或 Apple 公证。发布资产存在和 CI 通过只证明对应构建成功，原生安装、跨机器与平台集成测试另行记录。

## 使用

1. 两端安装并登录官方 [Tailscale](https://tailscale.com/docs/how-to/quickstart)，确认网络策略允许目标 TCP 47321。
2. 在目标打开“本机执行端”，选择允许工作目录并开启。新安装默认关闭，应用不自动授权网络内设备。
3. 在目标生成配对信息，复制到控制端“远程任务 → 配对设备”，核对设备和证书指纹。
4. 选择目标，填写程序与独立参数数组，或选择目标已安装的解释器运行脚本。提交后查看任务详情。

关闭控制窗口不会结束目标任务。停止执行端可选择排空或取消后停止。撤销某个控制端后，其令牌不能再提交、读取或取消任务，已接受任务默认继续。

详见 [远程使用说明](docs/remote-usage.md)，包括 Windows PowerShell 策略、日志编码、配对、结果限制和登录启动。

## 数据与实现

- SQLite 控制端与执行端使用独立目录和独占锁；迁移前备份，已启动且结果未知的任务不自动重放。
- TLS 使用证书固定和地址校验，禁用代理继承与重定向。
- 配对信息五分钟有效、单次消费，最多五次错误；控制端令牌使用平台凭证库，执行端只存摘要。
- 命令使用参数数组；队列默认并发 1、最大 4、最多等待 100 项。
- Windows Job Object / POSIX 进程组管理执行实例，取消宽限五秒。
- 结果文件复制到独立区生成 SHA-256 清单，支持 .partial 续传及校验。

允许工作目录用于位置校验，不是命令沙箱。执行权限等同目标当前用户。Linux 凭证存储需要登录会话中的 Secret Service，不提供明文降级。界面采用简约科技风，GPT Image 设计参考及提示词位于 docs/design。

应用标识为 `com.xiangwriter.remote`，本地数据位置在“设置”页查看。控制端为 controller.db，执行端为 agent/agent.db。0.2.0 使用模式 3；升级后 0.1.0 无法打开新模式数据。卸载不主动删除用户数据库。

## 开发与打包

Node.js 24.18.0、Rust 1.98.1；依赖由两个锁文件固定。Windows 需要 MSVC 和 WebView2，其他平台按 [Tauri 前置依赖](https://v2.tauri.app/start/prerequisites/) 准备，Linux 另需 libdbus-1-dev。

```sh
npm ci
npm run desktop:dev
```

以上打开原生窗口。`npm run dev` 仅用于内部布局预览，不能访问设备、凭证或数据库。

```sh
npm run check
npm test
npm run build
cargo fmt --all --check
cargo check --workspace --locked
cargo test --workspace --locked
npm run tauri build -- --bundles nsis -- --locked
node scripts/collect-artifacts.mjs windows-x64
```

macOS 使用 `--bundles dmg`，Linux 使用 `--bundles appimage,deb`。安装器位于 target/release/bundle/。维护 CLI 位于 crates/agent；新增 serve 子命令需先提供显式配置，不会因启动桌面而自动监听。内部 Rust/npm 代号仍为 TailTask。
