# my_brain 工程约束文档
## 工程蓝图与实现规范

**版本**: v1.0  
**日期**: 2026-05-05  
**目标实现者**: OpenClaw / Claude Code / Cursor Agent  
**语言**: Rust  
**协议**: MCP (Model Context Protocol) stdio Server  

---

## 1. 系统定位与边界

### 1.1 一句话定义
`my_brain` 是一个 **Rust 实现的 MCP stdio Server**，作为 Hermes/Claude/Cursor 等 AI Agent 的**外部持久化大脑**。它只包含**确定性代码**，不做任何 LLM 推理或决策。决策由调用方（Hermes/Claude）通过 Skills 完成，执行由 `my_brain` 完成。

### 1.2 系统边界图

```
Hermes / Claude / Cursor (决策者，含 LLM)
        │
        │ MCP stdio (JSON-RPC 2.0)
        ▼
   my_brain (执行者，零 LLM)
   ├── MCP 协议层
   ├── 服务路由层
   ├── 执行引擎 (确定性代码)
   └── 存储适配层
        │
        ├──→ FNS Server (HTTP API, localhost:9000)
        │      └── Markdown 文件存储 + WebSocket 同步
        │
        ├──→ SQLite (brain.db, FTS5 + sqlite-vec)
        │      └── 全文索引 + 向量索引 + 图谱关系
        │
        └──→ Git (brain-repo/)
               └── 版本历史
```

### 1.3 核心约束（不可违背）

| 约束 | 说明 | 违背后果 |
|------|------|----------|
| **零 LLM** | `my_brain` 内部禁止调用任何 LLM API | 延迟/成本爆炸，责任不清 |
| **零决策** | `my_brain` 不做任何业务判断，只执行指令 | 智能泄漏到执行层 |
| **MCP stdio** | 唯一对外接口是 MCP stdio Server | 跨平台兼容性 |
| **Skills 即 Resources** | Skills 以 Markdown 形式通过 MCP Resources 暴露 | 动态加载，无需重启 |
| **乐观锁** | 所有写操作带 `etag` (SHA256) | 并发冲突保护 |
| **单二进制** | 编译为单个可执行文件 | 部署极简 |

---

## 2. 技术栈（固定，不可更改）

| 层级 | 技术 | 版本/说明 |
|------|------|----------|
| 语言 | Rust | Edition 2021+ |
| 异步运行时 | tokio | full feature |
| MCP 协议 | 手写 JSON-RPC | 无需第三方 crate |
| 序列化 | serde + serde_json | |
| HTTP 客户端 | reqwest | 用于调用 FNS API |
| SQLite | sqlx | async, migration, compile-time checked |
| SQLite 扩展 | FTS5 | 内置，无需安装 |
| 向量搜索 | sqlite-vec | cargo crate |
| Markdown 解析 | pulldown-cmark | frontmatter 需自行解析 |
| YAML frontmatter | serde_yaml | |
| Git 操作 | git2 | libgit2 绑定 |
| 日志 | tracing + tracing-subscriber | |
| CLI | clap | v4 |

---

## 3. 目录结构（工程规范）

```
my_brain/
├── Cargo.toml
├── brain-repo/                  # Git 管理的人类可读知识库
│   ├── .git/
│   ├── skills/                  # MCP Resources 来源
│   │   ├── universal/
│   │   │   ├── schema.md
│   │   │   ├── purpose.md
│   │   │   └── conventions.md
│   │   ├── hermes/
│   │   │   ├── ingest.md
│   │   │   ├── query.md
│   │   │   ├── enrich.md
│   │   │   └── maintain.md
│   │   └── claude/
│   │       └── CLAUDE.md
│   ├── wiki/                    # 实际知识内容
│   │   ├── index.md
│   │   ├── log.md
│   │   ├── overview.md
│   │   ├── entities/
│   │   ├── concepts/
│   │   ├── sources/
│   │   ├── queries/
│   │   ├── synthesis/
│   │   └── comparisons/
│   ├── raw/                     # 原始资料
│   │   ├── queue/               # 待处理
│   │   ├── processed/           # 已处理 (SHA256 命名)
│   │   └── failed/              # 失败重试
│   └── .obsidian/               # Obsidian 配置 (可选)
├── brain.db                     # SQLite 索引 (Git ignore)
├── src/
│   ├── main.rs                  # CLI + 启动 MCP Server
│   ├── mcp/
│   │   ├── protocol.rs          # JSON-RPC 2.0 基础
│   │   ├── server.rs            # MCP Server 生命周期
│   │   ├── tools.rs             # Tool 注册与分发
│   │   └── resources.rs         # Resource 扫描与读取
│   ├── services/                # 确定性执行引擎
│   │   ├── mod.rs
│   │   ├── ingest.rs            # brain_ingest
│   │   ├── query.rs             # brain_query
│   │   ├── enrich.rs            # brain_enrich
│   │   ├── maintain.rs          # brain_maintain
│   │   └── sync.rs              # brain_sync
│   ├── storage/                 # 存储适配
│   │   ├── mod.rs
│   │   ├── fns.rs               # FNS HTTP API 客户端
│   │   ├── sqlite.rs            # brain.db 管理
│   │   └── git.rs               # brain-repo git 操作
│   ├── search/                  # 搜索算法
│   │   ├── mod.rs
│   │   ├── keyword.rs           # FTS5 搜索
│   │   ├── vector.rs            # sqlite-vec 搜索
│   │   └── hybrid.rs            # RRF 四信号融合
│   └── models/                  # 数据模型
│       ├── mod.rs
│       ├── page.rs              # CompiledTruth + Timeline
│       ├── frontmatter.rs       # YAML 解析
│       ├── link.rs              # Wikilink 图谱
│       └── search.rs            # SearchResult, RRF 分数
└── migrations/                  # sqlx 数据库迁移
    ├── 001_init.sql
    └── 002_fts5_vec.sql
```

