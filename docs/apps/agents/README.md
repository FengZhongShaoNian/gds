# GDS Agents

GDS Agents is a native GPUI session workspace for an independently running **BAS** daemon, providing multi-session conversations, streaming Markdown and tool activity, attachments, authentication, interactive permission prompts, searchable history, and Agent management.

## Running

```sh
just dev agents
just dev agents --compact
just dev agents --server unix:///path/to/bas/service.sock
just build agents
just release agents
```

Equivalent Cargo command: `cargo run --features agents --bin gds-agents -- ...`.

- Default mode opens the full workspace; `--compact` opens the floating remote-style input window (switchable from the top-right toolbar).
- `--cwd` sets the initial working directory for new sessions; `--server` or `BAS_SERVER` overrides the active connection profile (supporting local IPC, Unix sockets, Windows named pipes, TCP, and TLS).

## Workspaces, Sessions & Turn Navigation

- **Persistent BAS workspaces**: Workspaces are daemon-owned entities with stable IDs, primary directories, revisions, and archive states. Adding an empty workspace (`features/workspace_launch`) does not spawn an Agent until a prompt is typed, an Agent is chosen, or a session is explicitly created.
- **Sidebar views**: A single header toolbar switches between four mutually exclusive views—**Workspaces**, **Bookmarks**, **History** (time-grouped), and **Archived**—alongside inline `Ctrl+F` search, new session, and add-workspace actions. Collapsed rail mode preserves all four actions, agent icons, status badges, and hover details.
- **Archive vs delete**: Archiving a workspace cascades archival across its sessions on BAS (restoring the workspace leaves individual sessions archived until restored). Deleting a non-empty workspace requires confirmation showing session counts and sends `force=true` to purge BAS history while leaving project files on disk untouched.
- **Conversation turn rail**: Loaded turns render as evenly spaced ticks on the right-hand rail; consecutive unloaded turn ranges collapse into compact summary menu buttons that load turn bodies on demand. `Ctrl+Click` toggles bookmarks; `Alt+Scroll` and turn shortcuts traverse full history.
- **Overlay navigation**: Settings (`Ctrl+,`), Session Management, Agent Installation, Help (`F1`), and About mount inside the main window stack (`Esc`, `Alt+Left`, `Ctrl+W`, or top-left back arrow return one level while preserving unsent drafts and live streams).

## Configuration, Cache & Themes

| Data | Default Location | Purpose |
| --- | --- | --- |
| Frontend settings | `$XDG_CONFIG_HOME/gds/app/agents/settings.json` | Local appearance, input, connection profiles, and terminal settings (`--settings-file` / `GDS_AGENTS_SETTINGS_FILE`) |
| Connection history cache | `$XDG_CACHE_HOME/gds/agents/history-v1/history.sqlite` | Rebuildable Toasty SQLite cache with a 512 MiB LRU eviction budget |
| Navigation state | `$XDG_STATE_HOME/gds-agents/<connection-key>.json` | Selected session, pins, bookmarks, and read cursors per connection |
| Shared user themes | `$XDG_CONFIG_HOME/gds/themes/**/*.toml` | Custom light/dark palettes shared with GDS Files (see [Themes](../../visual-language/theme.md)) |

Pass `--enable-logs` to enable in-memory tracing and the `Ctrl+Shift+L` Debug/Tasks window. [Embedded terminals](../terminal.md) are available exclusively on declared local IPC workspaces.

## Implementation Boundaries

Code lives in [`bin/agents/src/`](../../../bin/agents/src/) following FSD (`app → pages → widgets → features → entities → shared`). GDS Agents depends on `bas-client` and `bas-protocol` (pinned in `Cargo.lock`) without embedding the BAS server or writing daemon config files directly—remote service settings use revision-checked management RPCs.

For the dual-stream ACP observation / BAS management contract, frontend multiplexing, and JSON Schema workspace launch forms, see [Agents & BAS Protocol Architecture](../../development/agents-protocol.md).
