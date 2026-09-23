# Domain Docs

## Before exploring, read these

- 读取根目录 CONTEXT.md。
- 若存在 CONTEXT-MAP.md，按其中指针读取相关上下文。
- 读取 docs/adr/ 中与当前改动相关的决策。

文件不存在时直接继续，不把缺少文档视为阻塞，也不预先批量创建空文档。领域梳理技能在术语或决策实际明确时按需创建。

## File structure

本项目采用 single-context：根目录 CONTEXT.md 与 docs/adr/。Rust workspace 内的 core、agent、desktop 属于同一产品，不据此拆成多个领域上下文。

## Use the glossary's vocabulary

使用 CONTEXT.md 中已定义的领域术语。新概念尚未定义时先标明缺口，不随意引入同义名称。

## Flag ADR conflicts

如果方案与已有 ADR 冲突，指出具体 ADR、原因和影响，不静默覆盖。
