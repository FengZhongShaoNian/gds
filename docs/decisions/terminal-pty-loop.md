# Terminal PTY Loop & Directory Policy

## 1. Custom Tokio PTY Worker Instead of Upstream `alacritty_terminal` Event Loop

GDS uses `alacritty_terminal = 0.26.0` for its grid, selection, and ANSI/UTF-8 parser, and `portable-pty = 0.9.0` for PTY allocation, without adopting `alacritty_terminal::event_loop::EventLoop` or `tty::unix` (evaluated against upstream commit [`94e7c887`](https://github.com/alacritty/alacritty/blob/94e7c8874e526b1e67b349d9ba30ddf81669119e/alacritty_terminal/src/event_loop.rs)).

### Why Upstream's Loop Was Not Adopted

| Upstream `alacritty_terminal` Behavior | Required GDS Service Contract |
| --- | --- |
| `EventLoop::channel()` uses an unbounded `mpsc` feeding a private FIFO without public write-progress acknowledgments. | Byte-bounded input admission until bytes actually flush to the PTY, with priority scheduling so terminal OSC/DSR replies precede queued user keystrokes without interleaving partial messages. |
| Event callbacks fire while the `Term` mutex is held, with no post-parse query hook. | Dynamic OSC color queries must inspect emulator overrides first and latest theme defaults second without re-locking `Term` inside a callback. |
| `OnResize` returns `()` and I/O failures inside `EventLoop::spawn` log directly rather than returning structured outcomes. | Startup, resize, and transport failures must propagate to `service::terminal` status and completion barriers. |
| `tty::unix::from_fd` builds its own `Command`, ignores failed `chdir`, and `Pty::drop` signals only the direct child. | Exact `argv`, verified working-directory failure reporting, inherited FD/signal-mask hygiene, and Linux session process-group cleanup (`SIGHUP` + `SIGCONT` → grace period → re-enumerated `SIGKILL` + child reap). |

Wrapping upstream's event loop to preserve these invariants would require parallel queues, a custom `EventedPty` shim, and duplicate process supervision—increasing complexity rather than reducing it. Instead, `service::terminal` runs one Tokio current-thread worker per PTY (`AsyncFd` + `Notify` + `SIGCHLD` + parser synchronized-update deadlines).

- **Retirement condition**: Revisit only if upstream `alacritty_terminal` exposes bounded priority write queues, lock-free post-parse hooks, structured I/O error outcomes, and pluggable process-group teardown.

## 2. New Terminal Tab vs Injecting `cd` on Folder Navigation

- **Context**: File managers with embedded terminals often attempt to sync the shell's working directory with folder navigation by writing `cd '/new/path'\n` into the PTY master.
- **Compromise**: GDS Files **never** injects `cd` or `Enter` into an existing PTY session, and shell `chdir` never drives file-browser navigation. Instead, the terminal header action **Open current folder in new terminal tab** spawns a fresh shell with its child process `cwd` set cleanly via `std::process::Command::current_dir`.
- **Why**: Without invasive per-shell prompt integration scripts, a host cannot reliably know whether the foreground process is an idle shell prompt, `vim`, an interactive `git rebase`, a password prompt, or a partially typed user command—where injected keystrokes corrupt input or execute unintended commands.
