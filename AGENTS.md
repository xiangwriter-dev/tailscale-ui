# TailTask 工程约定

用户最新平台安排：先完成 Windows，macOS 和 Linux 搁置。Windows 从设备列表直接打开系统远程桌面；该入口不要求任务执行端配对。后续恢复其他平台工作需用户新指令。

用户最新指令优先。用户于 2026-09-24 明确撤回“Agent 只有修复、没有操控权”的限制，恢复原始远程任务产品范围：由用户选择并授权目标设备，主动提交远程命令/脚本并查看进度和结果。不可继续把仅修复作为产品限制。Agent 不因安装或设备在线而自动接收远程命令；配对和任务提交遵循原 PRD。应用显示名为 xiangwriter远程器，仓库为 tailscale-ui。UI 已由用户指定为简约风格，带少量科技元素，设计时使用 GPT Image 生图。

## Agent skills

### Issue tracker

工作事项记录在本仓库的 GitHub Issues；由已配置的 Git remote 确定归属。详见 docs/agents/issue-tracker.md。

### Domain docs

使用 single-context：根目录 CONTEXT.md 与 docs/adr/。详见 docs/agents/domain.md。
