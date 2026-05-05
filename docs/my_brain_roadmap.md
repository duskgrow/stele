# my_brain 项目完整路线图
## 从 MVP 到长期演进的系统化规划

**版本**: v1.0  
**日期**: 2026-05-05  
**当前状态**: OpenCode 正在实现第一阶段（MCP Server + FNS 集成）  

---

## 项目总览

```
┌─────────────────────────────────────────────────────────────────┐
│                        my_brain 生态系统                         │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  Phase 0: 地基 (当前)                                              │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐         │
│  │   my_brain    │  │     FNS      │  │  Obsidian    │         │
│  │  MCP Server   │  │   同步服务    │  │   阅读器     │         │
│  │  (Rust)       │  │              │  │              │         │
│  └──────────────┘  └──────────────┘  └──────────────┘         │
│                                                                  │
│  Phase 1-2: 智能层                                                │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐         │
│  │   Skills      │  │   搜索/图谱   │  │   维护循环   │         │
│  │  (Prompts)    │  │  (RRF/Louvain)│  │ (Dream)     │         │
│  └──────────────┘  └──────────────┘  └──────────────┘         │
│                                                                  │
│  Phase 3-5: 扩展层                                                │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐         │
│  │   外部源      │  │   深度研究    │  │   异步审核   │         │
│  │  (RSS/Plane)  │  │  (Tavily)    │  │ (Review)    │         │
│  └──────────────┘  └──────────────┘  └──────────────┘         │
│                                                                  │
│  Phase 6+: 自治层                                                 │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐         │
│  │   Agent 自治  │  │   多模态     │  │   协作网络   │         │
│  │  (Self-driving)│  │  (图片/语音)  │  │  (Share)    │         │
│  └──────────────┘  └──────────────┘  └──────────────┘         │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

---

## 第一阶段：地基（Phase 0）— 正在进行

### 目标
让 OpenCode 实现的最小可用系统能跑通第一个 end-to-end 闭环。

### 交付物

| 模块 | 状态 | 说明 |
|------|------|------|
| Config 系统 | 🔄 进行中 | TOML + 环境变量，backend 类型切换 |
| FileBackend Trait | 🔄 进行中 | `FnsBackend` 实现 |
| MCP Streamable HTTP | 🔄 进行中 | axum 路由，`/mcp` 端点 |
| brain_get Tool | 🔄 进行中 | 读取页面 |
| brain_put Tool | 🔄 进行中 | 写入/覆盖页面 |
| SQLite 基础表 | 🔄 进行中 | pages, links 表 |
| Prompts 暴露 | 🔄 进行中 | 5 个核心 Prompt 模板 |

### 验证标准
1. `cargo test` 全绿
2. 启动服务后，Hermes 能连接 MCP Server
3. `brain_get("wiki/index.md")` 返回正确内容
4. `brain_put` 写入后，Obsidian 内秒同步出现新文件

### 时间估算
- OpenCode 正在实现，预计 **3-7 天**

---

## 第二阶段：智能层（Phase 1）— 搜索与查询

### 目标
让 Agent 能够有效地从 Wiki 中检索知识，回答用户问题。

### 交付物

| 模块 | 优先级 | 技术方案 |
|------|--------|---------|
| **brain_search** (FTS5) | ⭐⭐⭐ 高 | SQLite FTS5 虚拟表，关键词匹配 |
| **brain_query** (RRF) | ⭐⭐⭐ 高 | 四信号融合：关键词 + 向量 + 图谱 + 类型亲和 |
| **sqlite-vec 集成** | ⭐⭐⭐ 高 | 可选：OpenAI embedding API → 向量存储 |
| **Prompt: query** | ⭐⭐⭐ 高 | 定义搜索策略（quick/standard/deep）|
| **Context Budget 控制** | ⭐⭐ 中 | 返回结果时的 token 预算管理 |
| **结果缓存** | ⭐⭐ 中 | `search_cache` 表，避免重复查询 |

### 技术细节

**RRF 融合算法实现**（`src/search/hybrid.rs`）：
```
score(page) = Σ weight_i / (60 + rank_i)

signals:
  keyword_rank × 1.0
  vector_rank   × 1.0
  direct_link   × 3.0   (from_slugs 直接链接)
  source_overlap × 4.0  (共享 sources)
  common_neighbors × 1.5 (Adamic-Adar)
  type_affinity × 变量
