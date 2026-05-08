# stele

Stele is a personal knowledge management tool that indexes markdown pages from an FNS (vault) server, stores them in a local SQLite database with full-text search, and exposes operations through both a CLI and an MCP (Model Context Protocol) server.

## Quick Start

### Install

```bash
cargo install --path .
```

Or with Nix:

```bash
nix develop
cargo build --release
```

### Configure

Create a config file at `~/.config/stele/config.toml`:

```toml
[server]
host = "127.0.0.1"
port = 3000

[fns]
base_url = "http://localhost:3000"
token = "your-api-token"
vault = "default"

[index]
db_path = "~/.local/share/stele/index.db"
```

See `config.toml.example` for all options and environment variable overrides.

### Run

Start the MCP server (stdio transport, for use with Claude Desktop):

```bash
stele serve
```

Start the MCP HTTP server:

```bash
stele serve --transport http --port 3000
```

## Usage

### CLI Commands

```bash
# Page operations
stele page get <slug>
stele page put <slug> --content "# Hello\n\nWorld"
stele page put <slug> --file ./page.md
stele page delete <slug>
stele page list [dir]

# Search
stele search <query> [--limit N] [--type-filter TYPE]

# Graph queries
stele graph <slug> [--depth N]

# Sync from FNS vault
stele sync [--dir /notes]

# Maintenance
stele maintain [--scope lint|orphans|backlinks|full]

# Index stats
stele stats

# Rebuild search index
stele reindex
```

### MCP Server Setup

Add to your Claude Desktop config (`~/.config/claude/config.json`):

```json
{
  "mcpServers": {
    "stele": {
      "command": "stele",
      "args": ["serve", "--transport", "stdio"]
    }
  }
}
```

Available MCP tools: `page.get`, `page.put`, `page.delete`, `page.list`, `search`, `graph.query`, `graph.backlinks`, `sync`, `maintain`, `stats`, `reindex`.

## Configuration Reference

Configuration is loaded with this priority (highest first):

1. Environment variables (`STELE_*`)
2. Config file (TOML)
3. Hard-coded defaults

Config file resolution order:

1. `STELE_CONFIG` environment variable
2. `~/.config/stele/config.toml`
3. `./config.toml`

| Section | Key | Default | Environment Variable |
|---------|-----|---------|---------------------|
| server | host | `127.0.0.1` | `STELE_SERVER_HOST` |
| server | port | `3000` | `STELE_SERVER_PORT` |
| fns | base_url | `http://localhost:3000` | `STELE_FNS_BASE_URL` |
| fns | token | `""` | `STELE_FNS_TOKEN` |
| fns | vault | `default` | `STELE_FNS_VAULT` |
| index | db_path | `~/.local/share/stele/index.db` | `STELE_INDEX_DB_PATH` |

## Architecture

Stele reads markdown files from an FNS vault via HTTP, parses YAML frontmatter and wikilink syntax (`[[target]]`, `[[type::target]]`), and stores pages in SQLite with an FTS5 full-text search index. The link graph is tracked in a separate `links` table, enabling graph queries like backlinks, BFS neighborhood traversal, and orphan detection. Operations are exposed through a unified `OperationRegistry` that dispatches to both the CLI and MCP server handlers.

## Development

```bash
# Enter Nix development shell
nix develop

# Run tests
cargo test

# Build documentation
cargo doc --no-deps

# Run with logging
RUST_LOG=info cargo run -- serve
```
