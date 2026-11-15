# Agents & BAS Protocol Architecture

GDS Agents is a stateless/cache-only native frontend for an independently running **BAS** daemon (`GanDesktop/bas`, pinned via `Cargo.lock`). BAS is the persistent ACP client of each underlying Agent process; desktop and remote frontends attach as equal, non-exclusive producers and observers.

## Dual Independent Event Streams

Frontend attachment never owns Agent execution, and there is no primary-frontend election, lease, or command scheduler. Live updates split strictly across two independent RPC streams:

| Stream / RPC | Carries | Never Carries |
| --- | --- | --- |
| **ACP Observation** (`Bas.ObserveAcp`) | JSON-RPC prompt requests/results and live `session/update` notifications, tagged with public session ID, observation cursor, timestamp, and optional submission association | BAS-owned session metadata, titles, workspace archival, or daemon/dispatch failures |
| **BAS Management** (`Bas.Attach`) | BAS-owned Session activity state, BAS-managed titles, workspace/archival updates, Agent process lifecycle, and service errors | Streamed conversation chunks or fabricated Agent responses |
| **Commands & Callbacks** (`bas.v1.AcpTransport`) | Prompt submission, explicit cancellation, authentication, and interactive permission/elicitation replies (prepared with `callbacks_only` when paired with `ObserveAcp` to avoid duplicate replay) | Duplicate history replay |

- **Error attribution**: An Agent protocol error arrives on `ObserveAcp`. A dispatch failure, crashed Agent process, or BAS restart arrives on `Attach` and marks an interrupted turn in the UI without fabricating an Agent message.
- **No optimistic prompt insertion**: Prompt submission returns an admission receipt containing the outgoing ACP request ID, confirming admission to BAS—not Agent completion. The UI renders user prompts and replies exclusively from the observed stream, regardless of whether `ObserveAcp` delivers the admitted input before or after the submission receipt returns.
- **Shared interactive requests**: Permission or elicitation prompts may be delivered to multiple attached frontends. BAS forwards the first valid reply to the Agent and retires the remaining deliveries. Disconnecting a frontend removes only its own subscriptions without cancelling running Agent turns or pending interactions.

## History Restoration & Stream Continuity

1. **Paginated history queries**: Opening, restoring, or scrolling a session queries a finite conversation window from BAS along with the observation cursor through which that window is complete.
2. **Seamless handoff**: `ObserveAcp` resumes strictly after the history cursor, preserving in-progress message and tool identities across the boundary. Historical permission records render as past observations and never re-execute callbacks.
3. **Independent frontiers**: `ObserveAcp` and `Attach` advance their cursors independently—a newer management event never drops an in-transit ACP chunk or advances the content renderer cursor. Local SQLite cache writes sort incoming batches before verifying contiguous history ranges.
4. **Transport keepalive**: All BAS HTTP/2 channels (including lazy channels) send a `PING` after 15 seconds of inactivity with a 5-second ACK timeout. Timeouts trigger cursor-based stream reconnection without resending prompts or polluting conversation history.

## Workspace Provider Launch Forms

Adding a workspace (`features/workspace_launch`) loads provider descriptors from the connected BAS server using JSON Schema 2020-12 (`schemars` / `jsonschema`):

- **Explicit locality (`local: bool`)**: Connection profiles declare whether BAS shares the frontend's local filesystem namespace (true for built-in/discovered local IPC profiles; false by default for saved remote/legacy profiles, even on loopback TCP). GDS never substitutes a local filesystem chooser for a non-local backend.
- **Directory selection**:
  - Single-provider local connections requiring only one `ServerDirectoryPath` open the native directory chooser directly and submit on confirmation.
  - Multi-parameter forms render inside a parent-window alert dialog using `gpui-kit` fields and `DialogActions`, querying provider choice sources dynamically with partial form values, search text, pagination, and schema version checks.
  - Remote `ServerDirectoryPath` fields embed the shared `gds::ui::picker::directory::DirectoryPicker` backed by `BrowseServerDirectories` pagination; closing the dialog cancels in-flight loader futures.
