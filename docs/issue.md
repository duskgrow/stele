# Stele 接口问题汇总

基于 2026-05-09 对 Stele 12 个 MCP 接口的全量验证，共发现 5 个 bug 和 2 个设计缺陷。

## 问题总览

| # | 接口 | 严重程度 | 类型 | 简述 |
|---|------|---------|------|------|
| 1 | search | 中 | Bug | 中文 FTS5 修复不彻底，部分中文短语仍搜不到 |
| 2 | page.list | 中 | Bug | 根目录和父目录返回空，只有叶子目录能用 |
| 3 | maintain lint | 中 | Bug | 所有合法 slug 被误报为 invalid format |
| 4 | maintain backlinks | 中 | Bug | 误报链接目标不存在，与 graph 模块逻辑不一致 |
| 5 | page.put | 中高 | 设计缺陷 | 全量覆盖时不保留 append-only 内容，导致 timeline 数据丢失 |
| 6 | page.append | 中 | 设计缺陷 | 盲追加到文件末尾，不区分 body 和 timeline 区域 |
| 7 | maintain full | 低 | 纹波 | lint + backlinks 叠加导致 full 维护报告全是噪音 |

## Bug 1：中文 FTS5 搜索修复不彻底

search 的 FTS5 tokenizer 已更新，连字符问题已修复，但中文搜索仍有不一致：

```
查询词          页面实际包含    搜索结果
接口验证        ✅              total: 1 ✅
测试记录        ✅              total: 0 ❌
全量验证        ✅（标题+正文）  total: 0 ❌
逐一验证        ✅              total: 0 ❌
verification    ✅              total: 1 ✅
MCP             ✅              total: 2 ✅
```

英文搜索稳定正常。部分中文短语能命中，部分不能，无明显规律。

**修复方向：** 排查 trigram tokenizer 对特定中文字符组合的分词边界，确保所有中文子串都能被匹配到。

## Bug 2：page.list 根目录和父目录返回空

```
page.list(dir=".")          → count: 0, files: []  ❌
page.list(dir="wiki")       → count: 0, files: []  ❌
page.list(dir="wiki/bugs")  → count: 2, files: [...]  ✅
```

只有叶子目录（直接包含文件的目录）能返回结果。根目录 `.` 和包含子目录的父目录全部返回空。

**修复方向：** page.list 应返回指定目录下的直接子项（文件 + 子目录），根目录 `.` 应能列出顶层文件和目录。

## Bug 3：maintain lint 误报 slug 格式错误

执行 `maintain(scope="lint")` 将所有 5 个页面标记为 "invalid slug format"，包括：
- `test/full-verification.md` — 通过 page.put 正常创建
- `wiki/bugs/stele-fts5-search-issues.md` — 完全符合文档规范

lint 的 slug 校验规则与 page.put 写入时的校验规则不一致。page.put 接受这些 slug，但 lint 拒绝。

**修复方向：** 统一 slug 格式校验逻辑，lint 和 page.put 使用同一套规则。

## Bug 4：maintain backlinks 误报链接目标不存在

测试页面包含 `[[stele]]`、`[[mcp]]`、`[[wiki/bugs/stele-fts5-search-issues]]` 三个 wikilink。

```
maintain(backlinks): "links to nonexistent page 'stele'"     ❌ 误报
graph.query:         neighbors 包含 stele                     ✅ 正常
graph.backlinks:     从 stele 查到反向链接                     ✅ 正常
```

maintain 的链接目标查找逻辑与 graph 模块不一致。可能原因：
1. slug 匹配方式不同（精确匹配 vs 模糊/归一化）
2. 查询路径不同（FNS 文件系统 vs 本地 SQLite 索引）
3. wikilink 解析后 target slug 的后缀处理差异

**修复方向：** maintain backlinks 应复用 graph 模块的链接查找逻辑，确保一致性。

## 设计缺陷 1：page.put 覆盖 append-only 内容

page.put 全量替换页面内容时，不区分"用户可编辑区"和"append-only 累积区"。通过 page.append 追加的 timeline 等内容会被静默覆盖，无警告、无确认、不可恢复。

**复现证据：** 验证 FTS5 修复时，页面 `wiki/bugs/stele-fts5-search-issues.md` 的 timeline（`- 2026-05-09: 初始报告，基于 stele 完整 CRUD 验证轮次`）在 page.put 更新正文后丢失。

**修复建议：** page.put 写入时自动保留 append-only 区域（页面末尾以 `---` 分隔的部分）。想删除需显式传参 `clear_append=true`。

## 设计缺陷 2：page.append 不理解页面结构

page.append 盲追加到文件末尾。如果页面已有 timeline 区域，新内容会插到 timeline 之后，破坏文档结构：

```
frontmatter
---
正文内容
---
- 2026-05-09: 初始记录        ← 原 timeline

## 新追加的内容                ← append 到了这里（位置错误）
---
- 2026-05-09: 第二条          ← 又一次 append，timeline 被割裂
```

**修复建议：** Stele 内部将页面解析为三个区域（frontmatter / body / timeline），page.append 追加到 body 末尾（timeline 之前），page.put 只替换 body 自动保留 frontmatter 和 timeline。

## 纹波影响：maintain full 全是噪音

lint 误报 + backlinks 误报叠加后，`maintain(scope="full")` 的输出几乎全是假阳性，真正的维护问题被淹没，失去实用价值。

## 建议修复优先级

1. **Bug 3 + 4（maintain 误报）** — 修复成本低，恢复 maintain 工具的可信度
2. **Bug 2（page.list）** — 修复成本低，恢复目录浏览能力
3. **Bug 1（中文搜索）** — 需要排查 tokenizer，可能涉及 Rust 代码修改
4. **设计缺陷 1 + 2（page.put / page.append）** — 需要引入页面结构感知，改动较大但从根本上解决问题