---

## 4. 数据模型详细定义

### 4.1 Markdown 页面格式（Compiled Truth + Timeline）

每个 `.md` 文件必须严格遵循以下结构：

```markdown
---
type: entity | concept | source | query | synthesis | comparison
title: 人类可读标题
tags: [tag1, tag2]
related: ["entities/fns", "concepts/llm-wiki"]      # wikilink 路径，无 .md 后缀
sources: ["2026-05-05-rss-tech"]                    # 原始资料 ID（source 类型必填）
date: 2026-05-05
status: seedling | budding | evergreen               # 可选，知识成熟度
---

# 页面标题

这是 Compiled Truth 区域。
当前对主题的最佳理解。可以被新证据整体重写。
使用 `[[wikilink]]` 做交叉引用。

关键点：
1. ...
2. ...

---

- 2026-05-01: [来源](链接) 初始记录：xxx
- 2026-05-03: [来源](链接) 新发现：yyy，与之前矛盾
```

**解析规则**：
- 第一个 `---` 到第二个 `---` 之间是 YAML frontmatter
- 第二个 `---` 之后到文件末尾第二个 `---`（或文件结束）是 **Compiled Truth**
- 最后一个 `---` 之后是 **Timeline**（bullet list，只追加）
- 如果没有第二个 `---`，整个正文都是 Compiled Truth，无 Timeline

### 4.2 SQLite Schema

```sql
-- migrations/001_init.sql

-- 页面主表
CREATE TABLE pages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    slug TEXT NOT NULL UNIQUE,          -- 相对路径，如 "wiki/entities/fns"
    title TEXT NOT NULL,
    type TEXT NOT NULL,                  -- entity/concept/source/query/synthesis/comparison
    vault TEXT NOT NULL DEFAULT 'forge',
    content_hash TEXT NOT NULL,          -- SHA256(content)
    compiled_truth TEXT,                  -- 提取的 CT 文本
    timeline TEXT,                       -- 提取的 Timeline 文本
    frontmatter TEXT NOT NULL,           -- 原始 YAML JSON
    sources JSON,                        -- 解析后的 sources 数组
    tags JSON,
    related JSON,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- 全文搜索虚拟表
CREATE VIRTUAL TABLE pages_fts USING fts5(
    slug,
    title,
    compiled_truth,
    timeline,
    content='pages',
    content_rowid='id'
);

-- 全文搜索触发器：自动同步
CREATE TRIGGER pages_fts_insert AFTER INSERT ON pages BEGIN
    INSERT INTO pages_fts(rowid, slug, title, compiled_truth, timeline)
    VALUES (new.id, new.slug, new.title, new.compiled_truth, new.timeline);
END;

CREATE TRIGGER pages_fts_delete AFTER DELETE ON pages BEGIN
    INSERT INTO pages_fts(pages_fts, rowid, slug, title, compiled_truth, timeline)
    VALUES ('delete', old.id, old.slug, old.title, old.compiled_truth, old.timeline);
END;

CREATE TRIGGER pages_fts_update AFTER UPDATE ON pages BEGIN
    INSERT INTO pages_fts(pages_fts, rowid, slug, title, compiled_truth, timeline)
    VALUES ('delete', old.id, old.slug, old.title, old.compiled_truth, old.timeline);
    INSERT INTO pages_fts(rowid, slug, title, compiled_truth, timeline)
    VALUES (new.id, new.slug, new.title, new.compiled_truth, new.timeline);
END;

-- 链接图谱表
CREATE TABLE links (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    source_slug TEXT NOT NULL,           -- 出发页面
    target_slug TEXT NOT NULL,           -- 目标页面
    link_type TEXT DEFAULT 'link',       -- link / works_at / attended / author / etc.
    context_snippet TEXT,                -- 链接周围文本
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(source_slug, target_slug, link_type)
);

CREATE INDEX idx_links_source ON links(source_slug);
CREATE INDEX idx_links_target ON links(target_slug);

-- 向量搜索表 (sqlite-vec)
CREATE VIRTUAL TABLE page_embeddings USING vec0(
    embedding float[1536]                 -- OpenAI text-embedding-3-small
);

-- migrations/002_fts5_vec.sql
-- 可选：添加辅助表用于 RRF 中间结果
CREATE TABLE search_cache (
    query_hash TEXT PRIMARY KEY,          -- SHA256(query)
    query TEXT NOT NULL,
    results JSON,                         -- 缓存的搜索结果
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);
```

