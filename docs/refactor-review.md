# Stele 重构问题报告

审查日期: 2026-05-08 (第二轮)
审查范围: `/share/code/my_brain/` 全部源码
对比基线: 第一轮审查 (2026-05-08)

---

## 第一轮问题修复情况

| 编号 | 原始问题                      | 状态   | 说明 |
|------|-------------------------------|--------|------|
| 1    | FNS API 路径不兼容             | 已修复 | 改为 /api/note, /api/folder/notes, /api/folders 等正确端点 |
| 2    | FNS 响应码反转 + 认证头错误     | 已修复 | code==1 为成功, 用 `token` 头 |
| 3    | path_to_slug 丢失路径信息      | 已修复 | 删除 path_to_slug, slug 直接用 FNS 返回的完整路径 |
| 4    | slug 验证与文件名不兼容        | 已修复 | 允许 alphanumeric + `-` `_` `/` `.`, 加 path traversal 防护 |
| 5    | retry 多执行一次请求           | 未修复 | 循环外仍有多余一次调用 (低优先级) |
| 6    | graph/index 链接查询重复       | 已修复 | index/sqlite.rs 移除 get_backlinks/get_outgoing, 统一用 graph 模块 |
| 7    | MCP 缺少 append 操作          | 已修复 | 新增 Operation::Append + MCP "page.append" tool |
| 8    | MCP 缺少 Resources 支持       | 不修复 | 经讨论, 现阶段不需要 |
| 9    | MCP 缺少 Prompts 支持         | 不修复 | 经讨论, 现阶段不需要 |
| 10   | list_notes 不区分子目录        | 已修复 | sync 现在递归遍历目录 |
| 11   | sync 不递归子目录              | 已修复 | sync_directory 递归 + MAX_SYNC_DEPTH=10 |
| 12   | PageType 手动 match 冗余       | 未修复 | (低优先级) |
| 13   | parse_link_row 逻辑重复        | 已修复 | 改为 graph::parse_link_row pub(crate) |
| 14   | flake.nix 版本号不一致          | 未检查 | |
| 15   | 测试工厂函数重复               | 已修复 | 提取到 src/test_utils.rs |

---

## 新发现的问题

### 16. handle_page_list 默认目录仍用 "/"

严重程度: 中 — page.list 不传 dir 时失败

src/ops/page.rs:92:
```rust
let files = fns.list_notes(dir.unwrap_or("/")).await?;
```

sync 已经改为 `dir.unwrap_or(".")`, 但 page.list 还是用 `"/"`。
FNS 的 `/api/folder/notes` 端点对 `path=/` 返回 305 Invalid Params (实测确认过)。
只有 `path=.` 或不传 path 才表示根目录。

触发条件: MCP 调用 `page.list` 不传 dir 参数, 或 CLI `stele page list` 不带目录参数。

---

### 17. URL query 参数未编码

严重程度: 中 — 含特殊字符的路径导致请求失败

src/fns/client.rs 多处:
```rust
let url = format!("{}/api/note?vault={}&path={}", self.base_url, self.vault, path);
```

path 直接拼入 URL, 没有做 URL encoding。如果路径包含空格、中文、`#`、`&`、`?` 等字符:
- 空格: URL 断裂 (`/api/note?vault=X&path=wiki/my file.md` → path 被截断)
- `&`: 被解析为下一个 query 参数的开始
- `#`: 后面的部分被当作 fragment

FNS vault 里的文件名如果含中文或空格, 所有操作都会失败。

触发条件: vault 中存在文件名含空格、中文、`&`、`#`、`?` 等字符的笔记。

---

### 18. page.append 不更新本地索引

严重程度: 中 — append 后本地索引与 FNS 不同步

src/ops/registry.rs:211-213:
```rust
Operation::Append { slug, content } => {
    self.fns.append_note(&slug, &content).await?;
    Ok(json!({ "slug": slug, "appended": true }))
}
```

只调了 FNS 的 append API, 没有:
1. 重新读取更新后的完整内容
2. 更新 SQLite 中的 page 记录 (compiled_truth, content_hash 等)
3. 重新提取 wikilinks 更新链接图

对比 page.put 的处理: put 会 parse_page + index_page + update_links, append 什么都没做。

触发条件: 通过 MCP `page.append` 追加内容后, 搜索和图查询返回的是旧内容, 直到下次 sync。

---

### 19. get_note 的 430 错误未映射为 NotFound

严重程度: 低 — 错误类型不精确

src/fns/client.rs:260-265:
```rust
let fns_resp: FnsResponse = serde_json::from_str(&body)?;
if fns_resp.code == 1 {
    Ok(fns_resp.data.unwrap_or(Value::Null))
} else {
    Err(Error::Fns(fns_resp.message.unwrap_or_default()))
}
```

