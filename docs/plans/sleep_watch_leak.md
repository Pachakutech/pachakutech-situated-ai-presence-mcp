# Reap the sleep-watch monitor

This is one part of [presence_spawn.md](complete/presence_spawn.md). That task is the one to implement: `presence spawn` starts the daemon when it is down and shows the figure first, and the sleep-watch monitor dies with the daemon, including on `SIGKILL`.

The leak itself is unchanged. `spawn_sleep_watch` leaves a `dbus-monitor` that `presence daemon stop` does not signal, systemd --user adopts it, and a parent-death signal on the child is what covers the kill. Do not reap by command name. Omarchy has its own `PrepareForSleep` monitor.