### 4.3 内存模型 (Rust struct)

```rust
// models/page.rs
pub struct Page {
    pub slug: String,                     // "wiki/entities/fns"
    pub vault: String,
    pub frontmatter: Frontmatter,
    pub compiled_truth: String,
    pub timeline: Vec<TimelineEntry>,
    pub content_hash: String,             // SHA256
    pub raw_content: String,              // 完整原始 markdown
}

pub struct Frontmatter {
    pub r#type: PageType,                  // enum
    pub title: String,
    pub tags: Vec<String>,
    pub related: Vec<String>,             // ["entities/fns"]
    pub sources: Vec<String>,             // ["2026-05-05-rss"]
    pub date: NaiveDate,
    pub status: Option<PageStatus>,
}

pub struct TimelineEntry {
    pub date: NaiveDate,
    pub source_url: Option<String>,
    pub content: String,
}

pub enum PageType {
    Entity, Concept, Source, Query, Synthesis, Comparison,
}

// models/link.rs
pub struct Link {
    pub source_slug: String,
    pub target_slug: String,
    pub link_type: String,                // "link", "works_at", "attended", "author"
    pub context_snippet: Option<String>,
}

// models/search.rs
pub struct SearchResult {
    pub slug: String,
    pub title: String,
    pub compiled_truth_preview: String,   // 前 500 字符
    pub score: f64,                        // RRF 融合分
    pub signals: SearchSignals,            // 各信号原始分
}

pub struct SearchSignals {
    pub keyword_rank: Option<usize>,      // FTS5 排名
    pub vector_rank: Option<usize>,       // 向量排名
    pub direct_link: bool,                // 查询页是否直接链接到此页
    pub source_overlap: usize,              // 共享 sources 数量
    pub common_neighbors: usize,          // 共享 wikilink 目标数
    pub type_affinity: f64,               // 类型亲和系数
}
```

---

## 5. MCP 协议实现规范

### 5.1 通信层

- **Transport**: stdio (stdin/stdout)
- **Format**: JSON-RPC 2.0
- **编码**: UTF-8, 每行一个 JSON 对象，以 `\n` 分隔
- **双向**: Client → Server (requests), Server → Client (notifications/responses)

### 5.2 必需实现的 MCP 方法

| 方法 | 方向 | 说明 |
|------|------|------|
| `initialize` | C→S | 初始化握手，返回 Server 信息 |
| `notifications/initialized` | C→S | 初始化完成通知 |
| `tools/list` | C→S | 返回可用 Tools 列表 |
| `tools/call` | C→S | 调用指定 Tool |
| `resources/list` | C→S | 返回可用 Resources 列表 |
| `resources/read` | C→S | 读取指定 Resource 内容 |
| `prompts/list` | C→S | 返回可用 Prompts (可选，可空) |
| `ping` | C→S | 心跳检测 |

### 5.3 JSON-RPC 消息格式示例

**Client → Server: 初始化**
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "initialize",
  "params": {
    "protocolVersion": "2024-11-05",
    "capabilities": {
      "sampling": {},
      "roots": { "listChanged": true }
    },
    "clientInfo": { "name": "hermes", "version": "0.8.0" }
  }
}
```

**Server → Client: 初始化响应**
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "protocolVersion": "2024-11-05",
    "capabilities": {
      "tools": { "listChanged": false },
      "resources": { "listChanged": true, "subscribe": false }
    },
    "serverInfo": { "name": "my_brain", "version": "0.1.0" }
  }
}
```

**Client → Server: 调用 Tool**
```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "tools/call",
  "params": {
    "name": "brain_get",
    "arguments": {
      "slug": "wiki/index.md",
      "vault": "forge"
    }
  }
}
```

**Server → Client: Tool 响应**
```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "result": {
    "content": [
      {
        "type": "text",
        "text": "{\"slug\":\"wiki/index.md\",\"title\":\"Wiki Index\",\"content\":\"# Wiki Index\\n\\n...\"}"
      }
    ],
    "isError": false
  }
}
```

**错误响应格式**
```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "error": {
    "code": -32602,
    "message": "Invalid params: missing 'slug'",
    "data": { "field": "slug" }
  }
}
```

### 5.4 Server 启动流程

