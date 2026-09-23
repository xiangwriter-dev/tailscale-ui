# 验证记录

应用：xiangwriter远程器。日期：2026-09-24。内部代号 TailTask。

## 0.2.1 Windows 远程桌面

用户确认从设备列表打开 Windows 系统远程桌面，本轮只交付 Windows，macOS/Linux 工作搁置。

- TypeScript、Vite 构建与 16 项前端测试通过；新增直接连接、错误重试、连续点击抑制、非 Windows/本机禁用、前台恢复刷新和配对提示测试。
- Windows Rust check、fmt 与 43 项测试通过（含辅助入口）；新增网络变化、节点不可见、本机/系统限制、IP 参数注入拒绝、最新地址选择、固定系统客户端路径和启动失败测试。
- 新命令已经列入 Tauri 的生成命令清单及主窗口能力声明。后端每次启动重新 inspect，前端仅传网络与节点 ID。
- 原生窗口的完整点击、远端 Windows 账号登录和证书提示没有自动操作验证。应用只声明 mstsc 已打开，不声明远端登录成功。
- [最终 Windows 构建 35905779960](https://github.com/xiangwriter-dev/tailscale-ui/actions/runs/35905779960) 对应源码 9e8bac0；前端、Rust、打包、安装验证及产物收集步骤均成功。

最终 CI 在独立 Windows 运行器中安装 0.2.1，核对产品名/版本，并逐字节确认 NSIS 安装后的程序内容符合预期打包变换；安装与卸载退出码均为 0，卸载后测试可执行文件移除。

[v0.2.1 Windows 测试版](https://github.com/xiangwriter-dev/tailscale-ui/releases/tag/v0.2.1) 已公开，文件 `xiangwriter-remote_0.2.1_x64-setup.exe` 为 5,366,920 字节，SHA-256：`2bc87190fb9b21c5af1a64ca0c106e3d3b4ed04f327d93b403823c8b32a47afb`。安装器及两份清单均核对 GitHub 服务端摘要，随后不带账号凭据下载公开安装包并再次核对 SHA-256。本轮不覆盖历史平台验证结论。

本机构建的 0.2.1 原生程序在独立目录通过真实 Tailscale/Windows 凭证库的配对、exec/script、幂等、取消、重启记录与撤销回归。用户旧版正在运行，因此本轮没有在用户主机静默安装或卸载，安装测试移至隔离的 Windows CI。

首次 CI 已完成安装和产品版本核对，但直接比较安装后程序与构建目录程序的哈希失败。[Tauri 打包实现](https://github.com/tauri-apps/tauri/blob/dev/crates/tauri-bundler/src/bundle.rs) 会将 bundle marker 从 UNK 改为 NSS，再恢复构建目录中的原程序。校验脚本已按该确切变换构造预期载荷，全文件逐字节比较，不忽略其他差异；2 项 Node 回归测试覆盖正常标记、其他字节损坏、缺失标记及截断。未修改应用程序以绕过检查。

## 0.2.0 历史证据

## 已有证据

| 项目 | 结果 |
| --- | --- |
| 工具链 | Node 24.18.0、Rust 1.98.1、Windows 11 x64 |
| 前端 | TypeScript / Vite 通过；12 个测试通过 |
| Rust | 最终四平台工作区 check / fmt / test 通过；Windows 39、macOS 各 40、Linux 41 项通过（按测试输出统计，含辅助入口） |
| 数据库 | 模式 1/2/3 升级至 4，WAL 一致备份、目录锁、所有权隔离、并发幂等、重启不重跑、离线结果索引保留均通过 |
| 配对 | 过期、单次消费、5 次错误失效、令牌摘要与撤销测试通过 |
| TLS | 真实 TLS 握手、证书变化拒绝、重定向拒绝、部分下载续传及 SHA-256 不符拒绝通过 |
| 进程 | Windows 合成进程实测：中文独立参数、真实退出码、父子进程取消、超时、输出洪泛截断通过 |
| 本机完整链路 | 独立测试目录，经真实本机 Tailscale 地址、系统凭证库完成配对、exec、PowerShell script、幂等、取消、重启记录与撤销，通过；测试服务与凭据已清理 |
| 安装程序完整链路 | 在独立测试目录使用 NSIS 安装后的 tailtask-desktop.exe 运行相同完整流程，通过；测试后台服务已停止，测试凭据已移除 |
| 前端失败路径 | 详情失败不重提、未知提交沿用原请求、网络变化保留草稿并禁用提交、系统自启注册失败不显示开启，通过 |
| 日志与恢复 | UTF-8 多字节与 ANSI 序列跨读取边界、无效尾字节、进度范围测试通过；中断的结果收集标记不完整但不重跑命令 |
| 新页面布局 | 浏览器 1280×720 下核对远程任务与本机执行端空状态，无横向溢出；真实配对表单依赖原生 IPC，未冒充浏览器实测 |

首次 Windows 脚本实测被系统默认执行策略拒绝，随后增加显式任务选项“仅本次进程使用 RemoteSigned”，保持默认“遵循系统策略”。以显式选项重测通过，没有修改系统或用户全局策略。

0.1.0 基础构建四个平台全部通过：[Actions 35890418954](https://github.com/xiangwriter-dev/tailscale-ui/actions/runs/35890418954)。其 Windows 安装器已实测新装、启动、单实例、覆盖安装、卸载与数据保留。该证据不能替代新增 0.2.0 产物验收。

## Windows 0.2.0 本地产物验收

最后一次本机构建的安装包 `xiangwriter远程器_0.2.0_x64-setup.exe`，5,357,049 字节，SHA-256：`79ebeca2f28a4d9ad5430ec07e90e49f68e2fb0a234989532ebad1041b5fc96f`。包含 Darwin 修复与设置页权限说明更新，实际安装后的程序再次完成配对、exec/script、幂等、取消、重启与撤销验证；安装与卸载退出码均为 0。正式测试版附件采用下节的最终 CI 产物。

前一轮 c2ca08e 构建（SHA-256 `0ac91aa7ffa598310de86487c3db99403c3e3a9ed0afbf375cfc1e731ee182ba`）另有新装、覆盖安装和卸载证据，退出码均为 0；可执行程序产品名/版本正确，运行时窗口标题为 xiangwriter远程器。重复启动进程退出，原实例保持运行。既有模式 2 数据库升级到 4，生成一份升级前备份，原记录数量保持不变；覆盖安装数据库 SHA-256 不变。测试卸载后用户数据库保留且 `quick_check=ok`。上述测试未删除用户数据库。最终 setup.exe 保留在 release 目录。

macOS 取消测试曾暴露 Darwin 对仅含僵尸的进程组返回 EPERM，修复增加 libproc 状态确认，真实权限拒绝仍报错，不再重复向已确认结束的数字进程组发送信号。依据：[Apple XNU killpg1 实现](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/kern/kern_sig.c)、[libproc 接口](https://github.com/apple-oss-distributions/xnu/blob/main/libsyscall/wrappers/libproc/libproc.c)。最终平台结论以 CI 链接为准。

Linux AppImage 后台与自启使用持久的 APPIMAGE 路径，避免保存 GUI 的临时挂载可执行路径；systemd ExecStart 禁止环境变量展开并转义路径百分号。依据：[AppImage 环境变量](https://docs.appimage.org/packaging-guide/environment-variables.html)、[systemd 命令行语法](https://github.com/systemd/systemd/blob/v257/man/systemd.service.xml)。包含路径选择和模板测试，真实登录自启仍未冒充实测。

## 最终 CI 与发布产物

源码提交 `24bd8158644ba94a4dcce9b94f11dfdb1fbd110a` 的 [Actions 35900010408](https://github.com/xiangwriter-dev/tailscale-ui/actions/runs/35900010408) 四个平台全部成功。每个平台均通过 TypeScript、12 项前端测试、Vite 构建、Rust 格式/检查/测试以及 Tauri 安装包构建。macOS arm64/x64 的 Rust 测试各 40 项、Windows 39 项、Linux 41 项，包含平台专属和辅助测试入口。

下载后的五种安装文件及八份 manifest/校验清单共 13 个文件已逐一核对 SHA-256 与字节数。发布目录为 `release/v0.2.0/`；[v0.2.0 测试版](https://github.com/xiangwriter-dev/tailscale-ui/releases/tag/v0.2.0) 使用这些 CI 文件。

GitHub 会改写中文附件名，因此发布文件统一使用 `xiangwriter-remote_0.2.0_*`，对应清单同步更新，并通过 sourceName、sourceCommit、sourceRun 保留 CI 来源。安装器字节未修改，应用显示名仍为 xiangwriter远程器；已上传附件同时核对 GitHub 返回的 SHA-256 摘要。

最终 Windows CI 安装器为 5,352,562 字节，SHA-256：`21fe2d98e6a56b96121e2ce059e6ee8ff11f4202137fcb2ee4659b14ae8643ec`。在独立目录安装退出码 0，产品名为 xiangwriter远程器，产品版本 0.2.0；安装后的程序经真实本机 Tailscale 和 Windows 凭证库通过 TLS 配对、exec/script、幂等、取消、重启记录和撤销。随后卸载退出码 0，程序移除，已有用户数据库哈希保持不变；临时后台服务与凭据已清理。此项证据直接对应发布的 Windows 文件，不套用本机构建哈希。

## 尚未证明的项目

- 不同构建生成的安装包哈希可能不同；本机安装证据对应上述明确哈希，GitHub CI 产物各有独立清单。
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

原生完整链路辅助程序：`cargo build -p tailtask-remote --bin tailtask-test-worker --locked`，然后运行 `tailtask-test-worker native-smoke <独立测试目录> [已安装桌面程序路径]`。仅在测试主机上调用：需当前 Tailscale 已登录，使用该主机自己的地址和 47879 临时测试端口，结束关闭服务并删除测试凭据。目录不可与用户数据混用。

未上传真实节点身份、数据库、私钥、令牌或运行日志。MSVC 的“正在创建库”被 Rust 1.98 报为 linker_messages warning；构建成功且未屏蔽提示。
