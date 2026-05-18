# stele

Stele is a personal knowledge management tool that indexes Markdown wiki pages from the `wiki/` directory of an FNS (vault) server, stores them in a local SQLite database with full-text search, and exposes operations through both a CLI and an MCP (Model Context Protocol) server. The `raw/` directory is a temporary, unindexed pool for original undigested source content.

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

### Content Layout

Stele treats the FNS vault as two separate spaces:

- `wiki/`: durable knowledge pages. These are indexed by default. Only `.md` files are indexed, hidden paths are skipped, and pages must use YAML frontmatter plus timeline entries.
- `raw/`: temporary source material. These files keep the original undigested content exactly as received, have no frontmatter or timeline, are not indexed by `sync`, and are deleted after agents digest them into `wiki/` pages.

### CLI Commands

```bash
# Page operations
stele page get <slug>
stele page put wiki/<slug> --content "# Hello\n\nWorld" --frontmatter '{"title":"Hello"}' --timeline-content "Create wiki page"
stele page put raw/<slug> --file ./source.txt
stele page delete <slug>
stele page list [dir]

# Search
stele search <query> [--limit N] [--type-filter TYPE]

# Graph queries
stele graph query <slug> [--depth N] [--direction in|out|both]

> **Note**: `graph.backlinks` has been removed. Use `stele graph query <slug> --direction in` instead.

# Sync wiki Markdown pages from FNS vault (default: wiki/)
stele sync [--dir wiki]

# Maintenance
stele maintain [--scope lint|orphans|backlinks|full]

# Index stats
stele stats

# Rebuild wiki search index
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

Available MCP tools: `page.get`, `page.put`, `page.delete`, `page.list`, `search`, `graph.query`, `sync`, `maintain`, `stats`, `reindex`.

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

Stele reads Markdown files from `wiki/` in an FNS vault via HTTP, parses YAML frontmatter, timeline entries, and wikilink syntax (`[[target]]`, `[[type::target]]`), and stores pages in SQLite with an FTS5 full-text search index. `sync` indexes only `.md` files, skips hidden paths, and leaves `raw/` content unindexed because raw files are temporary original source material with no frontmatter or timeline. The link graph is tracked in a separate `links` table, enabling graph queries like backlinks, BFS neighborhood traversal, and orphan detection. Operations are exposed through a unified `OperationRegistry` that dispatches to both the CLI and MCP server handlers.

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

## References

- [karpathy/min](https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f)
- [garrytan/gbrain](https://github.com/garrytan/gbrain)

Thanks to their work for the inspiration.