```rust
// main.rs 伪代码
#[tokio::main]
async fn main() {
    let args = Cli::parse();
    
    match args.command {
        Commands::Serve { config } => {
            let brain = Brain::load(config).await?;
            let server = MyBrainMcpServer::new(brain);
            
            // stdio 监听
            let stdin = tokio::io::stdin();
            let stdout = tokio::io::stdout();
            server.run(stdin, stdout).await?;
        }
    }
}
```

**启动后行为**：
1. 等待 `initialize` 请求
2. 响应 capabilities (tools + resources)
3. 收到 `notifications/initialized` 后，开始处理 tools/call 和 resources/read
4. 后台线程：每分钟扫描 `brain-repo/skills/` 目录，如果文件变更，发送 `notifications/resources/list_changed`

---

## 6. Tool 详细规范

### 6.1 完整 Tool 列表

| Tool 名 | 作用域 | 说明 |
|---------|--------|------|
| `brain_get` | read | 读取页面完整内容 |
| `brain_put` | write | 创建/覆盖页面（带乐观锁） |
| `brain_append` | write | 在 Timeline 末尾追加条目 |
| `brain_delete` | write | 删除页面（软删除，移入 .archive/） |
| `brain_search` | read | 关键词搜索 (FTS5) |
| `brain_query` | read | 混合搜索 (RRF 四信号融合) |
| `brain_enrich` | write | 提取页面链接，更新图谱，丰富实体 |
| `brain_maintain` | write | 运行维护检查 (lint + 孤儿 + 矛盾) |
| `brain_sync` | admin | 手动触发 git→SQLite 同步 |
| `brain_stats` | read | 返回大脑统计信息 |

### 6.2 brain_get

**输入参数** (JSON Schema):
```json
{
  "type": "object",
  "properties": {
    "slug": { "type": "string", "description": "页面路径，如 'wiki/index.md' 或 'wiki/entities/fns'" },
    "vault": { "type": "string", "default": "forge" }
  },
  "required": ["slug"]
}
```

**输出**:
```json
{
  "slug": "wiki/index.md",
  "vault": "forge",
  "title": "Wiki Index",
  "type": "overview",
  "frontmatter": { "type": "overview", "title": "Wiki Index", "date": "2026-05-05" },
  "compiled_truth": "# Wiki Index\n\n...",
  "timeline": ["2026-05-01: 初始创建"],
  "content_hash": "sha256:a1b2c3...",
  "etag": "sha256:a1b2c3...",          // 用于乐观锁
  "related_pages": ["wiki/entities/fns", "wiki/concepts/llm-wiki"],
  "backlinks": ["wiki/synthesis/retro"],
  "created_at": "2026-05-01T00:00:00Z",
  "updated_at": "2026-05-05T12:00:00Z"
}
```

**副作用**: 无
**错误码**:
- `404`: 页面不存在
- `400`: slug 格式非法 (必须匹配 `^[a-z0-9/_-]+\.md$`)

### 6.3 brain_put

**输入参数**:
```json
{
  "type": "object",
  "properties": {
    "slug": { "type": "string" },
    "vault": { "type": "string", "default": "forge" },
    "content": { "type": "string", "description": "完整 Markdown 内容，含 frontmatter" },
    "etag": { "type": "string", "description": "乐观锁: 预期的 content_hash，省略则跳过检查" }
  },
  "required": ["slug", "content"]
}
```

**处理逻辑**:
1. 校验 slug 格式 (`^[a-z0-9/_-]+\.md$`)
2. 解析 frontmatter (YAML)，校验必需字段：`type`, `title`, `date`
3. 计算新 content 的 SHA256
4. 如果提供了 `etag`:
   - 读取 FNS 上当前文件，计算其 SHA256
   - 如果不等于 `etag`，返回 `409 Conflict` (乐观锁失败)
5. 调用 FNS API `POST /api/note` (或 PUT，如果已存在)
6. 解析 `compiled_truth` 和 `timeline`，更新 SQLite `pages` 表
7. 提取 wikilink，更新 `links` 表
8. 如果 `type == "source"`，更新 sources 关联
9. 返回新页面的完整信息 + 新 `etag`

**输出**:
```json
{
  "slug": "wiki/sources/2026-05-05-rss.md",
  "vault": "forge",
  "title": "RSS Digest 2026-05-05",
  "content_hash": "sha256:newhash...",
  "etag": "sha256:newhash...",
  "created": false,                  // true=新建, false=覆盖
  "indexed": true,
  "links_extracted": 3
}
```

**错误码**:
- `400`: 参数错误或 frontmatter 缺失
- `409`: 乐观锁冲突 (etag 不匹配)
- `500`: FNS API 调用失败

### 6.4 brain_append

