# Stele Cron Guide

Automated execution of Stele skills through cron jobs and systemd timers.

## Overview

| Skill | Trigger | Cron Job | Purpose |
|-------|---------|----------|---------|
| `stele-lint` | Periodic | Yes | Health checks and maintenance |
| `stele-ingest` | Manual | No | Raw material ingestion |
| `stele-query` | Manual | No | Wiki knowledge retrieval |

`stele-ingest` and `stele-query` are user-initiated skills. They do not run on a schedule. `stele-lint` runs periodically to keep the knowledge base healthy.

---

## Cron Jobs

### 1. Daily Lite Maintenance (Lint Only)

Runs a quick structural check every morning.

```cron
# Daily at 03:00 - stele-lint (lite)
0 3 * * * stele maintain --scope lint >> ~/.local/share/stele/logs/lint.log 2>&1
```

| Field | Value |
|-------|-------|
| Schedule | `0 3 * * *` |
| Command | `stele maintain --scope lint` |
| Skill | `stele-lint` |
| Scope | Errors and warnings only |

### 2. Weekly Full Maintenance

Runs the complete health check including orphan detection and backlink repair.

```cron
# Weekly on Sunday at 02:00 - stele-lint (full)
0 2 * * 0 stele maintain --scope full >> ~/.local/share/stele/logs/maintenance.log 2>&1
```

| Field | Value |
|-------|-------|
| Schedule | `0 2 * * 0` |
| Command | `stele maintain --scope full` |
| Skill | `stele-lint` |
| Scope | lint, orphans, backlinks |

### 3. Daily Vault Sync

Syncs the local wiki index with the FNS vault server. By default, `stele sync` scans `wiki/`, indexes only `.md` files, skips hidden paths, and does not index temporary `raw/` source material.

```cron
# Daily at 01:00 - sync from FNS vault
0 1 * * * stele sync >> ~/.local/share/stele/logs/sync.log 2>&1
```

| Field | Value |
|-------|-------|
| Schedule | `0 1 * * *` |
| Command | `stele sync` |
| Skill | N/A (background sync) |
| Notes | Only needed if using an FNS vault; sync targets `wiki/` by default |

### 4. Weekly Stats Report

Generates an index statistics report.

```cron
# Weekly on Monday at 08:00 - stats report
0 8 * * 1 stele stats >> ~/.local/share/stele/logs/stats.log 2>&1
```

| Field | Value |
|-------|-------|
| Schedule | `0 8 * * 1` |
| Command | `stele stats` |
| Skill | N/A (reporting) |
| Output | Page counts, index health |

---

## Recommended Schedule Summary

```cron
STELE_CONFIG=/home/user/.config/stele/config.toml

# Daily sync at 01:00 (wiki/ Markdown index)
0 1 * * * stele sync >> ~/.local/share/stele/logs/sync.log 2>&1

# Daily lite lint at 03:00
0 3 * * * stele maintain --scope lint >> ~/.local/share/stele/logs/lint.log 2>&1

# Weekly full maintenance on Sunday at 02:00
0 2 * * 0 stele maintain --scope full >> ~/.local/share/stele/logs/maintenance.log 2>&1

# Weekly stats on Monday at 08:00
0 8 * * 1 stele stats >> ~/.local/share/stele/logs/stats.log 2>&1
```

---

## Environment Variables

Set these at the top of your crontab or in the service unit.

| Variable | Purpose | Example |
|----------|---------|---------|
| `STELE_CONFIG` | Path to config file | `/home/user/.config/stele/config.toml` |
| `STELE_FNS_BASE_URL` | FNS vault URL | `http://localhost:3000` |
| `STELE_FNS_TOKEN` | API token for FNS | `your-api-token` |
| `STELE_INDEX_DB_PATH` | SQLite database path | `/home/user/.local/share/stele/index.db` |
| `RUST_LOG` | Logging level | `info` or `warn` |

### Minimal crontab header

```cron
SHELL=/bin/bash
PATH=/usr/local/bin:/usr/bin:/bin
STELE_CONFIG=/home/user/.config/stele/config.toml
RUST_LOG=warn
```

---

## Installation: Crontab (User-level)

1. Ensure the log directory exists:

```bash
mkdir -p ~/.local/share/stele/logs
```

2. Open your crontab:

```bash
crontab -e
```

3. Paste the recommended schedule from above. Adjust paths to match your system.

4. Verify the jobs are registered:

```bash
crontab -l
```

---

## Installation: Crontab (System-wide)

For system-wide execution as a dedicated user:

1. Create or edit the system crontab:

```bash
sudoedit /etc/cron.d/stele
```

2. Add jobs with a user field:

```cron
STELE_CONFIG=/home/stele/.config/stele/config.toml

# Daily sync (wiki/ Markdown index)
0 1 * * * stele sync >> /var/log/stele/sync.log 2>&1

# Daily lint
0 3 * * * stele maintain --scope lint >> /var/log/stele/lint.log 2>&1

# Weekly full maintenance
0 2 * * 0 stele maintain --scope full >> /var/log/stele/maintenance.log 2>&1
```

