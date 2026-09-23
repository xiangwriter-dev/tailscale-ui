# 0.1.0 验证记录

应用：xiangwriter远程器。日期：2026-09-24。内部代码代号 TailTask。

## 已有本地证据

| 项目 | 结果 |
| --- | --- |
| Node / Rust | Node 24.18.0；Rust 1.98.1，锁文件已生成 |
| 前端检查与生产构建 | 通过 TypeScript 和 Vite 构建 |
| 前端测试 | 9 项通过：状态筛选、设置失败、请求重试、详情读取失败、账户切换、设备选择与预览禁用 |
| Rust 工作区测试 | 18 项通过（其中 2 个为测试进程辅助入口）；包含 CLI 非零退出、数据库迁移/备份/校验、并发锁、故障持久化、幂等及子进程资源限制 |
| Tauri 编译 | Windows x64 release 编译通过 |
| NSIS 打包 | 成功生成 xiangwriter远程器_0.1.0_x64-setup.exe |
| UI 检查 | 已查看 1440px 浏览器界面预览；不计为真实桌面交互验收 |
| GitHub | 已验证公开仓库 xiangwriter-dev/tailscale-ui 与登录 owner |

MSVC 链接器输出“正在创建库”被 Rust 1.98 标为 linker_messages warning，编译成功；没有掩盖此提示。

## 需要单独验证的项目

- Windows 安装器新装、覆盖安装、启动和卸载验证仍在进行。
- 真实 Tauri 窗口中的完整设备偏好与任务交互、1024px 布局、对应平台的 Tailscale 安装形态测试尚未完成。当前自动化会话不能操作原生窗口，因此不把浏览器测试冒充桌面测试。
- 尚无新测试设备加入网络的端到端验收；添加设备功能是加入官方网络后的重新发现引导。
- macOS arm64/x64 与 Ubuntu 24.04 的 CI 结果以对应 Actions 运行记录为准；工作流文件存在不表示构建已经成功。
- macOS / Linux 安装启动、真实网络发现和持久化仍需对应系统环境。
- 未配置平台签名或 Apple 公证，所有安装器仅作为无签名测试版交付。

## 可复现命令

```sh
npm ci
npm run check
npm test
npm run build
cargo fmt --all --check
cargo check --workspace --locked
cargo test --workspace --locked
npm run tauri build -- --bundles nsis -- --locked
node scripts/collect-artifacts.mjs windows-x64
```

Rust 故障测试只使用临时目录和合成样例。未上传真实设备清单、用户数据、SQLite、凭证或运行日志。

## 原 PRD 能力对照

| 能力 | 0.1.0 状态 |
| --- | --- |
| Tauri 原生桌面工程 | 已实现 |
| 设备列表 / 搜索 / 别名 / 收藏 / 详情 | 已实现；Windows 原生 UI 完整验收待执行 |
| 后续添加设备 | 已实现引导与重新发现；新设备加入实测待执行 |
| SQLite 任务 / 事件 / 状态 | 已实现并通过故障及重启测试 |
| 白名单本机修复 | 已实现并通过测试 |
| 远程配对 / 远程修复分发 | 未开放，权限范围待明确 |
| 任意远程命令 / 控制桌面 | 按后续权限约束不提供 |
| 三平台安装包 | Windows 已构建；其他平台等 CI 证据 |
| 签名、自动更新 | 未实现 |

构建参考：[Tauri GitHub CI](https://v2.tauri.app/distribute/pipelines/github/)、[GitHub 托管 runner](https://docs.github.com/en/actions/reference/runners/github-hosted-runners)。