**输入参数**:
```json
{
  "type": "object",
  "properties": {
    "slug": { "type": "string" },
    "vault": { "type": "string", "default": "forge" },
    "timeline_entry": { "type": "string", "description": "Timeline 条目文本，不含日期前缀" },
    "date": { "type": "string", "description": "条目日期，默认今天" }
  },
  "required": ["slug", "timeline_entry"]
}
```

**处理逻辑**:
1. `brain_get(slug)` 读取当前内容
2. 在最后一个 `---` 之后的 Timeline 区域追加条目
3. 条目格式：`- YYYY-MM-DD: {timeline_entry}`
4. 调用 `brain_put` 写回（不带 etag 或强制覆盖）

**输出**: 同 brain_put

### 6.5 brain_search (FTS5 关键词)

**输入参数**:
```json
{
  "type": "object",
  "properties": {
    "query": { "type": "string", "description": "搜索关键词" },
    "limit": { "type": "integer", "default": 20, "maximum": 100 },
    "type_filter": { "type": "string", "description": "可选: entity/concept/source/..." }
  },
  "required": ["query"]
}
```

**实现**:
```sql
SELECT slug, title, compiled_truth, rank
FROM pages_fts
WHERE pages_fts MATCH ?
  AND (?1 IS NULL OR type = ?1)
ORDER BY rank
LIMIT ?2;
```

**输出**:
```json
{
  "query": "FNS sync protocol",
  "total": 3,
  "results": [
    {
      "slug": "wiki/entities/fns.md",
      "title": "FNS",
      "preview": "FNS (Fast Note Sync) is a real-time note synchronization service...",
      "rank": 1
    }
  ]
}
```

### 6.6 brain_query (RRF 混合搜索)

**输入参数**:
```json
{
  "type": "object",
  "properties": {
    "query": { "type": "string" },
    "limit": { "type": "integer", "default": 10 },
    "from_slugs": { "type": "array", "items": { "type": "string" }, "description": "已知相关页面，用于图谱信号增强" }
  },
  "required": ["query"]
}
```

**处理逻辑** (见第 8 节 RRF 详细算法):
1. 对 `query` 做 FTS5 搜索，取前 `limit * 2` 结果
2. 对 `query` 做向量搜索（如果 sqlite-vec 可用），取前 `limit * 2` 结果
3. 如果 `from_slugs` 提供，计算四信号图谱关联度
4. RRF 融合，取前 `limit` 结果
5. 对每个结果读取 Compiled Truth 预览

**输出**:
```json
{
  "query": "FNS 同步",
  "total": 10,
  "results": [
    {
      "slug": "wiki/entities/fns.md",
      "title": "FNS",
      "compiled_truth_preview": "...",
      "score": 0.0517,
      "signals": {
        "keyword_rank": 1,
        "vector_rank": 2,
        "direct_link": true,
        "source_overlap": 1,
        "common_neighbors": 3,
        "type_affinity": 1.0
      }
    }
  ]
}
```

### 6.7 brain_enrich

**输入参数**:
```json
{
  "type": "object",
  "properties": {
    "slug": { "type": "string", "description": "要丰富的页面" },
    "depth": { "type": "integer", "default": 1, "description": "递归深度 (1=只处理当前页, 2=处理关联页)" }
  },
  "required": ["slug"]
}
```

**处理逻辑** (零 LLM):
1. 读取页面内容
2. 正则提取所有 `[[wikilink]]` → 创建/更新 `links` 表
3. 提取裸 slug 引用（如 `fns` 在 `[[fns|Fast Note Sync]]` 中）→ 标准化为 `entities/fns`
4. 对于每个 target_slug:
   - 如果不存在对应页面，在 `reviews/` 创建 stub 记录
   - 更新 backlink 计数
5. 对于 `type == "source"` 的页面:
   - 提取 `sources` frontmatter
   - 反向更新被引用页面的 `sources` 字段（通过 SQLite 关联查询）
6. 返回：创建了多少链接、多少个 stub、多少个 backlink 更新

**输出**:
```json
{
  "slug": "wiki/sources/2026-05-05-rss.md",
  "links_created": 5,
  "stubs_created": 2,
  "backlinks_updated": 3,
  "entities_found": ["fns", "affine", "n8n"]
}
```

### 6.8 brain_maintain

**输入参数**:
```json
{
  "type": "object",
  "properties": {
    "scope": { "type": "string", "enum": ["lint", "orphans", "backlinks", "full"], "default": "full" }
  }
}
```

**处理逻辑**:
- `lint`: 扫描所有页面，检查 frontmatter 完整性、YAML 语法、slug 命名规范
- `orphans`: 查找 `links` 表中 target_slug 不存在对应 `pages` 记录的行
- `backlinks`: 确保每个页面的 backlink 计数正确
- `full`: 执行所有检查 + 生成 `reviews/maintain-{timestamp}.md` 报告

