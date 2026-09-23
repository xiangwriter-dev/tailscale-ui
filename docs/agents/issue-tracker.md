# Issue tracker: GitHub

工作事项记录在本仓库的 GitHub Issues，通过 gh CLI 读写。仓库归属从 git remote -v 获取；未配置远程或未登录时如实报告，不推测账号，也不默认改用本地事项。

OpenSpec 的变更提案与规格保存在本地 openspec/ 中；GitHub Issue 可链接相关变更，避免出现两份相互矛盾的完整规格。

## Conventions

- 创建：gh issue create --title "..." --body-file <正文文件>
- 查看：gh issue view <编号> --comments
- 列表：gh issue list --state open --json number,title,labels
- 评论：gh issue comment <编号> --body-file <正文文件>
- 标签：gh issue edit <编号> --add-label "..." 或 --remove-label "..."
- 关闭：gh issue close <编号>

正文含多行时写入临时 UTF-8 文件，保留真实换行；不得将凭证、真实设备清单、用户日志或数据库上传到 Issue。具体写入应在用户或明确调用的技能授权范围内。

## Pull requests as a triage surface

PRs as a request surface: no.

## When a skill says publish to the issue tracker

创建本仓库的 GitHub Issue。

## When a skill says fetch the relevant ticket

执行 gh issue view <编号> --comments。
