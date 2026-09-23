# Proposal

## Why

TailTask 需要把 Tailscale 设备发现、本地持久化和可追溯的任务结果做成可安装的跨平台客户端。现有工程已搭起 React/Tauri/Rust/SQLite 骨架，但尚未通过完整构建；用户后续明确了“Agent 只有修复，没有操控权”和简约科技风格，需要把这些约束落实为可验证的开发依据。

## What Changes

- 完成基础里程碑：读取官方 Tailscale 状态，展示、搜索、筛选和选择设备，保存别名与收藏，并提供新增设备引导。
- 以 SQLite 保存设备观测、偏好、本机修复任务与有序事件；正确区分当前观测、历史缓存和未知状态，隔离不同网络上下文。
- 修复执行器暂限定为用户发起的本产品数据库检查与索引重建；后端严格拒绝任意命令、脚本及未知动作，重试复用请求编号，重启不自动重放任务。
- UI 采用用户指定的“简约、带少量科技元素”；使用 GPT Image 生成概念图，随后用真实控件实现设备、任务、本机修复与设置页面。生成图只作设计参考。
- 修复已观察到的构建缺口，准备 Windows NSIS `-setup.exe`、macOS DMG、Linux AppImage/deb 的构建、验收和 GitHub 分发流程。
- 配置 AGENTS.md、GitHub Issues 工作约定及单一领域文档上下文。GitHub 仓库按此前方案拟建为公开 `tailscale-ui`；必须先取得已登录账户的真实归属。
- 范围说明：这是完整产品的基础里程碑，**不等于原 PRD 的远程任务产品已交付**。任意远程命令与设备操控不在本变更中；远程修复的动作范围、目标设备授权与配对流程尚未明确，保持关闭并另立后续变更。用户已选定 UI 风格，旧文档“风格待确认”的表述将在实施阶段更新。

## Capabilities

### New Capabilities

- `device-directory`：Tailscale 状态适配、设备选择、网络上下文隔离、本地偏好与新增设备引导。
- `repair-task-lifecycle`：修复白名单、持久任务状态与事件、请求去重、异常恢复及本机作用范围。
- `desktop-experience`：Tauri 桌面交互、简约科技视觉、GPT Image 设计资产、真实状态与可访问性。
- `desktop-distribution`：可复现验证、三平台安装包、GitHub 源码与版本资产、平台验证记录。

### Modified Capabilities

无。当前 `openspec list --specs` 未发现已有主规格；这些能力以现有代码为起点形成首次规范。

## Impact

- 源码：`src/`、`src-tauri/`、`crates/core/`、`crates/agent/`、工作区 Cargo/npm 配置与数据库迁移。
- 工程与交付：新增 CI、发布文档、设计资产和技能配置；上传前排除数据库、真实设备信息、凭证、工具链与 npm 缓存。
- 外部依赖：用户单独安装并登录官方 Tailscale；本客户端不代管 Tailnet 管理凭证。Tailscale 提供网络能力，应用修复权限由本产品单独约束。
- 已知证据：TypeScript 检查与 4 项前端测试通过；Rust 测试因 `sqlx::migrate!` 缺少宏功能而未编译通过；安装图标缺失，三平台安装包和 GitHub 仓库尚未交付。
- 发布约束：GitHub 登录、macOS/Windows 签名材料及平台运行环境会影响上传与正式发布；未完成的平台或签名只能如实标记为待验证/测试构建。