**输出**:
```json
{
  "scope": "full",
  "issues_found": 7,
  "report_slug": "reviews/maintain-2026-05-05-120000.md",
  "details": {
    "missing_frontmatter": 1,
    "orphan_links": 3,
    "broken_backlinks": 2,
    "naming_violations": 1
  }
}
```

### 6.9 brain_sync

**输入参数**: `{}` (空对象)

**处理逻辑**:
1. `git status --short` 检查 brain-repo/ 变更
2. 对每个变更文件:
   - 如果是新增/修改的 `.md` → 解析 → INSERT/UPDATE `pages` 表
   - 如果是删除的 `.md` → DELETE `pages` 表 + `links` 表清理
3. 对变更页面重新提取链接 → 更新 `links` 表
4. 对变更页面重新生成 embedding → 更新 `page_embeddings` 表
5. 返回变更统计

**输出**:
```json
{
  "files_changed": 5,
  "pages_indexed": 3,
  "pages_removed": 1,
  "links_updated": 7,
  "embeddings_refreshed": 3
}
```

### 6.10 brain_stats

**输出**:
```json
{
  "total_pages": 1523,
  "by_type": { "entity": 45, "concept": 23, "source": 1200, "synthesis": 255 },
  "total_links": 3402,
  "orphan_pages": 12,
  "db_size_mb": 45.2,
  "last_sync": "2026-05-05T11:00:00Z"
}
```

---

## 7. MCP Resources 规范

### 7.1 Resource URI 格式

```
skills://{scope}/{filename}       → brain-repo/skills/{scope}/{filename}.md
pages://{slug}                   → brain-repo/{slug} (读取原始 Markdown)
log://latest                     → brain-repo/wiki/log.md 最后 50 行
```

### 7.2 list_resources 实现

```rust
fn list_resources(&self) -> Vec<Resource> {
    let mut resources = vec![];
    
    // 扫描 skills/ 目录
    for entry in walkdir::WalkDir::new(&self.skills_dir) {
        let path = entry.path();
        if path.extension() == Some("md") {
            let rel = path.strip_prefix(&self.brain_repo).unwrap();
            resources.push(Resource {
                uri: format!("skills://{}", rel.to_string_lossy().replace("skills/", "").replace(".md", "")),
                name: path.file_stem().unwrap().to_string_lossy().to_string(),
                mime_type: "text/markdown".to_string(),
                description: Some(extract_first_heading(path)),
            });
        }
    }
    resources
}
```

### 7.3 read_resource 实现

```rust
fn read_resource(&self, uri: &str) -> Result<String> {
    if uri.starts_with("skills://") {
        let rel_path = uri.trim_start_matches("skills://");
        let full = self.brain_repo.join("skills").join(format!("{}.md", rel_path));
        fs::read_to_string(full)
    } else if uri.starts_with("pages://") {
        let slug = uri.trim_start_matches("pages://");
        let full = self.brain_repo.join(slug);
        fs::read_to_string(full)
    } else {
        Err(Error::InvalidResourceUri)
    }
}
```

---

## 8. 搜索算法详细规范（四信号 RRF）

### 8.1 信号定义

| 信号 | 权重 | 计算方式 | 代码实现 |
|------|------|---------|---------|
| **Keyword Rank** | 1.0 (基线) | FTS5 `rank` 函数返回的顺序 | `1.0 / (60 + fts5_rank)` |
| **Vector Rank** | 1.0 (基线) | sqlite-vec `distance` 排序 | `1.0 / (60 + vec_rank)` |
| **Direct Link** | 3.0 | `from_slugs` 中任一页面是否通过 `[[slug]]` 直接链接到结果页 | SQLite: `EXISTS(SELECT 1 FROM links WHERE source_slug IN (?) AND target_slug = result.slug)` |
| **Source Overlap** | 4.0 | 结果页与 `from_slugs` 页面的 `sources` JSON 数组交集大小 | `4.0 * overlap_count / (60 + rank)` |
| **Common Neighbors** | 1.5 | Adamic-Adar: 共享的 wikilink 目标数，按目标出度加权 | `1.5 * Σ 1/log(degree(target)) / (60 + rank)` |
| **Type Affinity** | 变量 | 预定义矩阵 | `entity↔entity=1.2, concept↔concept=1.2, source↔source=0.5, entity↔concept=1.0, ...` |

### 8.2 RRF 公式

