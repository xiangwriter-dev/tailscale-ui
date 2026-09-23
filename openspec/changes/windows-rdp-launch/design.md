# Design

## Context

设备页已有 Tailscale 实时发现、选择和刷新；唯一动作导向 RemoteWorkspace 的执行端配对。用户确认使用系统 mstsc，当前请求只交付 Windows。详见 proposal.md。

## Goals / Non-Goals

**Goals:** 通过专用 Tauri IPC 从网络/节点 ID 得到最新目标，直接调用系统 RDP；让设备读取和连接入口可理解。

**Non-Goals:** 不实现嵌入 RDP、自动填写 Windows 凭据、替用户开启远端 RDP、推断 ping 成功等于 RDP 登录成功。本轮不开发或发布 macOS/Linux 新版本。

## Decisions

- IPC 仅接受 context_id/node_id，后端重新 inspect 并验证状态；不接受前端 IP 或通用命令，防止过期地址和参数注入。
- 从 Windows 系统目录定位 mstsc.exe，使用独立 /v: 参数，IPv4 优先；调用成功只表示进程已启动。独立进程不随应用退出而停止。
- 设备列表与详情显示远程桌面按钮，本机、非 Windows 目标、历史状态、非 Windows 控制端显示原因；在线未知不等于端口失败，不以网络在线指示冒充 RDP 可用性探测。
- 网络刷新的并发锁复用现有 hook；新增 focus/visibility 刷新。命令执行端配对提示与 RDP 入口明确区分。
- 用合成快照测试身份变化、非法地址和启动参数；用可替换的进程启动边界验证专用 IPC 所调用路径。Windows 真实登录交互由用户在系统窗口进行，不自动输入凭据。
- 0.2.1 发布仅 Windows，保留 0.2.0 其他平台附件。产物收集阶段使用 ASCII 下载名，中文产品名不变。

## Risks / Trade-offs

- [系统客户端已打开但远端拒绝登录] → 登录与网络错误由 Windows 原生客户端反馈，应用不显示已连接。
- [启动后后台网络变化] → 每次启动重新确认目标，旧网络缓存不可执行。
- [当前会话不能自动操作原生窗口] → 记录可验证的参数与启动测试，把真实登录验收明确留给用户。

## Migration Plan

数据库仍为模式 4，用户覆盖安装 0.2.1 保留原任务与配对。回退 0.2.0 不涉及数据库降级。macOS/Linux 工作按用户要求搁置。
