# xiangwriter远程器项目上下文

## 当前产品

桌面设备目录、配对远程任务和本机应用维护。Tailscale 提供设备网络与身份观测；本应用不嵌入 VPN 实现，也不把设备在线视为远程授权。

用户于 2026-09-24 撤回了“Agent 仅能修复、不能操控”的限制。0.2.0 已实现原 PRD 中用户主动发起的远程命令、脚本、日志、取消和结果功能，正在验证各平台产物；0.1.0 是本机修复基础。窗口、安装器和界面使用 xiangwriter远程器 名称，仓库名称固定 tailscale-ui。

## 结构

- src：React 19 / TypeScript 界面；生产数据只来自 Tauri IPC。
- src-tauri：Tauri 2 命令边界、原生窗口和平台打包。
- crates/core：Tailscale 只读适配、SQLite 迁移、独占锁、白名单任务生命周期。
- crates/agent：维护 CLI；serve 子命令仅在提供本地配置后显式启动执行端。
- crates/remote：TLS 配对、平台凭证、后台生命周期、执行队列、结果文件与平台自启适配。
- openspec/changes/bootstrap-tailtask-repair-foundation：当前基础里程碑的规格和任务证据。
- openspec/changes/remote-task-execution：当前远程功能的规格与验收清单。SQLite 模式 4 增加控制端离线结果索引，已有迁移不可修改。

## 关键不变量

1. 设备键为网络上下文和节点 ID，不使用显示名去重。
2. 账户失效时不将旧缓存冒充当前在线设备。
3. 本机修复接口仍为独立白名单；新增远程执行接口必须绑定已配对的目标、控制端身份与明确任务请求，不复用本机修复目标。
4. 同一个 request_id 不能产生第二次执行；异动作冲突必须拒绝。
5. 取得规范化数据目录的独占锁后才可迁移或恢复任务。
6. 无签名产物标注为测试版；未完成的平台验收不得勾选完成。

详见 docs/permission-boundary.md、docs/validation.md。