```

### 验证标准
1. `brain_query("FNS 同步协议")` 返回包含 `wiki/entities/fns.md` 的结果
2. 结果按 RRF 分数排序，Top-5 包含最相关页面
3. 有向量搜索时，语义相近但不关键词匹配的页面也能召回

### 时间估算
- **1-2 周**（全职开发）

---

## 第三阶段：智能层（Phase 2）— 知识图谱与维护

### 目标
让 Wiki 自动保持结构健康，发现知识间的隐藏关联。

### 交付物

| 模块 | 优先级 | 说明 |
|------|--------|------|
| **brain_enrich** | ⭐⭐⭐ 高 | 提取 wikilink，更新图谱，创建 stub 实体页 |
| **brain_maintain** | ⭐⭐⭐ 高 | lint + 孤儿检测 + 死链检查 + 矛盾扫描 |
| **Prompt: enrich** | ⭐⭐⭐ 高 | 丰富化的触发条件和执行流程 |
| **Prompt: maintain** | ⭐⭐⭐ 高 | 维护检查的 Step-by-step |
| **Louvain 社区检测** | ⭐⭐ 中 | 发现知识聚类（可延后）|
| **图谱洞察** | ⭐⭐ 中 | 惊奇连接 + 知识空白（依赖社区检测）|

### 技术细节

**链接提取**（`src/services/enrich.rs`，零 LLM）：
```rust
fn extract_links(content: &str) -> Vec<Link> {
    // 1. [[slug]] → link_type: "link"
    // 2. [[slug|text]] → link_type: "link"
    // 3. "CEO of [[company]]" → link_type: "works_at"
    // 4. "attended [[meeting]]" → link_type: "attended"
    // 5. 裸 slug 引用（非 wikilink 格式但匹配实体名）
}
```

**维护检查清单**（`src/services/maintain.rs`）：
- [ ] frontmatter 完整性（必需字段是否存在）
- [ ] YAML 语法正确性
- [ ] slug 命名规范检查
- [ ] 孤立页面（无入站链接超过 30 天）
- [ ] 死链（指向不存在的页面）
- [ ] 矛盾声明（同一实体在不同页面描述冲突）
- [ ] 过时内容（超过 90 天未更新的页面）

### 验证标准
1. `brain_enrich("wiki/sources/xxx.md")` 后，links 表有新记录
2. `brain_maintain("full")` 生成报告，列出所有问题
3. 新创建的 stub 页面在 24 小时内被补充定义

### 时间估算
- **1-2 周**（enrich + maintain）
- **+1-2 周**（Louvain + 图谱洞察，可选）

---

## 第四阶段：扩展层（Phase 3）— 外部信息管道

### 目标
让 RSS、Plane、AFFiNE 等外部源能自动流入 Wiki，减少人工操作。

### 交付物

| 模块 | 优先级 | 说明 |
|------|--------|------|
| **RSS 抓取脚本** | ⭐⭐⭐ 高 | Python/Bash 脚本，定时抓取，写入 FNS raw/queue/ |
| **Plane Webhook 接收器** | ⭐⭐⭐ 高 | 极简 HTTP 服务（可用 n8n 或 Python FastAPI）|
| **AFFiNE 导出同步** | ⭐⭐ 中 | 定时导出 AFFiNE 内容到 raw/queue/ |
| **raw/ 文件系统队列** | ⭐⭐⭐ 高 | queue/ → processed/ → failed/ 状态管理 |
| **Prompt: ingest** | ⭐⭐⭐ 高 | 定义完整 Ingest 流程（Analysis → Generation）|
| **brain_sync** | ⭐⭐⭐ 高 | 手动/定时触发文件→索引同步 |

### 架构设计

```
外部信息源
    │
    ├── RSS → 定时脚本 → FNS raw/queue/ → 等待 Hermes Ingest
    │
    ├── Plane Webhook → HTTP 接收器 → FNS raw/queue/
    │
    └── AFFiNE → 定时导出 → FNS raw/queue/
                │
                ▼
        Hermes 检测到新文件
                │
                ▼
        调用 brain://ingest Prompt
                │
                ▼
        brain_get("raw/queue/xxx.md") → Analysis
                │
                ▼
        brain_put("wiki/sources/xxx.md") → Generation
        brain_put("wiki/entities/xxx.md") → 实体丰富
        brain_put("wiki/index.md") → 索引更新
        brain_append("wiki/log.md") → 日志记录
                │
                ▼
        brain://enrich → 链接提取
                │
                ▼
        brain_maintain → 健康检查
```

### 验证标准
1. RSS 更新后 5 分钟内，Obsidian 出现新文件
2. Plane 任务完成后，相关复盘页面自动生成
3. AFFiNE 画布结论性内容，24 小时内进入 Wiki

### 时间估算
- **1-2 周**（RSS + Plane Webhook）
- **+3-5 天**（AFFiNE 导出）

---

## 第五阶段：扩展层（Phase 4）— 深度研究

### 目标
让 Agent 能自主执行多轮网络研究，将结果结构化进 Wiki。

### 交付物

| 模块 | 优先级 | 说明 |
|------|--------|------|
| **Prompt: deep-research** | ⭐⭐⭐ 高 | 定义研究流程 |
| **Tavily API 集成** | ⭐⭐⭐ 高 | 网络搜索，获取结构化结果 |
| **SearXNG 自建** | ⭐⭐ 中 | 自托管搜索，替代 Tavily |
| **研究结果 Ingest** | ⭐⭐⭐ 高 | 搜索结果 → raw/ → wiki/ |
| **研究计划生成** | ⭐⭐ 中 | 自动分解主题，生成子问题 |

### 技术细节

**研究流程**（Hermes 执行）：
1. 用户请求："研究 FNS 的竞品"
2. 调用 `brain_search("FNS 竞品")` 检查 Wiki 已有知识
3. 调用 Tavily API：`query = "FNS Fast Note Sync alternatives Obsidian sync"`
4. 获取结果 → 写入 `raw/research/2026-05-05-fns-alternatives.md`
5. 调用 `brain://ingest` → 转化为 Wiki 页面
6. 生成 `wiki/comparisons/fns-vs-others.md`
7. 更新 `wiki/index.md`

