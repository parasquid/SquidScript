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

- `service.ble.start(profile, config)` — **set** the calling app's BLE receive
  to this profile: register it in the routing table and begin advertising the
  service UUID. The `config` object is the same shape used by the old
  declaration (`id`, `accept`, `events`). **One receive per app** and `start` is
  **idempotent** — calling it again re-applies the config (same config is a
  no-op; a changed config replaces the prior one). It never errors on a second
  call, which is what makes putting `start` in `app.start` safe across
  re-launches (see Re-launch semantics).
- `service.ble.stop()` — clear the calling app's profile, abort any in-flight
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
- **Routing/dispatch.** A completed transfer routes by uploaded file extension
  to the installed app or target fallback app that started the matching profile,
  via the pending-event poll-drain path. Firmware stores the app registry slot
  or reserved fallback slot in the active BLE route and resolves the app-id text
  only when dispatching the completion event. The active route table must map an
  uploaded extension to exactly one receiver; ambiguous or stale route state is
  reported through `device errors` as an invariant diagnostic and the transfer is
  rejected.
- **In-flight transfer on stop.** `stop` aborts any partially received object
  and cleans its staging file (reusing `sq_ble_transfer_abort` /
  `reset_session`, which already preserve a *completed* pending event).
- **Caps unchanged.** `SQ_VM_RUNTIME_BLE_PROFILE_MAX` bounds total registered
  profiles (see `docs/runtime_limits.md`).

### Re-launch semantics (resolved)

Two dispatch paths matter:

- **Explicit launch** (`app.launch`, opening the app) dispatches `app.start`.
- **A pushed file to an exited-but-persisted app** dispatches the configured
  event (`ble.object.complete`) directly — **not** `app.start`.

So the only place `start` runs twice is an **explicit re-launch**, because
`app.start` re-runs and re-calls `start` while the persisted profile still
exists. A background push never re-runs `app.start`, so it never re-calls
`start`. Because `start` is idempotent (set/replace), the re-launch case is a
clean re-apply — no error, and no implicit "clear on launch" rule is needed.

### Activation requires running the app once (consequence)

The profile is created by *running* `service.ble.start`, not by reading the
compiled trigger table. So after a device reset, BLE receive is inactive until
the owning app is launched once and runs `start`. This is the intended
"explicit/imperative" behavior and differs from the old armed model, which
registered profiles from boot.

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

## Resolved decisions

- **One receive per app.** No profile-id argument on `stop`; no multi-profile
  bookkeeping. A second `start` is an idempotent set/replace, not an error.
- **`start` is idempotent (set/replace).** Re-running it (e.g. on re-launch)
  re-applies the config; it never errors.
- **Persist across exit; app decides cleanup.** Exiting does not auto-stop;
  the app calls `service.ble.stop()` if it wants to. A started profile
  auto-clears when its owning app is uninstalled.
