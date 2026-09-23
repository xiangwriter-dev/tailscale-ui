# 0.2.0 验证记录

应用：xiangwriter远程器。日期：2026-09-24。内部代号 TailTask。

## 已有证据

| 项目 | 结果 |
| --- | --- |
| 工具链 | Node 24.18.0、Rust 1.98.1、Windows 11 x64 |
| 前端 | TypeScript / Vite 通过；12 个测试通过 |
| Rust | 工作区 check / fmt 通过；35 个测试通过（含 2 个辅助入口） |
| 数据库 | 模式 1/2 升级至 3，WAL 一致备份、目录锁、所有权隔离、并发幂等、重启不重跑均通过 |
| 配对 | 过期、单次消费、5 次错误失效、令牌摘要与撤销测试通过 |
| TLS | 真实 TLS 握手、证书变化拒绝、重定向拒绝、部分下载续传及 SHA-256 不符拒绝通过 |
| 进程 | Windows 合成进程实测：中文独立参数、真实退出码、父子进程取消、超时、输出洪泛截断通过 |
| 本机完整链路 | 独立测试目录，经真实本机 Tailscale 地址、系统凭证库完成配对、exec、PowerShell script、幂等、取消、重启记录与撤销，通过；测试服务与凭据已清理 |
| 前端失败路径 | 详情失败不重提、未知提交沿用原请求、网络变化保留草稿并禁用提交、系统自启注册失败不显示开启，通过 |

首次 Windows 脚本实测被系统默认执行策略拒绝，随后增加显式任务选项“仅本次进程使用 RemoteSigned”，保持默认“遵循系统策略”。以显式选项重测通过，没有修改系统或用户全局策略。

0.1.0 基础构建四个平台全部通过：[Actions 35890418954](https://github.com/xiangwriter-dev/tailscale-ui/actions/runs/35890418954)。其 Windows 安装器已实测新装、启动、单实例、覆盖安装、卸载与数据保留。该证据不能替代新增 0.2.0 产物验收。

## 尚未证明的项目

- 0.2.0 的发布安装器构建和安装状态须以本节后续补充为准，不能借用 0.1.0 结果。
- 原生 Tauri 窗口不能由当前会话的 UI 工具操作；React 测试、浏览器布局和原生进程测试分别记录，不冒充原生窗口全流程测试。
- macOS/Linux 实机、登录后自启注册、平台凭证授权提示及不同机器的 3×3 组合尚无验证环境。
- Windows 登录后自启仅有模板和失败 UI 测试，未在用户系统注册真实自启项。
- 未验证新设备加入 Tailscale 的完整网络审批流程；产品提供官方安装/登录与重新发现指引。
- 未提供签名、公证、自动更新或屏幕/鼠标键盘远程控制。

## 可复现检查

```sh
npm ci
npm run check
npm test
npm run build
cargo fmt --all --check
cargo check --workspace --locked
cargo test --workspace --locked
```

原生完整链路辅助程序：`cargo build -p tailtask-remote --bin tailtask-test-worker --locked`，然后运行 `tailtask-test-worker native-smoke <独立测试目录>`。仅在测试主机上调用：需当前 Tailscale 已登录，使用该主机自己的地址和 47879 临时测试端口，结束关闭服务并删除测试凭据。目录不可与用户数据混用。

未上传真实节点身份、数据库、私钥、令牌或运行日志。MSVC 的“正在创建库”被 Rust 1.98 报为 linker_messages warning；构建成功且未屏蔽提示。