3. Create log directory with proper permissions:

```bash
sudo mkdir -p /var/log/stele
sudo chown stele:stele /var/log/stele
```

---

## Installation: Systemd Timers

For systems using systemd, timers are more robust than cron. They support dependency management, better logging via journald, and failure handling.

### 1. User service directory

```bash
mkdir -p ~/.config/systemd/user
```

### 2. Service unit: `~/.config/systemd/user/stele-lint.service`

```ini
[Unit]
Description=Stele knowledge base lint
After=network-online.target

[Service]
Type=oneshot
Environment="STELE_CONFIG=%h/.config/stele/config.toml"
Environment="RUST_LOG=warn"
ExecStart=stele maintain --scope lint
```

### 3. Timer unit: `~/.config/systemd/user/stele-lint.timer`

```ini
[Unit]
Description=Run Stele lint daily at 03:00

[Timer]
OnCalendar=*-*-* 03:00:00
Persistent=true

[Install]
WantedBy=timers.target
```

### 4. Service unit: `~/.config/systemd/user/stele-sync.service`

```ini
[Unit]
Description=Stele vault sync
After=network-online.target

[Service]
Type=oneshot
Environment="STELE_CONFIG=%h/.config/stele/config.toml"
Environment="RUST_LOG=warn"
ExecStart=stele sync
```

This service syncs the default `wiki/` Markdown index; `raw/` source material remains unindexed.

### 5. Timer unit: `~/.config/systemd/user/stele-sync.timer`

```ini
[Unit]
Description=Run Stele sync daily at 01:00

[Timer]
OnCalendar=*-*-* 01:00:00
Persistent=true

[Install]
WantedBy=timers.target
```

### 6. Enable and start timers

```bash
systemctl --user daemon-reload
systemctl --user enable stele-lint.timer
systemctl --user enable stele-sync.timer
systemctl --user start stele-lint.timer
systemctl --user start stele-sync.timer
```

### 7. Check timer status

```bash
systemctl --user list-timers
journalctl --user -u stele-lint.service
```

### Weekly full maintenance timer

For the weekly full maintenance, create an additional timer with a weekly calendar:

```ini
[Timer]
OnCalendar=Sun *-*-* 02:00:00
Persistent=true
```

---

## Manual Skill Execution

These skills do not use cron jobs. Trigger them on demand.

### stele-ingest (manual)

Run when new raw materials arrive:

```bash
# Ingest after receiving a document or URL
hermes --skill stele-ingest
```

Or trigger through your MCP client (Claude Desktop, etc.) by invoking the `stele-ingest` skill.

### stele-query (manual)

Run when you have a question about wiki content:

```bash
hermes --skill stele-query
```

Or trigger through your MCP client.

### Alternative: File watcher for stele-ingest

If you have a drop directory for raw materials, use `inotifywait` or `systemd.path` units to trigger ingestion on file arrival. This is outside the scope of standard cron.

---

## Log Rotation

Crontab logs grow indefinitely. Set up rotation:

```bash
# ~/.config/logrotate/stele
~/.local/share/stele/logs/*.log {
    daily
    rotate 14
    compress
    delaycompress
    missingok
    notifempty
}
```

For systemd timers, logs are managed by journald and rotate automatically. Check retention with:

```bash
journalctl --user --disk-usage
```

---

## Troubleshooting

| Problem | Cause | Fix |
|---------|-------|-----|
| `stele: command not found` | PATH not set in crontab | Add `PATH=...` or use absolute path to binary |
| Config not loaded | `STELE_CONFIG` not set | Export variable in crontab or service unit |
| FNS sync fails | Token expired or network down | Check `STELE_FNS_TOKEN` and network connectivity |
| Lint reports too many warnings | Database out of date | Run `stele reindex` before `stele maintain` |

### Test a cron job manually

```bash
env -i STELE_CONFIG=/home/user/.config/stele/config.toml \
    HOME=/home/user \
    PATH=/usr/local/bin:/usr/bin:/bin \
    stele maintain --scope lint
```

This simulates the minimal environment cron provides.

---

## Reference: Stele CLI Commands

| Command | Maps to Skill | Cron Eligible |
|---------|---------------|---------------|
| `stele maintain --scope lint` | stele-lint (phase 1) | Yes |
| `stele maintain --scope orphans` | stele-lint (phase 2) | Yes |
| `stele maintain --scope backlinks` | stele-lint (phase 3) | Yes |
| `stele maintain --scope full` | stele-lint (all phases) | Yes |
| `stele sync` | N/A (wiki/ Markdown index) | Yes |
| `stele stats` | N/A | Yes |
| `stele search` | stele-query | No (manual) |
| `stele page put` | stele-ingest | No (manual) |
| `stele reindex` | N/A | On demand only |
