# Spawn straight into a presence

`presence spawn` is the way a client arrives. If the daemon is down, it starts, then attaches a presence, and the figure is the first thing on screen. If the daemon is already up, it attaches a presence and does not start a second process. One body today. The same two branches are what a later one-per-client attach uses.

The sleep-watch leak is part of this, because a start that can be repeated has to take its monitor with it when it exits. Not started.

## The command

Top level, beside `daemon`:

```
presence spawn [--context <text>] [--style <hint>] [--artifact <id>]
```

Stdout is only the presence id, same as today, so `ID=$(presence spawn)` still works.

`presence daemon start` stays the observe path. It brings the daemon up with no presence, and the crystal ball is what you see. `presence presence spawn` stays the socket-only verb: it attaches, and it fails if the daemon is down.

`presence spawn`, `presence speak`, `presence stop`, and `presence retire` already reach that same socket path. The help text used to print those words and then only accept the doubled form, so `presence speak "$ID" "Hello"` opened the menu. A missing id also opened the menu. Both of those now say what was missing, or they send the proposal.

What this task still adds is the start. `presence spawn` does not yet bring the daemon up, and it does not yet hold the ball off the screen. That is the remaining work. The doubled form does not grow a second meaning.

The flags match the socket verb. The CLI still talks to the daemon directly and still bypasses the MCP policy gate. The six MCP tools keep their names. This task does not make `spawnPresence` launch a daemon.

## Do not show the ball on the way in

The layer is created at the observing size. `Overlay::connect` commits 220 (`OBSERVE_PX`) and the run loop presents the crystal ball as soon as the swapchain exists. `startDaemon` returns when the socket accepts, which is after that layer is up. A client that then sends `spawnPresence` has already lost: the ball was on screen.

`presence spawn` does not stop on that ball, and it does not stop on a large empty disc either. The presence size is 330. The first presented frame on this path is the figure at that size.

When this command is the thing that started the daemon, the daemon comes up knowing a presence is about to bind and does not present until that bind has a frame to show. An argument or an environment value from the CLI is enough. A socket-only start does not pass it, and the ball remains the observe path.

Holding the first present needs a bound. If the client dies between start and attach, the daemon must not sit forever with a layer and no picture. A few seconds with no presence, then either exit or fall through to the observing ball, is the bound. Pick one and test it. Do not leave it implicit.

When the daemon is already up and the ball is showing, attach works as it does now. The layer grows to 330 and the figure replaces the ball. One blank frame while the compositor configures the new size is fine. A presented 330 crystal ball in between is not.

## One presence now, one per client later

There is one avatar body and one layer. The first live presence gets that body. A later `presence spawn` is tracked, prints its id, and warns that it has no body, the same warning `presence presence spawn` already prints (`avatar: false`). It does not start another daemon, and it does not hide the owner.

The command still has two branches, and the second one is the extension point:

- daemon down → start, then attach
- daemon already running → attach another presence

Today the second branch attaches a presence that does not get a body. When a client can own its own body, that branch is where the new body comes from. This task does not split the avatar actor, does not add a second layer, and does not change the cap of three tracked presences.

## The monitor dies with the daemon

`spawn_sleep_watch` in `daemon/src/overlay.rs` starts:

```
stdbuf -oL dbus-monitor --system \
  type='signal',interface='org.freedesktop.login1.Manager',member='PrepareForSleep'
```

`stdbuf` replaces itself, so the process table shows `dbus-monitor` and that argument string. The `Child` lives on that thread's stack. Nothing stores the pid. The thread blocks on stdout.

`presence daemon stop` only signals the daemon. `signalDaemon` in `src/daemonControl.ts` checks `/proc/<pid>/cmdline` for `presence-daemon`, sends `SIGTERM` to that pid, then `SIGKILL` if it is still up. A socket shutdown takes the same exit. The monitor is reparented to systemd --user and runs until reboot.

Seen on 2026-10-12 with one daemon live: ten leftover monitors with our argument string, parents already dead, each at 0% CPU. The live daemon had one child of its own. Another monitor, with `sender='org.freedesktop.login1'` in the argument string, belongs to `omarchy-system-sleep-monitor` and is not ours.

Keep `stdbuf -oL`. Without it, `dbus-monitor` block-buffers the pipe and `PrepareForSleep` arrives after the machine is already suspending. Lock, the screensaver, and sleep still park the layer. Idle still does not.

The child has to die when the parent dies, including on `SIGKILL`, which never runs daemon code. A `Drop` that kills the child does not cover that. Set the parent-death signal in the child after fork and before exec (`PR_SET_PDEATHSIG`, `SIGTERM`), then check the parent pid so a parent that died in that window still takes the child down. The signal survives `stdbuf` execing `dbus-monitor`. That is a normal Linux facility. It is enough.

Do not reap by command name, at startup or at stop. A name match hits Omarchy's monitor. Leftovers already adopted by systemd are not this process's children. Clearing those is a one-time pass over pids whose command line is exactly our invocation and whose parent is not a live `presence-daemon`. The daemon does not do that scan.

## Do not start a second daemon

`startDaemon` refuses only when the socket already accepts a connection. A `presence-daemon` that is still alive after its socket is gone gets a sibling, and a process list groups them under one name. That is how a quiet sit and a second copy were read as one heavier process.

`presence spawn` and `presence daemon start` share one start. It refuses when the pid file names a live process whose `/proc/<pid>/cmdline` is `presence-daemon`, using the check `isPresenceDaemon` already has. It does not signal that process and it does not match the short comm name. If the socket is up, it attaches, or for `daemon start` it reports already running, as it does now.

## Out of scope

No second body, no per-client layer, no change to the MCP tool names or the policy gate. No change to the heat path, the disc sizes, or the hide rule. The monitors sit at 0%. Skipping the still-pose splat upload is a separate plan (`cpu_skin_and_heat.md`). Not started.

## Check

Daemon down. `presence spawn` leaves one `presence-daemon`, one monitor child with our argument string, and the figure on screen. The observing ball is not presented on the way in. `presence daemon start` with no spawn still shows the ball.

Daemon already up, ball showing. `presence spawn` attaches, the figure replaces the ball, and there is still one daemon process.

Daemon already up, a presence owning the body. A second `presence spawn` prints an id and the no-body warning. The owner stays.

Stop through the socket, and separately `SIGKILL` the daemon pid. Our monitor is gone. Omarchy's monitor is still there. A following start has one child, not two.

Start while a pid file already names a live `presence-daemon`. The command refuses, and the process count stays one.