```rust
const RRF_K: f64 = 60.0;

fn rrf_score(rank: usize, weight: f64) -> f64 {
    weight / (RRF_K + rank as f64)
}

fn hybrid_score(result: &Candidate, query_context: &QueryContext) -> f64 {
    let mut score = 0.0;
    
    // 1. Keyword
    if let Some(rank) = result.keyword_rank {
        score += rrf_score(rank, 1.0);
    }
    
    // 2. Vector
    if let Some(rank) = result.vector_rank {
        score += rrf_score(rank, 1.0);
    }
    
    // 3. Direct Link
    if query_context.from_slugs.iter().any(|s| has_direct_link(s, &result.slug)) {
        score += rrf_score(1, 3.0);  // 直接链接排第 1
    }
    
    // 4. Source Overlap
    let overlap = count_source_overlap(&query_context.from_slugs, &result.slug);
    if overlap > 0 {
        score += rrf_score(1, 4.0 * overlap as f64);
    }
    
    // 5. Common Neighbors (简化版: 共享 wikilink 数)
    let common = count_common_neighbors(&query_context.from_slugs, &result.slug);
    if common > 0 {
        score += rrf_score(1, 1.5 * common as f64);
    }
    
    // 6. Type Affinity
    score *= type_affinity(result.page_type, query_context.preferred_type);
    
    score
}
```

### 8.3 查询执行流程

```rust
pub async fn query(&self, q: &QueryRequest) -> Result<QueryResponse> {
    // 1. 多查询扩展 (可选, 由调用方决定是否扩展)
    // 这里不做 LLM 扩展, 只做字面查询
    
    // 2. 并行执行两个搜索
    let (keyword_hits, vector_hits) = tokio::join!(
        self.keyword_search(&q.query, q.limit * 2),
        self.vector_search(&q.query, q.limit * 2),
    );
    
    // 3. 合并候选集
    let mut candidates: HashMap<String, Candidate> = HashMap::new();
    for (rank, hit) in keyword_hits.iter().enumerate() {
        candidates.entry(hit.slug.clone())
            .or_default()
            .keyword_rank = Some(rank + 1);
    }
    for (rank, hit) in vector_hits.iter().enumerate() {
        candidates.entry(hit.slug.clone())
            .or_default()
            .vector_rank = Some(rank + 1);
    }
    
    // 4. 计算四信号 (需要查询上下文)
    let context = QueryContext::from_slugs(&q.from_slugs);
    let mut scored: Vec<_> = candidates.into_values()
        .map(|c| (c.slug.clone(), hybrid_score(&c, &context), c))
        .collect();
    
    // 5. 排序取 Top-N
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    let top = scored.into_iter().take(q.limit).collect();
    
    // 6. 读取 Compiled Truth 预览
    let results = self.load_previews(top).await?;
    
    Ok(QueryResponse { query: q.query.clone(), results })
}
```

### 8.4 Context Budget（查询时）

当 `brain_query` 被调用时，返回的结果需要遵守 Context Budget，但 **预算控制由调用方（Hermes）负责**。`my_brain` 只需提供：

- `compiled_truth_preview`: 前 500 字符（可配置）
- 不返回完整 `timeline`（减少 token）
- 返回 `signals` 明细（帮助调用方判断相关性）

---

## 9. FNS 集成契约

### 9.1 HTTP 客户端配置

```rust
pub struct FnsClient {
    base_url: String,      // "http://localhost:9000"
    token: String,         // Authorization header value
    default_vault: String, // "forge"
    client: reqwest::Client,
}
```

### 9.2 必需调用的 FNS API

| FNS API | my_brain 用途 | Rust 函数 |
|---------|-------------|-----------|
| `POST /api/user/login` | 初始化时获取 token | `authenticate()` |
| `GET /api/note?path=&vault=` | `brain_get` | `get_note(path, vault)` |
| `POST /api/note` | `brain_put` (创建/覆盖) | `put_note(path, vault, content, fm)` |
| `POST /api/note/append` | `brain_append` | `append_note(path, vault, content)` |
| `DELETE /api/note?path=&vault=` | `brain_delete` | `delete_note(path, vault)` |
| `GET /api/note/list?vault=&path=&page=&limit=` | `brain_search` 辅助 | `list_notes(vault, path, limit)` |
| `GET /api/folder/files?vault=&path=` | 扫描目录 | `list_folder(vault, path)` |

### 9.3 FNS 请求格式细节

**认证 Header**:
```
Authorization: eyJhbGciOiJIUzI1NiIs...    (纯 token, 无 Bearer 前缀)
Content-Type: application/json
```

**PUT /api/note 请求体**:
```json
{
  "path": "wiki/entities/fns.md",
  "vault": "forge",
  "content": "# FNS\n\nFast Note Sync...",
  "frontmatter": {
    "type": "entity",
    "title": "FNS",
    "date": "2026-05-05",
    "tags": ["sync", "obsidian"]
  }
}
```

**错误处理**:
- FNS 返回 `code != 1` → 映射为 MCP `500` error
- FNS 返回网络超时 → 重试 3 次（指数退避: 100ms, 300ms, 900ms）
- FNS 返回 `308` (登录失效) → 自动重新 login 获取新 token → 重试原请求

---

## 10. Git 同步策略

### 10.1 自动同步触发条件

