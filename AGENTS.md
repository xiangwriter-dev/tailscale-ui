# TailTask 工程约定

用户最新指令优先。产品 Agent 暂按仅允许限定修复处理，不提供任意命令或设备操控接口；这项保守边界详见 docs/permission-boundary.md，具体远程修复范围仍待明确。未确认的远程任务权限保持关闭，不能用旧 PRD 覆盖后续限制。UI 已由用户指定为简约风格，带少量科技元素，设计时使用 GPT Image 生图。

## Agent skills

### Issue tracker

工作事项记录在本仓库的 GitHub Issues；由已配置的 Git remote 确定归属。详见 docs/agents/issue-tracker.md。

### Domain docs

使用 single-context：根目录 CONTEXT.md 与 docs/adr/。详见 docs/agents/domain.md。