FNS 对不存在的笔记返回 `{"code": 430, "message": "Note does not exist"}`。
这走的是 HTTP 200 + code 430, 不是 HTTP 404。
所以 `parse_response` 会把它映射为 `Error::Fns("Note does not exist")`, 而不是 `Error::NotFound`。

对比: HTTP 404 会被正确映射为 `Error::NotFound`。但 FNS 用 HTTP 200 + 业务码 430 来表示文件不存在。

影响: 调用方无法用 `matches!(err, Error::NotFound(_))` 来区分 "文件不存在" 和 "其他 FNS 错误"。

触发条件: page.get 一个不存在的笔记。

---

### 20. list_notes 未处理 null list

严重程度: 低 — 空目录返回 null 而非空数组

src/fns/client.rs:152-157:
```rust
let list = data
    .get("list")
    .and_then(|l| l.as_array())
    .cloned()
    .unwrap_or_default();
```

代码处理了 `list: null` 的情况 (unwrap_or_default 返回空 Vec), 但 `total_rows` 的计算:

```rust
let total_rows = data
    .get("pager")
    .and_then(|p| p.get("totalRows"))
    .and_then(|t| t.as_u64())
    .unwrap_or(0);
```

如果 pager 为 null 或 totalRows 不存在, total_rows=0, 循环直接 break。逻辑正确, 但依赖 FNS 始终返回 pager 对象。如果某个 FNS 版本省略了 pager, 会导致跳过整个目录。

触发条件: FNS 对空目录返回 `{"list": null, "pager": null}` 时。

---

### 21. graph 模块测试仍用独立的 setup_test_db + insert_page

严重程度: 无 — 测试代码冗余

src/graph/mod.rs 测试部分没有使用 test_utils.rs 的 sample_page, 仍保留了自己的 setup_test_db 和 insert_page 辅助函数。
index/sqlite.rs 和 ops/page.rs 已经改用 test_utils, 但 graph 模块没改。

---

### 22. sync 的 N+1 查询模式

严重程度: 低 — 大 vault 时性能差

src/ops/sync.rs:38-118:
```
1. list_notes(dir) → 获取当前目录的文件列表
2. 对每个文件: get_note(file_path) → 获取内容 (逐个请求)
3. 对每个文件: parse_page + index_page + update_links
4. list_folders(dir) → 获取子目录列表
5. 递归进入每个子目录
```

每同步一个文件需要一次独立的 HTTP 请求。1000 个文件 = 1000 次 HTTP 调用。
FNS 有 `/api/notes` 端点可以一次性列出所有笔记 (带分页), 但没有批量获取内容的 API。

影响: 首次 sync 大 vault 会很慢。后续 sync 因为有 content_hash 跳过未变更的文件, 影响较小。

---

## 总结

| 编号 | 问题                         | 严重程度 | 类型     | 状态   |
|------|------------------------------|----------|----------|--------|
| 1    | FNS API 路径不兼容            | 致命     | 功能缺陷 | 已修复 |
| 2    | FNS 响应码反转 + 认证头错误    | 致命     | 功能缺陷 | 已修复 |
| 3    | path_to_slug 丢失路径信息     | 高       | 功能缺陷 | 已修复 |
| 4    | slug 验证与文件名不兼容        | 高       | 功能缺陷 | 已修复 |
| 5    | retry 多执行一次请求           | 低       | 逻辑瑕疵 | 未修复 |
| 6    | graph/index 链接查询重复       | 低       | 代码冗余 | 已修复 |
| 7    | MCP 缺少 append 操作          | 中       | 功能缺失 | 已修复 |
| 8    | MCP 缺少 Resources 支持       | —        | 不修复   | —      |
| 9    | MCP 缺少 Prompts 支持         | —        | 不修复   | —      |
| 10   | list_notes 不区分子目录        | 中       | 功能缺陷 | 已修复 |
| 11   | sync 不递归子目录              | 中       | 功能缺陷 | 已修复 |
| 12   | PageType 手动 match 冗余       | 无       | 代码冗余 | 未修复 |
| 13   | parse_link_row 逻辑重复        | 无       | 代码冗余 | 已修复 |
| 15   | 测试工厂函数重复               | 无       | 代码冗余 | 已修复 |
| 16   | handle_page_list 默认目录用 "/" | 中      | 功能缺陷 | 新发现 |
| 17   | URL query 参数未编码           | 中       | 功能缺陷 | 新发现 |
| 18   | page.append 不更新本地索引     | 中       | 功能缺陷 | 新发现 |
| 19   | 430 错误未映射为 NotFound      | 低       | 行为瑕疵 | 新发现 |
| 20   | list_notes null list 处理      | 低       | 鲁棒性   | 新发现 |
| 21   | graph 测试未用 test_utils      | 无       | 代码冗余 | 新发现 |
| 22   | sync N+1 查询                  | 低       | 性能     | 新发现 |
