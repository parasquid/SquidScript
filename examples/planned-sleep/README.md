# Planned Sleep

This app requests native planned sleep from the POWER event, saves state in
the `power.sleep` cleanup handler, and wakes after three seconds. On resume it
starts a fresh VM session with `system.startReason() == "wake"` and increments
the persisted wake count.

Install and launch it on XTEINK X4:

```sh
cargo run -p squidc -- app install examples/planned-sleep/main.squid
cargo run -p squidc -- app launch planned-sleep
```
