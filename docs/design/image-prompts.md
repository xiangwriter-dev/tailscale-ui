# GPT Image 生图记录

日期：2026-09-23。工具：内置 image_gen（非 API/CLI 模式）。用途：UI 概念设计，非运行截图；没有上传真实设备数据。

选用图：`tailtask-ui-concept-v1.png`。图中名称、地址、版本、日期均为示例，不得作为真实状态写入应用。实际界面须由 React/CSS 与可访问的控件实现。

## 初次生成提示词

```text
Use case: ui-mockup.
Asset type: TailTask desktop application UI concept for a software requirements/design proposal. Generate a polished, realistic flat front-facing desktop application screen, landscape 16:10, no device frame or perspective.
Primary request: 用户要求“简约风格，有点科技元素”。Design a minimal, calm Chinese-language cross-platform desktop app for viewing Tailscale devices and application repair tasks. Generous whitespace, precise typographic hierarchy, fine borders, simple restrained technical details, small network-node motif. Subtle tech atmosphere, no cyberpunk clutter. Professional everyday productivity interface, highly legible.
Composition: slim left navigation rail labeled TailTask, four sections “设备”, “任务记录”, “本机修复”, “设置”. Top workspace header “设备” with search and one primary action “添加设备”. Main area contains a clean device list on the left and selected device information panel on the right. Bottom main area contains a small recent task list with progress and results.
Use only fictional demonstration devices: “工作电脑”, “开发笔记本”, “家庭服务器”. Show Windows, macOS, Linux. Network status “在线”, “离线”, “未知” with both words and small indicators. Selected-device detail “开发笔记本”, “网络在线”, “修复权限：未配对”. Right panel has “查看详情”, not a command execution action. A local task “数据库检查” is “已完成”; a local task “索引重建” is “进行中”, show phase “正在检查索引” with an indeterminate line rather than a fabricated numeric percentage.
Text: all interface labels in crisp Simplified Chinese except TailTask and operating system names. Add a quiet caption “概念设计 · 示例数据” at the bottom of the image.
Constraints: no arbitrary command box, terminal, scripts, remote control, fake CPU charts, marketing headline, robot character, stock imagery, overdecorated illustration. Do not imply remote repair is implemented. This is an interface concept only. Controls must look feasible to implement in React/Tauri. Visual decoration must never overwhelm actual device names and task states.
```

## 定向修正提示词

```text
编辑这张 TailTask UI 概念图。保留简约浅色界面、蓝色强调、当前布局、左侧导航、设备列表和右侧详情。只修正底部“最近的本机任务”区域：两条任务的“相关设备”均显示“工作电脑（本机）”，下方系统均显示 Windows，绝不能把本机任务标在 macOS 开发笔记本上。底部任务保持“数据库检查 / 已完成”和“索引重建 / 进行中 / 正在检查索引”，进度条只表达活动，不出现百分比。右侧开发笔记本仍显示“网络在线”和“修复权限：未配对”。删除左下角营销标语，仅保留淡淡的网络节点装饰。右下角保留“概念设计 · 示例数据”。不增加命令执行、终端、脚本、远程控制按钮。中文字清晰，其余维持原样。
```