### 验证标准
1. "研究 X" 请求后，30 分钟内生成结构化对比页面
2. 研究来源可追溯，每个声明有 URL 引用
3. 研究结果自动与已有知识关联

### 时间估算
- **1-2 周**

---

## 第六阶段：扩展层（Phase 5）— 异步审核

### 目标
建立质量门控，防止错误累积。

### 交付物

| 模块 | 优先级 | 说明 |
|------|--------|------|
| **Rule-based 审核** | ⭐⭐⭐ 高 | 孤立页面、死链、命名冲突自动检测 |
| **LLM Semantic 审核** | ⭐⭐ 中 | 矛盾声明、过时内容、质量评估 |
| **reviews/ 目录** | ⭐⭐⭐ 高 | 审核项写入 Markdown，等待人工确认 |
| **Prompt: review** | ⭐⭐⭐ 高 | 定义审核标准和人工介入流程 |
| **审核队列** | ⭐⭐ 中 | 优先级排序，批量处理 |

### 技术细节

**两级审核**（同 GBrain 的 sweep-reviews.ts）：

**Level 1: Rule-based（自动）**
```
扫描 wiki/ 所有页面
├── 孤立页面（无入站链接 30+ 天）→ 自动标记
├── 死链（指向不存在的 slug）→ 自动标记
├── 命名冲突（相同 slug 不同内容）→ 自动标记
├── frontmatter 缺失 → 自动标记
└── 重复实体（相似度 > 85% 的两个页面）→ 自动标记
```

**Level 2: LLM Semantic（人工确认）**
```
对标记项调用 Hermes 评估：
├── "这个孤立页面是否应该删除？"
├── "这两个实体页是否应该合并？"
├── "这个矛盾声明需要更新哪个页面？"
└── 结果写入 reviews/2026-05-05-batch.md
```

### 验证标准
1. 每周生成审核报告，列出所有待确认项
2. 人工确认后，自动执行修复
3. 未确认项不阻塞其他操作

### 时间估算
- **1-2 周**

---

## 第七阶段：自治层（Phase 6+）— 长期演进

### 目标
让系统趋近于"自进化"——Agent 自主发现问题、自主研究、自主更新。

### 可能方向

| 方向 | 说明 | 难度 |
|------|------|------|
| **Agent 自治循环** | Hermes 定期（每天/每周）执行：扫描 → 发现空白 → 自主研究 → 更新 | ⭐⭐⭐⭐⭐ |
| **多模态支持** | 图片、语音、PDF 的 Ingest 和处理 | ⭐⭐⭐⭐ |
| **协作网络** | 多用户共享 Wiki，权限管理 | ⭐⭐⭐⭐ |
| **外部 API 生态** | 连接更多数据源（GitHub、Notion、Slack）| ⭐⭐⭐ |
| **可视化界面** | 知识图谱 Web UI，搜索界面 | ⭐⭐⭐ |
| **版本控制增强** | Compiled Truth 的 diff 视图，变更历史 | ⭐⭐⭐ |

---

## 时间总览

| 阶段 | 内容 | 时间估算 | 累计 |
|------|------|---------|------|
| Phase 0 | 地基（MCP + FNS + 基础 Tool）| 1-2 周 | 第 1-2 周 |
| Phase 1 | 搜索与查询 | 1-2 周 | 第 3-4 周 |
| Phase 2 | 图谱与维护 | 1-2 周 | 第 5-6 周 |
| Phase 3 | 外部信息管道 | 1-2 周 | 第 7-8 周 |
| Phase 4 | 深度研究 | 1-2 周 | 第 9-10 周 |
| Phase 5 | 异步审核 | 1-2 周 | 第 11-12 周 |
| Phase 6+ | 自治与扩展 | 按需 | 第 12 周+ |

**总计 MVP 到生产可用：约 8-10 周**  
**总计到高质量系统：约 12 周**

---

## 当前优先级（今天该做什么）

1. **等待 OpenCode 完成 Phase 0**，验证 MCP Server 能启动
2. **准备测试数据**：在 Obsidian 里创建 `wiki/index.md` 和 `wiki/schema.md`
3. **配置 Hermes MCP**：`~/.config/opencode/mcp.json` 指向 my_brain
4. **跑通第一个闭环**：对 Hermes 说"请读取 wiki/index.md"

Phase 0 验证通过后，立即进入 Phase 1（搜索实现）。

---

**文档结束。**
