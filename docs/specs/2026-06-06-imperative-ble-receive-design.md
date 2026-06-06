# Imperative BLE Object Receive — Design

**Goal:** Replace the declarative, armed-gated BLE object-transfer trigger with an
imperative service the app starts and stops itself, so BLE receive is under
explicit app control rather than implied by arming.

**Status:** Proposed (pre-implementation). Supersedes the roadmap's
"foreground-gated BLE receive" item.

## Motivation

Today an app declares BLE receive in its trigger block:

```squid
app.triggers {
  service.ble.profile("object-transfer", {
    id: "sqbc-install",
    accept: [".sqbc"],
    events: { complete: "ble.object.complete", error: "ble.object.error" }
  })
}
```

The profile is registered when the app is **armed** (a background concept), and
the radio advertises unconditionally from boot. Two problems:

1. `app.triggers` should be reserved for *actual triggers* (timers, input). A
   long-lived radio capability is not a trigger; it is a runtime service the app
   turns on and off.
2. Arming a *background* app to receive couples BLE receive to the armed-app
   lifecycle. That coupling is the direct cause of the `app.launch` → `-5`
   failure observed when an armed foreground app installs and launches another
   app: launching out of an *armed* foreground app is the broken transition;
   launching out of a *normal* foreground app works.

## Proposed API

BLE object receive becomes an imperative `service.ble.*` capability the app
drives directly — consistent with `service.wifi.*`, `service.timer.*`, etc.

```squid
event.on("app.start") {
  service.ble.start("object-transfer", {
    id: "sqbc-install",
    accept: [".sqbc"],
    events: { complete: "ble.object.complete", error: "ble.object.error" }
  })
}

event.on("ble.object.complete", ev) {
  app.install(ev.upload, "installed-app")
  app.launch("installed-app")
}
```

- `service.ble.start(profile, config)` — register the profile in the routing
  table and begin advertising the service UUID. The `config` object is the same
  shape used by the old declaration (`id`, `accept`, `events`).
- `service.ble.stop()` — clear the calling app's profile(s), abort any in-flight
  transfer, and stop advertising if no profiles remain.

`service.ble.profile(...)` inside `app.triggers` is **removed** (pre-1.0:
replace directly, no alias or migration).

## Semantics

- **Explicit, persistent state.** `start` activates receive; it stays active
  until `stop`, a device reset, or the profile cap is otherwise managed. App
  exit does **not** auto-stop — the app decides whether to `service.ble.stop()`
  on the way out (e.g. in a teardown handler) or leave the radio running.
- **Advertising is gated on active profiles.** The radio advertises the transfer
  service UUID only while ≥1 profile is registered; it stops when the last
  profile is cleared. (Boot no longer advertises unconditionally.)
- **Routing/dispatch is unchanged.** A completed transfer routes by object name
  (`<app_id>/<profile_id>/<.ext>`) to the app that started the matching profile,
  via the existing pending-event → poll-drain → `START_APP` path. If that app is
  not currently running, it is started to handle the event, exactly as today.
- **In-flight transfer on stop.** `stop` aborts any partially received object
  and cleans its staging file (reusing `sq_ble_transfer_abort` /
  `reset_session`, which already preserve a *completed* pending event).
- **Caps unchanged.** `SQ_VM_RUNTIME_BLE_PROFILE_ARMED_MAX` continues to bound
  total registered profiles; rename to drop "ARMED" from the macro name since the
  gating is no longer armed-based (see `docs/runtime_limits.md`).

## Why this dissolves the launch `-5`

The receiving app (`ble-install`) is now an ordinary foreground app that started
a service — it is **not armed**. After `app.install`, `app.launch("installed-app")`
is a normal foreground→foreground transition, which is verified to work. The
armed-app launch path that produced `-5` is no longer exercised by this flow.

## Layers touched (implementation slices)

1. **Compiler** (`squidc-core`): parse `service.ble.start`/`service.ble.stop`;
   IR `ServiceBleStart { profile, id, accept, events }` / `ServiceBleStop`;
   remove `ServiceBleProfile` from the `app.triggers` block; SQBC encoding;
   semantic visiting. Compiler fixtures/tests.
2. **VM** (`squidvm-core`): `BUILTIN_SERVICE_BLE_START` / `_STOP`, host-trait
   methods, dispatch. Add the constants to the `bytecode::{...}` import list
   (shadowing canary). VM tests.
3. **FFI** (`squidvm-ffi`): callbacks + ABI manifest entries + regenerated C
   helpers; FFI dispatch equivalence tests.
4. **Firmware** (`firmware/zephyr`): host callbacks register/clear the profile
   table and start/stop advertising; remove the armed-step BLE profile
   registration (`device_protocol.c`); advertising no longer auto-starts at boot.
   ztests + hardware run.
5. **Example + docs:** `examples/ble-install/main.squid` uses
   `service.ble.start`; update `docs/language_spec.md`,
   `docs/hardware_target_tests.md`, `docs/runtime_limits.md`; icebox/replace the
   foreground-gated roadmap entry.

## Verification

- Compiler: `cargo test` (start/stop parse + lower; old `service.ble.profile`
  no longer parses).
- VM: source→VM→host test that `service.ble.start`/`stop` reach the host with the
  profile config; dispatch of `ble.object.complete` after a started profile.
- Firmware: ztests for register-on-start / clear-on-stop / advertising gate.
- Hardware (XIAO): `service.ble.start` in `app.start`, push a `.sqbc`, confirm
  install byte-exact **and** the installed app launches (DoD #6) without `-5`,
  then `service.ble.stop` (or app exit) stops advertising.

## Open questions

- Should `service.ble.stop()` take an optional profile id to stop one of several,
  or always stop all of the calling app's profiles? (Default: stop all of the
  caller's; revisit if multi-profile apps appear.)
- Should a started profile auto-clear when its owning app is uninstalled?
  (Default: yes, on uninstall; otherwise persists.)
