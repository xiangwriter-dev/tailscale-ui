# xiangwriter远程器项目上下文

## 当前产品

桌面设备目录与本机应用修复工作台。Tailscale 提供设备网络与身份观测；本应用不嵌入 VPN 实现，也不把设备在线视为远程授权。

原始需求包含远程任务；用户后续要求 Agent 仅能修复、不能操控。0.1.0 因此实现本机限定修复基础版，远程范围保留待明确。窗口、安装器和界面使用 xiangwriter远程器 名称，仓库名称固定 tailscale-ui。

## 结构

- src：React 19 / TypeScript 界面；生产数据只来自 Tauri IPC。
- src-tauri：Tauri 2 命令边界、原生窗口和平台打包。
- crates/core：Tailscale 只读适配、SQLite 迁移、独占锁、白名单任务生命周期。
- crates/agent：开发用本机修复 CLI，不启动远程监听。
- openspec/changes/bootstrap-tailtask-repair-foundation：当前基础里程碑的规格和任务证据。

## 关键不变量

1. 设备键为网络上下文和节点 ID，不使用显示名去重。
2. 账户失效时不将旧缓存冒充当前在线设备。
3. 修复动作严格白名单；作用范围始终为 local_application。
4. 同一个 request_id 不能产生第二次执行；异动作冲突必须拒绝。
5. 取得规范化数据目录的独占锁后才可迁移或恢复任务。
6. 无签名产物标注为测试版；未完成的平台验收不得勾选完成。

详见 docs/permission-boundary.md、docs/validation.md。