| 触发源 | 行为 |
|--------|------|
| `brain_put` 成功 | 立即 `git add {file} && git commit -m "brain: {slug}"` |
| `brain_sync` 调用 | 全量扫描 `git status` → 同步所有变更到 SQLite |
| cron (可选) | 每分钟检查 `git status`，如果有变更自动 sync |

### 10.2 Git 提交规范

```
brain: wiki/entities/fns.md           # 单文件写入
brain: batch 3 pages (ingest)         # 批量写入
maintain: auto-fix 7 issues           # 维护脚本
sync: 5 files changed                 # 外部同步（如 Obsidian 编辑）
```

### 10.3 冲突解决

如果 `brain-repo/` 在 Obsidian 端和 my_brain 端同时修改：
1. my_brain 写入时先 `git pull --rebase`
2. 如果冲突，以 my_brain 版本为准（强制覆盖），但记录冲突到 `reviews/sync-conflicts-{date}.md`
3. 通知调用方（通过 MCP notification）

---

## 11. 构建与部署

### 11.1 Cargo.toml 关键依赖

```toml
[package]
name = "my_brain"
version = "0.1.0"
edition = "2021"

[dependencies]
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
serde_yaml = "0.9"
reqwest = { version = "0.12", features = ["json"] }
sqlx = { version = "0.7", features = ["runtime-tokio", "sqlite"] }
pulldown-cmark = "0.11"
git2 = "0.19"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
clap = { version = "4", features = ["derive"] }
sha2 = "0.10"
hex = "0.4"
walkdir = "2"
chrono = { version = "0.4", features = ["serde"] }
thiserror = "1"
anyhow = "1"

# sqlite-vec 需要确认 crate 名称和兼容性
# 如果不可用，初期可仅用 FTS5，后期手动集成向量
```

### 11.2 编译与运行

```bash
# 开发
cargo run -- serve --config brain-repo/

# 生产
cargo build --release
./target/release/my_brain serve --config /opt/brain-repo/

# Hermes 配置
# ~/.hermes/config.yaml
mcp_servers:
  my_brain:
    command: /usr/local/bin/my_brain
    args: ["serve", "--config", "/opt/brain-repo"]
```

---

## 12. 测试要求

### 12.1 单元测试（必须覆盖）

| 模块 | 测试内容 |
|------|---------|
| `frontmatter.rs` | YAML 解析/序列化 roundtrip，缺失字段报错 |
| `page.rs` | Compiled Truth / Timeline 分割边界条件 |
| `link.rs` | `[[slug]]`、`[[slug|text]]`、裸 slug 提取 |
| `protocol.rs` | JSON-RPC parse/serialize，id 匹配 |
| `tools.rs` | 每个 tool 的输入校验、错误码 |
| `hybrid.rs` | RRF 融合数学正确性，相同 rank 的处理 |
| `fns.rs` | Mock HTTP server，超时重试逻辑 |

### 12.2 集成测试

```bash
# 启动测试用的 FNS + my_brain
cargo test --test integration

# 测试场景:
# 1. brain_put → brain_get roundtrip
# 2. brain_search 返回正确排序
# 3. brain_enrich 后 links 表正确
# 4. brain_sync 后 git log 和 SQLite 一致
```

### 12.3 MCP 兼容性测试

使用官方 MCP Inspector:
```bash
npx @modelcontextprotocol/inspector node dist/stdio-client.js
```

---

## 13. 风险与回退方案

| 风险 | 回退方案 |
|------|---------|
| sqlite-vec 不稳定 | 初期禁用向量搜索，仅用 FTS5 + 四信号图谱 |
| FNS API 变更 | FNS 客户端封装抽象层，变更只改 `storage/fns.rs` |
| MCP 协议升级 | protocol.rs 独立模块，升级只改一处 |
| Git 冲突频繁 | 关闭 Obsidian 双向编辑，Obsidian 只读 |
| 性能瓶颈 (大量页面) | SQLite 加 WAL 模式，搜索加缓存层 |

---

## 14. 给 OpenClaw/Claude Code 的实现指令

**指令**:
> 按此文档实现 `my_brain` Rust 项目。先实现 MCP 协议层 + `brain_get`/`brain_put` 两个 tool，验证与 FNS 的连通性。然后依次实现 `brain_search`、`brain_query`、`brain_enrich`、`brain_maintain`、`brain_sync`。最后实现 Resources 暴露和 Git 同步。每个 Tool 必须有单元测试。使用 `sqlx migrate` 管理数据库。编译为单个二进制文件。

**验证标准**:
1. `cargo test` 全绿
2. Hermes/Claude 连接后，`brain_get("wiki/index.md")` 返回正确 JSON
3. `brain_put` 写入后，Obsidian 内秒同步出现新文件
4. `brain_query("FNS")` 返回包含 `wiki/entities/fns.md` 的结果
5. `brain_sync` 后 `git log` 有提交，且 SQLite 记录数匹配文件数

---

**文档结束。按此施工。**
