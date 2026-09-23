# Proposal

## Why

设备已从 Tailscale 读取，但 0.2.0 只有任务执行端配对入口，用户已配置的 Windows RDP 无法在应用里直接启动。用户明确选择调用系统远程桌面，并要求本轮仅完成 Windows，其他两个系统搁置。

## What Changes

- Windows 设备目录与详情增加远程桌面入口，自动使用当前目标的 Tailscale 地址调用系统 mstsc。
- 设备选择、网络刷新与任务配对明确分开；RDP 不要求任务 Agent 配对。
- 唤起前重新确认网络身份和目标，失败反馈到界面；只报告客户端已打开，不冒充 RDP 登录成功。
- 交付 0.2.1 Windows setup.exe，本轮 CI 和新版本产物仅面向 Windows。

## Capabilities

### New Capabilities
- `windows-rdp-launch`: 从可见设备直接启动 Windows 系统远程桌面。

### Modified Capabilities
无。现有远程任务执行和配对契约不变。

## Impact

设备页、Tauri 专用 IPC 与能力声明、启动后网络刷新、Windows 测试及打包流水线。不新增数据库迁移，不接触 Windows 登录凭据。macOS/Linux 的现有实现保留，本轮不继续验收或发布新产物。
