# my_brain 工程约束文档 v2.0
## 重构版：存储抽象 + Streamable HTTP MCP + FNS 后端

**版本**: v2.0  
**日期**: 2026-05-05  
**目标实现者**: OpenCode + Oh My OpenAgent  
**语言**: Rust Edition 2024, Nightly Toolchain  
**协议**: MCP (Model Context Protocol) Streamable HTTP Server  

---

## 1. 核心架构变更（与 v1 的差异）

### 1.1 项目不在源码中管理任何知识库文件

```
v1 (已废弃):                    v2 (当前):
my_brain/                       my_brain/
├── brain-repo/                 ├── src/
│   ├── skills/                 ├── Cargo.toml
│   ├── wiki/                   └── migrations/
│   └── raw/                    (无 brain-repo/ 目录)
├── brain.db                    (无 brain.db 文件)
└── src/
```

- **brain-repo/** 由 FNS + Obsidian 管理，不在 my_brain 项目中
- **brain.db** 在运行时指定目录，不在项目源码中

### 1.2 存储层 Trait 抽象

```
┌─────────────────────────────────────────────────────────────┐
│  my_brain (Rust MCP Streamable HTTP Server)                  │
│                                                             │
│  ┌─────────────┐    ┌─────────────────────────────────────┐ │
│  │ MCP Layer   │    │ Storage Layer (Trait)                │ │
│  │             │    │                                     │ │
│  │ Streamable  │───→│  trait FileBackend                  │ │
│  │ HTTP        │    │  ├── FnsBackend (HTTP API)          │ │
│  │             │    │  └── LocalBackend (fs + git)        │ │
│  │ Resources   │    │                                     │ │
│  │ Tools       │    │  trait IndexBackend                 │ │
│  │             │    │  ├── SqliteBackend (FTS5 + vec)     │ │
│  │             │    │  └── MemoryBackend (testing)        │ │
│  └─────────────┘    └─────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────┘
          │
          │ HTTP POST /mcp (Streamable)
          ▼
   ┌──────────────┐         ┌──────────┐
   │  OpenCode    │         │  Claude  │
   │  + Oh My OA  │         │  Desktop │
   └──────────────┘         └──────────┘
```

### 1.3 文件管理完全外包给 FNS

my_brain **不管理**：
- ❌ 文件同步
- ❌ 文件备份
- ❌ Git 版本控制
- ❌ 目录结构维护

my_brain **只做**：
- ✅ 通过 FileBackend trait 读写文件（FNS API 或本地 fs）
- ✅ 通过 IndexBackend trait 维护搜索索引（SQLite）
- ✅ MCP Streamable HTTP Server

---

## 2. 技术栈（固定）

| 层级 | 技术 | 版本/说明 |
|------|------|----------|
| 语言 | Rust | Edition 2024, Nightly toolchain |
| 异步运行时 | tokio | full feature |
| MCP 协议 | 手写 Streamable HTTP | MCP 2024-11-05 规范 |
| HTTP 框架 | axum | v0.8+ |
| HTTP 客户端 | reqwest | 用于调用 FNS API |
| 序列化 | serde + serde_json | |
| SQLite | sqlx | async, migration |
| SQLite FTS5 | 内置 | 全文搜索 |
| 向量搜索 | sqlite-vec | cargo crate |
| Markdown 解析 | pulldown-cmark | |
| YAML frontmatter | serde_yaml | |
| Git 操作 | git2 | 仅 LocalBackend 可选使用 |
| 日志 | tracing + tracing-subscriber | |
| CLI | clap | v4 |
| 配置 | config-rs | TOML + 环境变量 |
| 路径 | dirs | 标准目录定位 |

---

## 3. 目录结构（项目源码）

```
my_brain/
├── Cargo.toml
├── config.toml.example          # 配置文件模板
├── migrations/                  # sqlx 数据库迁移
│   ├── 001_init.sql
│   └── 002_fts5_vec.sql
├── src/
│   ├── main.rs                  # CLI + 配置加载 + 启动
│   ├── config.rs                # 配置结构体（Config）
│   │
│   ├── mcp/
│   │   ├── protocol.rs          # MCP JSON-RPC 基础
│   │   ├── transport.rs         # Streamable HTTP 传输层
│   │   ├── server.rs            # MCP Server 生命周期
│   │   ├── tools.rs             # Tool 注册与分发
│   │   └── resources.rs         # Resource 扫描与读取
│   │
│   ├── storage/                 # 存储层
│   │   ├── mod.rs               # FileBackend + IndexBackend traits
│   │   ├── fns.rs               # FnsBackend 实现
│   │   ├── local.rs             # LocalBackend 实现 (fs + optional git)
│   │   └── sqlite.rs            # SqliteBackend 实现
│   │
│   ├── services/                # 执行引擎
│   │   ├── mod.rs
│   │   ├── ingest.rs
│   │   ├── query.rs
│   │   ├── enrich.rs
│   │   ├── maintain.rs
│   │   └── sync.rs
│   │
│   ├── search/                  # 搜索算法
│   │   ├── mod.rs
│   │   ├── keyword.rs
│   │   ├── vector.rs
│   │   └── hybrid.rs
│   │
│   └── models/
│       ├── mod.rs
│       ├── page.rs
│       ├── frontmatter.rs
│       ├── link.rs
│       └── search.rs
```

**注意**：项目源码中 **无 brain-repo/** 和 **无 brain.db**。

---

## 4. 配置系统（config.toml）

```toml
# config.toml
[server]
host = "0.0.0.0"
port = 8080

[mcp]
# Streamable HTTP 端点路径
endpoint = "/mcp"
# API Key（可选，用于简单认证）
api_key = "your-api-key-here"

[storage]
# 后端类型: "fns" | "local"
backend = "fns"

# FNS 配置（backend = "fns" 时生效）
[storage.fns]
base_url = "http://localhost:9000"
api_token = "eyJhbGciOiJIUzI1NiIs..."
default_vault = "forge"

# Local 配置（backend = "local" 时生效）
[storage.local]
base_path = "/home/user/brain-repo"
enable_git = true

[index]
# SQLite 数据库路径（默认: ~/.local/share/my_brain/brain.db）
db_path = "/var/lib/my_brain/brain.db"
# 向量维度（OpenAI text-embedding-3-small = 1536）
embedding_dim = 1536
```

**配置加载优先级**（从高到低）：
1. 命令行参数 `--config /path/to/config.toml`
2. 环境变量 `MY_BRAIN_CONFIG=/path/to/config.toml`
3. 默认路径 `~/.config/my_brain/config.toml`

---

## 5. 存储层 Trait 定义

### 5.1 FileBackend Trait

```rust
// src/storage/mod.rs

use async_trait::async_trait;

#[async_trait]
pub trait FileBackend: Send + Sync {
    /// 读取文件内容
    async fn get(&self, path: &str) -> Result<String, BackendError>;
    
    /// 写入/覆盖文件
    async fn put(&self, path: &str, content: &str) -> Result<(), BackendError>;
    
    /// 追加内容到文件末尾
    async fn append(&self, path: &str, content: &str) -> Result<(), BackendError>;
    
    /// 删除文件（软删除：移入 .archive/）
    async fn delete(&self, path: &str) -> Result<(), BackendError>;
    
    /// 列出目录下的文件
    async fn list(&self, dir: &str) -> Result<Vec<FileMeta>, BackendError>;
    
    /// 检查文件是否存在
    async fn exists(&self, path: &str) -> Result<bool, BackendError>;
    
    /// 批量移动文件（raw/queue/ → raw/processed/）
    async fn batch_move(&self, moves: &[(String, String)]) -> Result<Vec<String>, BackendError>;
    
    /// 获取文件元信息（mtime, size, hash）
    async fn stat(&self, path: &str) -> Result<FileStat, BackendError>;
}

#[derive(Debug, Clone)]
pub struct FileMeta {
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone)]
pub struct FileStat {
    pub size: u64,
    pub modified: chrono::DateTime<chrono::Utc>,
    pub content_hash: String,  // SHA256
}

#[derive(thiserror::Error, Debug)]
pub enum BackendError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("api error: {status} {message}")]
    Api { status: u16, message: String },
    #[error("auth error: {0}")]
    Auth(String),
}
```

### 5.2 FnsBackend 实现

```rust
// src/storage/fns.rs

pub struct FnsBackend {
    client: reqwest::Client,
    base_url: String,
    api_token: String,
    default_vault: String,
}

#[async_trait]
impl FileBackend for FnsBackend {
    async fn get(&self, path: &str) -> Result<String, BackendError> {
        let resp = self.client
            .get(format!("{}/api/note", self.base_url))
            .query(&[("path", path), ("vault", &self.default_vault)])
            .header("Authorization", &self.api_token)
            .send().await?;
        
        if resp.status() == 404 {
            return Err(BackendError::NotFound(path.to_string()));
        }
        
        let json: FnsResponse = resp.json().await?;
        Ok(json.data.content)
    }
    
    async fn put(&self, path: &str, content: &str) -> Result<(), BackendError> {
        let resp = self.client
            .post(format!("{}/api/note", self.base_url))
            .json(&serde_json::json!({
                "path": path,
                "vault": self.default_vault,
                "content": content
            }))
            .header("Authorization", &self.api_token)
            .send().await?;
        
        if resp.status() == 409 {
            return Err(BackendError::Conflict(path.to_string()));
        }
        
        Ok(())
    }
    
    async fn append(&self, path: &str, content: &str) -> Result<(), BackendError> {
        let resp = self.client
            .post(format!("{}/api/note/append", self.base_url))
            .json(&serde_json::json!({
                "path": path,
                "vault": self.default_vault,
                "content": content
            }))
            .header("Authorization", &self.api_token)
            .send().await?;
        
        Ok(())
    }
    
    // delete, list, exists, batch_move, stat ...
}
```

### 5.3 LocalBackend 实现（预留）

```rust
// src/storage/local.rs

pub struct LocalBackend {
    base_path: PathBuf,
    enable_git: bool,
}

#[async_trait]
impl FileBackend for LocalBackend {
    async fn get(&self, path: &str) -> Result<String, BackendError> {
        let full = self.base_path.join(path);
        tokio::fs::read_to_string(full).await.map_err(Into::into)
    }
    
    async fn put(&self, path: &str, content: &str) -> Result<(), BackendError> {
        let full = self.base_path.join(path);
        if let Some(parent) = full.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(full, content).await?;
        
        if self.enable_git {
            self.git_add_commit(path).await?;
        }
        
        Ok(())
    }
    
    // ... 其他方法
}
```

---

## 6. MCP Streamable HTTP 协议

### 6.1 传输层规范

根据 MCP 2024-11-05 规范，Streamable HTTP 的核心特征：

| 特征 | 说明 |
|------|------|
| **传输** | HTTP POST 请求 |
| **请求格式** | JSON-RPC 2.0 对象放在 HTTP Body |
| **响应格式** | 普通 JSON（同步）或 `text/event-stream`（流式）|
| **会话管理** | 服务端生成 `Mcp-Session-Id`，客户端在后续请求中带回 |
| **SSE 事件** | 使用 `data:` 行格式，但只在需要流式时启用 |

### 6.2 端点设计

```
POST /mcp          ← MCP 主端点（JSON-RPC 请求/响应）
GET  /mcp          ← SSE 订阅端点（服务端推送通知）
```

### 6.3 请求/响应流程

**初始化（非流式）**:
```http
POST /mcp HTTP/1.1
Content-Type: application/json

{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "initialize",
  "params": {
    "protocolVersion": "2024-11-05",
    "capabilities": {},
    "clientInfo": { "name": "opencode", "version": "1.14" }
  }
}
```

```http
HTTP/1.1 200 OK
Content-Type: application/json
Mcp-Session-Id: abc123

{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "protocolVersion": "2024-11-05",
    "capabilities": {
      "tools": {},
      "resources": {}
    },
    "serverInfo": { "name": "my_brain", "version": "0.1.0" }
  }
}
```

**Tool Call（流式）**:
```http
POST /mcp HTTP/1.1
Content-Type: application/json
Mcp-Session-Id: abc123
Accept: text/event-stream

{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "tools/call",
  "params": {
    "name": "brain_query",
    "arguments": { "query": "FNS sync protocol", "limit": 5 }
  }
}
```

```http
HTTP/1.1 200 OK
Content-Type: text/event-stream

id: 2
event: message
data: {"jsonrpc":"2.0","id":2,"result":{"content":[{"type":"text","text":"{\"results\":[...]}"}]}}

event: done
data: {}
```

### 6.4 服务端实现骨架（axum）

```rust
// src/mcp/transport.rs

use axum::{
    routing::{post, get},
    Router, Json, Extension,
    extract::State,
    response::sse::{Event, Sse},
    http::HeaderMap,
};
use std::convert::Infallible;
use tokio::sync::mpsc;

pub struct StreamableHttpTransport {
    sessions: Arc<DashMap<String, Session>>,
}

impl StreamableHttpTransport {
    pub fn router() -> Router {
        Router::new()
            .route("/mcp", post(handle_mcp_post))
            .route("/mcp", get(handle_mcp_get))
    }
}

async fn handle_mcp_post(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(req): Json<JsonRpcRequest>,
) -> impl IntoResponse {
    let session_id = headers
        .get("mcp-session-id")
        .and_then(|v| v.to_str().ok())
        .map(String::from)
        .unwrap_or_else(|| generate_session_id());
    
    let accept = headers
        .get("accept")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    
    let is_streaming = accept.contains("text/event-stream");
    
    if is_streaming {
        // 流式响应：SSE
        let (tx, rx) = mpsc::channel::<Event>(10);
        
        tokio::spawn(async move {
            let result = state.mcp_server.handle(req).await;
            let event = Event::default()
                .event("message")
                .data(serde_json::to_string(&result).unwrap());
            let _ = tx.send(event).await;
            let _ = tx.send(Event::default().event("done").data("{}")).await;
        });
        
        Sse::new(ReceiverStream::new(rx))
            .into_response()
    } else {
        // 非流式：直接 JSON 响应
        let result = state.mcp_server.handle(req).await;
        (StatusCode::OK, [("Mcp-Session-Id", session_id)], Json(result))
            .into_response()
    }
}

async fn handle_mcp_get(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Sse<ReceiverStream<Event, Infallible>> {
    // SSE 长连接，用于服务端推送通知
    let (tx, rx) = mpsc::channel::<Event>(10);
    
    tokio::spawn(async move {
        // 监听资源变更，推送 notifications/resources/list_changed
        loop {
            tokio::time::sleep(Duration::from_secs(30)).await;
            let event = Event::default()
                .event("notification")
                .data(r#"{"jsonrpc":"2.0","method":"notifications/resources/list_changed","params":{}}"#);
            if tx.send(event).await.is_err() {
                break;
            }
        }
    });
    
    Sse::new(ReceiverStream::new(rx))
}
```

---

## 7. 完整 Tool 列表（10 个）

| Tool 名 | 作用域 | 后端调用 | 说明 |
|---------|--------|---------|------|
| `brain_get` | read | FileBackend::get | 读取页面完整内容 |
| `brain_put` | write | FileBackend::put | 创建/覆盖页面 |
| `brain_append` | write | FileBackend::append | Timeline 追加 |
| `brain_delete` | write | FileBackend::delete | 软删除 |
| `brain_list` | read | FileBackend::list | 列出目录文件 |
| `brain_search` | read | IndexBackend::keyword | FTS5 关键词搜索 |
| `brain_query` | read | IndexBackend::hybrid | RRF 混合搜索 |
| `brain_enrich` | write | IndexBackend::enrich | 链接提取 + 图谱更新 |
| `brain_maintain` | write | IndexBackend::maintain | lint + 孤儿检测 |
| `brain_sync` | admin | FileBackend + IndexBackend | 全量同步文件→索引 |
| `brain_stats` | read | IndexBackend::stats | 统计信息 |

---

## 8. MCP Resources（Skills 暴露）

Resource URI 格式：

```
skills://{scope}/{name}    → 通过 FileBackend 读取 skills/ 目录下的 .md 文件
pages://{slug}            → 通过 FileBackend 读取 wiki/ 下的 .md 文件  
log://latest              → 通过 FileBackend 读取 wiki/log.md 末尾 50 行
```

**Resource 列表由 FileBackend::list 动态扫描生成**——不是硬编码。

---

## 9. 数据模型（Compiled Truth + Timeline）

同 v1，无变更。见 v1 文档第 4.1 节。

---

## 10. 搜索算法（四信号 RRF）

同 v1，无变更。见 v1 文档第 8 节。

---

## 11. 与 Oh My OpenAgent 的集成

### 11.1 OpenCode MCP 配置

```json
// ~/.config/opencode/mcp.json
{
  "servers": {
    "my_brain": {
      "type": "http",
      "url": "http://localhost:8080/mcp",
      "api_key": "your-api-key-here"
    }
  }
}
```

### 11.2 Oh My OpenAgent 配置

```json
// ~/.config/opencode/oh-my-openagent.json
{
  "mcp_servers": {
    "my_brain": {
      "type": "http",
      "url": "http://localhost:8080/mcp",
      "api_key": "your-api-key-here",
      "resources": {
        "auto_load": true,
        "scopes": ["skills://hermes/", "skills://universal/"]
      }
    }
  }
}
```

---

## 12. 构建与部署

### 12.1 Cargo.toml

```toml
[package]
name = "my_brain"
version = "0.1.0"
edition = "2024"
rust-version = "1.85"  # nightly

[dependencies]
tokio = { version = "1", features = ["full"] }
axum = { version = "0.8", features = ["http2"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
serde_yaml = "0.9"
reqwest = { version = "0.12", features = ["json"] }
sqlx = { version = "0.8", features = ["runtime-tokio", "sqlite"] }
pulldown-cmark = "0.11"
git2 = { version = "0.19", optional = true }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
clap = { version = "4", features = ["derive"] }
config = "0.15"
dirs = "6"
chrono = { version = "0.4", features = ["serde"] }
thiserror = "1"
anyhow = "1"
sha2 = "0.10"
hex = "0.4"
walkdir = "2"
tower = "0.5"
tower-http = { version = "0.6", features = ["cors", "trace"] }
futures = "0.3"
tokio-stream = "0.1"
dashmap = "6"
async-trait = "0.1"

[features]
default = ["local-backend"]
local-backend = ["dep:git2"]

[profile.release]
lto = true
opt-level = 3
strip = true
```

### 12.2 编译
当前项目开发环境使用 flake 管理。

```bash
# 安装 nightly
使用 flake

# 开发
cargo run -- serve --config config.toml

# 生产
cargo build --release --target x86_64-unknown-linux-musl
# 输出: target/release/my_brain (静态链接，~5MB)
```

### 12.3 systemd 服务文件

```ini
# /etc/systemd/system/my_brain.service
[Unit]
Description=my_brain MCP Server
After=network.target

[Service]
Type=simple
User=brain
Group=brain
ExecStart=/usr/local/bin/my_brain serve --config /etc/my_brain/config.toml
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
```

---

## 13. 验证标准

1. `cargo test` 全绿
2. `cargo build --release` 成功，单二进制 < 10MB
3. 启动服务：`./my_brain serve --config config.toml`
4. OpenCode 连接 MCP Server 成功
5. `brain_get("wiki/index.md")` 返回正确 JSON
6. `brain_put` 写入后，Obsidian 内秒同步出现新文件
7. `brain_query("FNS")` 返回包含 `wiki/entities/fns.md` 的结果

---

## 14. 风险与回退

非必要不回退。

| 风险 | 回退方案 |
|------|---------|
| sqlite-vec 不稳定 | 初期禁用向量搜索，仅用 FTS5 |
| Streamable HTTP 兼容性 | 降级为普通 HTTP JSON（非流式）|
| FNS API 变更 | FileBackend trait 隔离，只改 fns.rs |
| Rust Nightly 不稳定 | 锁定具体 nightly 版本 |

---

## 15. MCP Prompts（Agent 行为触发器）

### 15.1 为什么需要 Prompts

Tools 告诉 Agent **"能做什么"**，Resources 告诉 Agent **"有什么知识"**，Prompts 告诉 Agent **"什么时候该做什么"**。

没有 Prompts，Hermes 面对用户的请求时不知道：
- "我收到了一篇 RSS 文章，该调用哪个 Tool？"
- "用户问了一个问题，我该先搜索还是先读取？"
- "什么时候该执行维护检查？"

### 15.2 Prompt 列表

| Prompt URI | 触发场景 | 调用方 |
|-----------|---------|--------|
| `brain://ingest` | 用户提供原始资料（文章、笔记、任务记录） | Hermes 检测到 "原始资料" 类型输入 |
| `brain://query` | 用户提问或请求查找知识 | Hermes 检测到 "问题" 或 "查找" 意图 |
| `brain://enrich` | 新页面写入后，需要提取链接/丰富实体 | Hermes 在 brain_put 成功后自动触发 |
| `brain://maintain` | 定时维护或用户请求整理知识库 | Hermes 定时触发 或 用户说 "整理/维护" |
| `brain://deep-research` | 用户请求深入研究某个主题 | Hermes 检测到 "研究/调研" 意图 |

### 15.3 prompts/list 响应格式

```json
{
  "prompts": [
    {
      "name": "ingest",
      "description": "当用户提供原始资料（文章、笔记、任务记录）时，将其转化为结构化的 Wiki 页面。",
      "uri": "brain://ingest",
      "arguments": [
        {
          "name": "source_type",
          "description": "资料类型: rss/article/meeting/task/affine-thought",
          "required": true
        },
        {
          "name": "source_content",
          "description": "原始资料内容或文件路径",
          "required": true
        },
        {
          "name": "auto_enrich",
          "description": "摄入后是否自动执行丰富化",
          "required": false
        }
      ]
    },
    {
      "name": "query",
      "description": "当用户提问或请求查找知识时，检索 Wiki 并合成回答。",
      "uri": "brain://query",
      "arguments": [
        {
          "name": "question",
          "description": "用户的问题",
          "required": true
        },
        {
          "name": "depth",
          "description": "搜索深度: quick(快速) / standard(标准) / deep(深度)",
          "required": false
        }
      ]
    },
    {
      "name": "enrich",
      "description": "当新页面写入后，提取链接、丰富实体页面、更新知识图谱。",
      "uri": "brain://enrich",
      "arguments": [
        {
          "name": "target_slug",
          "description": "要丰富的页面路径",
          "required": true
        }
      ]
    },
    {
      "name": "maintain",
      "description": "定期维护知识库健康：检查孤儿页面、死链、矛盾、过时内容。",
      "uri": "brain://maintain",
      "arguments": [
        {
          "name": "scope",
          "description": "维护范围: lint / orphans / backlinks / full",
          "required": false
        }
      ]
    },
    {
      "name": "deep-research",
      "description": "当用户请求深入研究某个主题时，执行多轮搜索、分析、综合。",
      "uri": "brain://deep-research",
      "arguments": [
        {
          "name": "topic",
          "description": "研究主题",
          "required": true
        },
        {
          "name": "iterations",
          "description": "研究轮数 (1-5)",
          "required": false
        }
      ]
    }
  ]
}
```

### 15.4 prompts/get 响应（完整 Prompt 内容）

当 Hermes 调用 `prompts/get` 获取具体 Prompt 时，返回完整的指令文本：

#### brain://ingest

```
## 场景
用户提供了原始资料，需要将其转化为结构化的 Wiki 知识。

## 资料类型判断
根据 source_type 选择处理策略：
- rss: RSS 文章摘要 → wiki/sources/{date}-{slug}.md
- article: 长文/论文 → wiki/sources/{author}-{year}-{slug}.md
- meeting: 会议记录 → wiki/sources/{date}-meeting-{topic}.md + 为每个参与者创建/更新 people 页面
- task: 任务记录 → wiki/sources/{date}-task-{id}.md + 更新相关 project 页面
- affine-thought: 画布思考 → wiki/concepts/ 或 wiki/synthesis/（结论性内容）

## 执行流程（必须遵循）
1. 调用 brain_get("wiki/index.md") 读取当前索引
2. 调用 brain_get("wiki/schema.md") 读取格式规范
3. 调用 brain_put 写入 source 页面（含 frontmatter）
4. 提取资料中的实体和概念：
   - 每个新实体 → brain_put("wiki/entities/{name}.md", ...)
   - 每个新概念 → brain_put("wiki/concepts/{name}.md", ...)
5. 更新 wiki/index.md（添加新页面的 [[wikilink]]）
6. 在 wiki/log.md 追加摄入记录
7. 如果 auto_enrich=true，继续执行 brain://enrich

## 质量标准
- 每个 source 页面必须有完整的 frontmatter（type, title, date, sources）
- 实体页面必须有定义和与其他实体的关系
- 所有新页面必须在 index.md 中有链接
- 使用 [[wikilink]] 格式做交叉引用
```

#### brain://query

```
## 场景
用户提问或请求查找知识。

## 搜索策略
根据 depth 参数选择策略：
- quick: 只调用 brain_search（FTS5 关键词搜索），取前 3 个结果
- standard: 先 brain_search，再对 Top-5 调用 brain_get 读取完整内容，合成回答
- deep: 调用 brain_query（RRF 混合搜索），读取 Top-10 的 Compiled Truth，深度综合

## 执行流程
1. 分析用户问题，提取关键词
2. 调用 brain_search 或 brain_query 执行搜索
3. 读取相关页面的 Compiled Truth（brain_get）
4. 检查 Timeline 中是否有最新更新
5. 综合所有信息，给出结构化回答：
   - 直接回答
   - 引用来源（[[wikilink]] 格式）
   - 如果信息不足，明确说明"Wiki 中没有 X 的信息"
6. （可选）如果回答有价值，调用 brain_put 写入 wiki/synthesis/{date}-{topic}.md

## 质量标准
- 每个事实必须附带 [[wikilink]] 来源
- 如果多个来源有矛盾，必须指出并说明
- 不编造 Wiki 中没有的信息
```

#### brain://enrich

```
## 场景
新页面写入后，需要提取链接、丰富实体、更新知识图谱。

## 执行流程
1. 调用 brain_get(target_slug) 读取页面内容
2. 正则提取所有 [[wikilink]] 和裸 slug 引用
3. 对每个引用的 slug：
   - 调用 brain_get 检查页面是否存在
   - 如果不存在 → 创建 stub 页面（仅 frontmatter + "TODO: 补充定义"）
   - 如果存在 → 检查是否需要更新（新信息补充）
4. 更新相关页面的 backlinks
5. 调用 brain_enrich(target_slug) 更新数据库索引
6. 返回丰富化报告（创建了多少 stub、更新了多少链接）

## 质量标准
- 不允许有孤立引用（即 [[slug]] 指向不存在的页面超过 24 小时）
- 每个实体页面至少被 1 个其他页面引用
```

#### brain://maintain

```
## 场景
定期维护知识库健康，或用户主动请求整理。

## 执行流程
1. 调用 brain_maintain("lint") 检查 frontmatter 完整性
2. 调用 brain_maintain("orphans") 查找孤立页面
3. 调用 brain_maintain("backlinks") 检查死链
4. 生成维护报告（brain_stats）
5. 如果发现问题：
   - 小问题（如缺失 tag）→ 自动修复
   - 大问题（如矛盾信息）→ 写入 reviews/maintain-{date}.md 等待人工确认
6. 在 wiki/log.md 追加维护记录

## 触发时机
- 用户说 "整理知识库" / "维护 Wiki" / "检查健康"
- 定时：每天凌晨 3 点（通过 cron 配置）
- 摄入超过 50 篇新资料后自动触发
```

#### brain://deep-research

```
## 场景
用户请求深入研究某个主题。

## 执行流程
1. 调用 brain_search 检查 Wiki 中是否已有相关资料
2. 调用 brain_query 深度搜索相关页面
3. 分析现有知识的空白点
4. 生成研究计划（写入 reviews/research-plan-{topic}.md）
5. 执行多轮研究（调用外部搜索 API 如 Tavily）
6. 将研究结果 Ingest 进 Wiki（调用 brain://ingest）
7. 生成综合报告（wiki/synthesis/{date}-{topic}-research.md）

## 质量标准
- 每轮研究必须有明确的子问题
- 外部来源必须经过验证（不轻信单一来源）
- 研究结果必须区分"已知事实"和"推测/假设"
```

### 15.5 服务端实现

```rust
// src/mcp/prompts.rs

use std::collections::HashMap;

pub struct PromptRegistry {
    prompts: HashMap<String, Prompt>,
}

impl PromptRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            prompts: HashMap::new(),
        };
        
        registry.register(Prompt {
            name: "ingest".to_string(),
            description: "当用户提供原始资料时，将其转化为结构化的 Wiki 页面".to_string(),
            uri: "brain://ingest".to_string(),
            template: include_str!("../../prompts/ingest.txt"),
        });
        
        registry.register(Prompt {
            name: "query".to_string(),
            description: "当用户提问或请求查找知识时，检索 Wiki 并合成回答".to_string(),
            uri: "brain://query".to_string(),
            template: include_str!("../../prompts/query.txt"),
        });
        
        // ... enrich, maintain, deep-research
        
        registry
    }
    
    pub fn list(&self) -> Vec<PromptSummary> {
        self.prompts.values()
            .map(|p| PromptSummary {
                name: p.name.clone(),
                description: p.description.clone(),
            })
            .collect()
    }
    
    pub fn get(&self, name: &str, args: HashMap<String, String>) -> String {
        let prompt = self.prompts.get(name)?;
        let mut rendered = prompt.template.clone();
        
        // 替换模板变量 {{arg_name}}
        for (key, value) in args {
            rendered = rendered.replace(&format!("{{{{{}}}}}" , key), &value);
        }
        
        rendered
    }
}
```

### 15.6 Hermes 侧的 Prompt 消费

```json
// Hermes 内部处理流程（概念示意）

用户输入: "这篇文章讲 FNS 的同步协议 [粘贴文章内容]"

Hermes 意图识别:
  → 包含大段原始文本 → 触发 "原始资料摄入" 场景
  → 调用 MCP prompts/get("ingest", {source_type: "article", source_content: "..."})
  → 收到完整 Prompt 文本（16.4 节的内容）
  → 按照 Prompt 定义的流程逐步执行:
    1. brain_get("wiki/index.md")
    2. brain_get("wiki/schema.md")
    3. brain_put("wiki/sources/2026-05-05-fns-sync.md", ...)
    4. brain_put("wiki/entities/fns.md", ...)  // 如果实体不存在
    5. brain_put("wiki/index.md", ...)         // 更新索引
    6. brain_append("wiki/log.md", ...)
    7. brain://enrich (auto_enrich=true)

用户输入: "我之前对 FNS 的分析是什么样的？"

Hermes 意图识别:
  → 问题句 + 引用过去知识 → 触发 "知识查询" 场景
  → 调用 MCP prompts/get("query", {question: "FNS 分析", depth: "standard"})
  → 按照 Prompt 定义的流程执行:
    1. brain_search("FNS 分析") 或 brain_query("FNS 分析")
    2. brain_get("wiki/entities/fns.md")
    3. 读取 Compiled Truth
    4. 合成回答 + [[wikilink]] 引用
```

### 15.7 Prompt 模板文件结构

```
my_brain/
├── prompts/                    # ← Prompt 模板文件
│   ├── ingest.txt              # brain://ingest 的完整指令
│   ├── query.txt               # brain://query 的完整指令
│   ├── enrich.txt              # brain://enrich 的完整指令
│   ├── maintain.txt            # brain://maintain 的完整指令
│   └── deep-research.txt       # brain://deep-research 的完整指令
├── src/
└── Cargo.toml
```

**注意**：Prompt 模板作为项目资源文件，编译时通过 `include_str!` 嵌入二进制，不依赖外部文件。

---

**文档结束。按此施工。**
