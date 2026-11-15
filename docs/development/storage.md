# Storage & Configuration Persistence

GDS separates human-editable configuration files (`gds::persistence`) from structured application state and caches (`gds::service::storage`). There is no cross-application shared config file; only user theme TOML files in `$XDG_CONFIG_HOME/gds/themes/` are shared across apps.

## Per-Application XDG Paths

| Application | Data Kind | Default Path | Mechanism |
| --- | --- | --- | --- |
| **Suite** | Custom user themes | `$XDG_CONFIG_HOME/gds/themes/**/*.toml` | Read-only TOML palettes + hot-reload |
| **GDS Files** | User preferences | `$XDG_CONFIG_HOME/gds/app/files/settings.toml` | `gds::persistence` (TOML) |
| **GDS Files** | Places & view state | `$XDG_STATE_HOME/gds/files/state.sqlite` | `gds::service::storage` (Toasty SQLite) |
| **GDS Files** | Accepted SSH host keys | `$XDG_STATE_HOME/gds/files/known_hosts` | `service::remote` |
| **GDS Files** | Media thumbnails | `$XDG_CACHE_HOME/thumbnails/x-large/*.png` | Freedesktop PNG cache (`file_inspection`) |
| **GDS Agents** | Frontend preferences | `$XDG_CONFIG_HOME/gds/app/agents/settings.json` | `gds::persistence` (JSON) |
| **GDS Agents** | Session navigation state | `$XDG_STATE_HOME/gds-agents/<connection-key>.json` | `gds::persistence` (JSON) |
| **GDS Agents** | Connection history cache | `$XDG_CACHE_HOME/gds/agents/history-v1/history.sqlite` | `gds::service::storage` (512 MiB LRU budget) |

## Configuration Persistence (`gds::persistence`)

- **Atomic writes & conflict detection**: Writes stage to a sibling file and replace atomically while serializing queued snapshots through a background Task Manager worker.
- **Managed-file safety**: Read-only files and declarative symlinks (e.g., Nix Home Manager links) can be read normally, and runtime changes still apply in memory, but GDS refuses to unlink or overwrite them—surfacing a clear "applied in memory, not saved to disk" notification.

## SQLite Storage (`gds::service::storage`)

Structured local databases use [Toasty](https://tokio-rs.github.io/toasty/0.11.0/guide/sqlite.html) (`Database` / `Db`) running on the Task Manager's Tokio runtime:

```rust,ignore
let db = gds::service::storage::Options::new(migrations)
    .models(toasty::models!(...))
    .open(&tasks_scope, &db_path)
    .await?;
```

- **Connection & pragmas**: Single-connection pool (`capacity = 1`) shared via cheap `Database` clones; underlying synchronous `rusqlite` calls run inside Toasty's dedicated worker task. Defaults enable `WAL`, `foreign_keys = ON`, and a 5-second `busy_timeout` (Agents' rebuildable cache uses `DELETE` journal mode with `FULL` auto-vacuum).
- **Migrations & multi-process locking**: Applications supply a `MigrationSet` of embedded `MigrationFile` entries (using `-- #[toasty::breakpoint]` statement separators) applied via `MigrationSet::apply(&db)`. Multi-process startup serializes initialization on a sibling `.lock` file using `std::fs` file locks inside Tokio's blocking pool.
- **Transactional accessors**: Each domain store wraps `Database` and exposes a typed `Accessor<'_>` borrowing an active `Transaction`, so multi-step writes and post-write snapshot reads share one atomic transaction.

### Domain Schema Highlights

- **Files (`state.sqlite`)**: `locations` and `view_settings` key records by tab-independent location identity (`key_kind`, `key_identity`, `key_path`, preserving raw non-UTF-8 OS path bytes). `view_settings` stores per-location column visibility/order, logical-pixel widths keyed by column identity, sort order, details-panel visibility, and list/grid mode alongside the application-wide default row.
- **Agents (`history.sqlite`)**: Stores cached sessions, paginated windows (indexed by starting cursor), and typed events scoped per BAS connection namespace. Eviction enforces a 512 MiB budget ordered by least-recently-accessed sessions without double-counting replaced pages.
