# BLE Opcode App-Transfer — End-to-End Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A SquidScript `.sqbc` transferred from a host over BLE to the running, armed `ble-install` app is staged → installed → launched on the XIAO ESP32-C3, with the device staying responsive.

**Architecture:** Keep the custom GATT app-transfer service, but make the control plane fully opcode-driven so no single GATT write exceeds the default 23-byte ATT MTU (this removes the current `REQUEST_NOT_SUPPORTED` blocker without ESP32-C3 MTU/ACL controller tuning). Control char: `BEGIN [0x01|size:u32|name_len:u16]`, `NAME [0x02|name-bytes…]` (repeatable, appended until `name_len`), `ABORT [0x03]`. Data char: raw content chunks (write-without-response). Status char: notify complete/error. The transport-neutral core (`ble_object_transfer.c`) grows a framing demux (`begin_framed` / `feed_name` / `feed_content`) that is unit-tested on native_sim; the GATT shim and host clients are thin layers over it.

**Tech Stack:** Zephyr C (firmware + ztest/native_sim), Python/bleak (CLI client), Web Bluetooth/JS (browser client), SquidScript (example app), `squidc` CLI + esptool (flash/drive the XIAO).

---

## File structure

- `firmware/zephyr/src/ble_object_transfer.{c,h}` — add the framing demux (`begin_framed`/`feed_name`/`feed_content`) + framing-state reset; ensure the staging dir exists. Remove the now-unused `sq_ble_transfer_begin`/`write_chunk` wrappers.
- `firmware/zephyr/src/ble_app_transfer.c` — control-char opcode dispatch (BEGIN/NAME/ABORT), data-char → `feed_content`, status notify on complete/error.
- `firmware/zephyr/tests/ble-ots-dispatch/src/main.c` — native_sim tests for the demux.
- `tools/ots-push/ots_push/client.py` + `tools/ots-push/tests/test_ots_push.py` — opcode framing on the bleak client.
- `tools/ble-web-uploader/index.html` — same framing in the browser client.
- `examples/ble-install/main.squid` — install + launch the received app.
- `examples/hello/main.squid` — NEW tiny payload app (debug.prints on start).
- `scripts/zephyr-test-ble-object-transfer.sh` — drive the full on-device flow.
- `docs/hardware_target_tests.md`, `docs/specs/2026-06-05-ble-object-transfer-design.md` — describe the opcode GATT protocol.

---

## Task 1: Core framing demux (begin_framed / feed_name / feed_content)

**Files:**
- Modify: `firmware/zephyr/src/ble_object_transfer.h` (replace the `feed` decl)
- Modify: `firmware/zephyr/src/ble_object_transfer.c`
- Test: `firmware/zephyr/tests/ble-ots-dispatch/src/main.c`

- [ ] **Step 1: Fix the header declarations** — replace the single `sq_ble_transfer_feed` decl (added earlier for a positional design) with the two-function demux. In `ble_object_transfer.h`, the block currently reads:

```c
int sq_ble_transfer_begin_framed(size_t total_size, size_t name_len);

int sq_ble_transfer_feed(const void *data, size_t len);
```

Replace the second line so it becomes:

```c
int sq_ble_transfer_begin_framed(size_t total_size, size_t name_len);

/* Append object-name bytes (control NAME writes). When name_len bytes have
 * arrived the name is parsed and the staging file opened. */
int sq_ble_transfer_feed_name(const void *data, size_t len);

/* Append content bytes (data-char writes). Completion (total_size reached)
 * publishes the pending event. */
int sq_ble_transfer_feed_content(const void *data, size_t len);
```

- [ ] **Step 2: Write the failing test** — append to `firmware/zephyr/tests/ble-ots-dispatch/src/main.c` (the suite already mounts a LittleFS at `/sqtest` with `/sqtest/tmp`, and `SQ_BLE_OTS_STAGING_DIR="/sqtest/tmp"`):

```c
ZTEST(ble_ots_dispatch, test_framed_name_then_content_one_feed)
{
	const char *name = "installed-app/wallpaper/.sqbc";
	const uint8_t content[] = {'S', 'Q', 'B', 'C', 0x10, 0x20, 0x30, 0x40};
	char app_id[SQ_APP_STORE_APP_ID_MAX] = {0};
	char event[SQ_VM_RUNTIME_EVENT_LEN] = {0};
	struct fs_file_t verify;
	uint8_t readback[8] = {0};
	int result;

	zassert_equal(sq_ble_transfer_begin_framed(sizeof(content), strlen(name)), 0);
	result = sq_ble_transfer_feed_name(name, strlen(name));
	zassert_equal(result, 0, "feed_name failed: %d", result);
	result = sq_ble_transfer_feed_content(content, sizeof(content));
	zassert_equal(result, 0, "feed_content failed: %d", result);

	zassert_true(sq_ble_ots_pending_is_complete(), "transfer should be complete");
	zassert_equal(sq_ble_ots_drain_pending_event(app_id, sizeof(app_id), event, sizeof(event)), 0);
	zassert_str_equal(app_id, "installed-app");

	fs_file_t_init(&verify);
	zassert_equal(fs_open(&verify, sq_ble_ots_pending_staging_path(), FS_O_READ), 0);
	zassert_equal(fs_read(&verify, readback, sizeof(readback)), (ssize_t)sizeof(content));
	(void)fs_close(&verify);
	zassert_mem_equal(readback, content, sizeof(content));
	sq_ble_ots_cleanup_staging();
}

ZTEST(ble_ots_dispatch, test_framed_split_across_name_content_boundary)
{
	const char *name = "installed-app/wallpaper/.sqbc";
	const uint8_t content[16] = {'S', 'Q', 'B', 'C', 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12};
	size_t nl = strlen(name);
	int result;

	zassert_equal(sq_ble_transfer_begin_framed(sizeof(content), nl), 0);
	/* name in two pieces */
	zassert_equal(sq_ble_transfer_feed_name(name, 5), 0);
	zassert_equal(sq_ble_transfer_feed_name(name + 5, nl - 5), 0);
	/* content in three pieces */
	zassert_equal(sq_ble_transfer_feed_content(content, 4), 0);
	zassert_equal(sq_ble_transfer_feed_content(content + 4, 8), 0);
	zassert_equal(sq_ble_transfer_feed_content(content + 12, 4), 0);
	zassert_true(sq_ble_ots_pending_is_complete());
	sq_ble_ots_cleanup_staging();
}

ZTEST(ble_ots_dispatch, test_framed_rejects_content_overrun)
{
	const char *name = "installed-app/wallpaper/.sqbc";
	const uint8_t content[8] = {'S', 'Q', 'B', 'C', 0, 0, 0, 0};

	zassert_equal(sq_ble_transfer_begin_framed(sizeof(content), strlen(name)), 0);
	zassert_equal(sq_ble_transfer_feed_name(name, strlen(name)), 0);
	/* declared 8 bytes; feeding 16 must be rejected, not overrun */
	uint8_t over[16] = {0};

	memcpy(over, content, sizeof(content));
	zassert_equal(sq_ble_transfer_feed_content(over, sizeof(over)), -EFBIG);
	sq_ble_ots_test_invoke_abort();
}
```

- [ ] **Step 3: Run the tests — expect FAIL** (stubs return `-ENOSYS` / functions missing until implemented):

Run: `bash scripts/zephyr-test-ble-ots-dispatch.sh --clobber-output`
Expected: build succeeds, the three new cases FAIL on the first `begin_framed`/`feed_*` assertion (or the suite fails to link if no stub exists — if so, add the stubs in Step 4 first, then re-run to see the assertion failure).

- [ ] **Step 4: Implement the demux** in `ble_object_transfer.c`. Add, after the `#ifndef SQ_BLE_OTS_STAGING_DIR` block, the framing state and `NAME_MAX`:

```c
#define SQ_BLE_TRANSFER_NAME_MAX 96

static struct {
	bool active;
	bool name_done;
	size_t total_size;
	size_t name_len;
	size_t name_got;
	size_t content_off;
	char name[SQ_BLE_TRANSFER_NAME_MAX];
} sq_ble_framed;
```

Add the three functions (place them next to `sq_ble_transfer_abort`):

```c
int sq_ble_transfer_begin_framed(size_t total_size, size_t name_len)
{
	if (name_len == 0 || name_len >= sizeof(sq_ble_framed.name)) {
		return BT_GATT_OTS_OACP_RES_INV_PARAM;
	}
	if (total_size == 0 || total_size > SQ_DEVICE_INSTALL_MAX_BYTES) {
		return BT_GATT_OTS_OACP_RES_INV_PARAM;
	}
	if (sq_ble_ots_session.active || sq_ble_framed.active) {
		return BT_GATT_OTS_OACP_RES_OBJ_LOCKED;
	}
	memset(&sq_ble_framed, 0, sizeof(sq_ble_framed));
	sq_ble_framed.active = true;
	sq_ble_framed.total_size = total_size;
	sq_ble_framed.name_len = name_len;
	return 0;
}

int sq_ble_transfer_feed_name(const void *data, size_t len)
{
	int result;

	if (!sq_ble_framed.active || sq_ble_framed.name_done || data == NULL) {
		return -EINVAL;
	}
	if (sq_ble_framed.name_got + len > sq_ble_framed.name_len) {
		return BT_GATT_OTS_OACP_RES_INV_PARAM;
	}
	memcpy(sq_ble_framed.name + sq_ble_framed.name_got, data, len);
	sq_ble_framed.name_got += len;
	if (sq_ble_framed.name_got == sq_ble_framed.name_len) {
		sq_ble_framed.name[sq_ble_framed.name_len] = '\0';
		result = sq_ble_ots_obj_created_internal(sq_ble_framed.name,
							 sq_ble_framed.total_size);
		if (result != 0) {
			memset(&sq_ble_framed, 0, sizeof(sq_ble_framed));
			return result;
		}
		sq_ble_framed.name_done = true;
	}
	return 0;
}

int sq_ble_transfer_feed_content(const void *data, size_t len)
{
	size_t rem;
	int result;

	if (!sq_ble_framed.active || !sq_ble_framed.name_done || data == NULL) {
		return -EINVAL;
	}
	if (sq_ble_framed.content_off + len > sq_ble_framed.total_size) {
		return -EFBIG;
	}
	rem = sq_ble_framed.total_size - (sq_ble_framed.content_off + len);
	result = sq_ble_ots_obj_write_internal(sq_ble_ots_session.staging_path, data, len,
					       (off_t)sq_ble_framed.content_off, rem);
	if (result < 0) {
		return result;
	}
	sq_ble_framed.content_off += len;
	if (rem == 0) {
		sq_ble_framed.active = false;
	}
	return 0;
}
```

Keep `sq_ble_transfer_begin` and `sq_ble_transfer_write_chunk` for now (the GATT shim still calls them until Task 3 — removing them here would break the firmware build between tasks). They are removed in Task 3 once the shim moves to the framed API. The new framed functions call the `*_internal` helpers directly.

Add framing reset to both `sq_ble_ots_reset_session()` and `sq_ble_ots_abort_internal()` — insert `memset(&sq_ble_framed, 0, sizeof(sq_ble_framed));` after each clears the session.

- [ ] **Step 5: Run the tests — expect PASS**

Run: `bash scripts/zephyr-test-ble-ots-dispatch.sh --clobber-output`
Expected: all `ble_ots_dispatch` cases PASS (the three new + the existing ones).

- [ ] **Step 6: Commit**

```bash
git add firmware/zephyr/src/ble_object_transfer.c firmware/zephyr/src/ble_object_transfer.h \
        firmware/zephyr/tests/ble-ots-dispatch/src/main.c
git commit -m "feat(zephyr): opcode-framed BLE transfer demux (begin_framed/feed_name/feed_content)"
```

---

## Task 2: Ensure the staging directory exists

The native_sim tests pre-create `/sqtest/tmp`; a fresh device has no `/sq/tmp`, so `obj_created`'s `fs_open(FS_O_CREATE)` would fail with `-ENOENT`. Make the staging open create the parent dir.

**Files:** Modify `firmware/zephyr/src/ble_object_transfer.c`; Test `firmware/zephyr/tests/ble-ots-staging/src/main.c`.

- [ ] **Step 1: Failing test** — append to `ble-ots-staging/src/main.c` a case that removes the tmp dir first:

```c
ZTEST(ble_ots_staging, test_obj_created_creates_missing_tmp_dir)
{
	char staging_path[128] = {0};
	int result;

	/* Remove the tmp dir the fixture created, to mimic a fresh device. */
	(void)fs_unlink("/sqtest/tmp");
	result = sq_ble_ots_test_invoke_obj_created_with_name("a/b/.sqbc", 16, staging_path,
							      sizeof(staging_path));
	zassert_equal(result, 0, "obj_created should create the staging dir, got %d", result);
	zassert_true(staging_file_exists(staging_path));
}
```

- [ ] **Step 2: Run — expect FAIL** (`-ENOENT` because the dir is gone):
Run: `bash scripts/zephyr-test-ble-ots-staging.sh --clobber-output` → the new case FAILS.

- [ ] **Step 3: Implement** — in `sq_ble_ots_open_staging_file`, create the parent dir before opening. Replace the body with:

```c
static int sq_ble_ots_open_staging_file(struct sq_ble_ots_session *session)
{
	struct fs_file_t file;
	char *slash;
	int result;

	/* Ensure the parent directory exists (e.g. /sq/tmp on a fresh device). */
	slash = strrchr(session->staging_path, '/');
	if (slash != NULL && slash != session->staging_path) {
		char dir[SQ_BLE_OTS_PATH_MAX];
		size_t dir_len = (size_t)(slash - session->staging_path);

		if (dir_len < sizeof(dir)) {
			memcpy(dir, session->staging_path, dir_len);
			dir[dir_len] = '\0';
			result = fs_mkdir(dir);
			if (result != 0 && result != -EEXIST) {
				return result;
			}
		}
	}

	fs_file_t_init(&file);
	result = fs_open(&file, session->staging_path, FS_O_CREATE | FS_O_WRITE | FS_O_TRUNC);
	if (result != 0) {
		return result;
	}
	return fs_close(&file);
}
```

- [ ] **Step 4: Run — expect PASS:** `bash scripts/zephyr-test-ble-ots-staging.sh --clobber-output`

- [ ] **Step 5: Commit**

```bash
git add firmware/zephyr/src/ble_object_transfer.c firmware/zephyr/tests/ble-ots-staging/src/main.c
git commit -m "fix(zephyr): create the BLE staging parent dir if missing"
```

---

## Task 3: GATT shim — opcode control plane

**Files:** Modify `firmware/zephyr/src/ble_app_transfer.c`.

- [ ] **Step 1: Replace the opcode constants** — set the control opcodes to BEGIN/NAME/ABORT:

```c
#define SQ_XFER_OP_BEGIN 0x01
#define SQ_XFER_OP_NAME  0x02
#define SQ_XFER_OP_ABORT 0x03
```

- [ ] **Step 2: Rewrite `sq_xfer_ctrl_write`** to dispatch opcodes and call the framed core. Replace the function body with:

```c
static ssize_t sq_xfer_ctrl_write(struct bt_conn *conn, const struct bt_gatt_attr *attr,
				  const void *buf, uint16_t len, uint16_t offset, uint8_t flags)
{
	const uint8_t *bytes = buf;
	uint8_t op;

	ARG_UNUSED(conn);
	ARG_UNUSED(attr);
	ARG_UNUSED(offset);
	ARG_UNUSED(flags);

	if (len < 1) {
		return BT_GATT_ERR(BT_ATT_ERR_INVALID_ATTRIBUTE_LEN);
	}
	op = bytes[0];

	if (op == SQ_XFER_OP_BEGIN) {
		uint32_t size;
		uint16_t name_len;

		if (len != 7) {
			return BT_GATT_ERR(BT_ATT_ERR_INVALID_ATTRIBUTE_LEN);
		}
		size = sys_get_le32(&bytes[1]);
		name_len = sys_get_le16(&bytes[5]);
		if (sq_ble_transfer_begin_framed(size, name_len) != 0) {
			sq_xfer_notify_status(SQ_XFER_STATUS_ERROR);
			return BT_GATT_ERR(BT_ATT_ERR_WRITE_NOT_PERMITTED);
		}
		LOG_INF("xfer begin size=%u name_len=%u", size, name_len);
		return len;
	}

	if (op == SQ_XFER_OP_NAME) {
		if (sq_ble_transfer_feed_name(&bytes[1], (size_t)len - 1u) != 0) {
			sq_ble_transfer_abort();
			sq_xfer_notify_status(SQ_XFER_STATUS_ERROR);
			return BT_GATT_ERR(BT_ATT_ERR_WRITE_NOT_PERMITTED);
		}
		return len;
	}

	if (op == SQ_XFER_OP_ABORT) {
		sq_ble_transfer_abort();
		return len;
	}

	return BT_GATT_ERR(BT_ATT_ERR_VALUE_NOT_ALLOWED);
}
```

Add `#include <zephyr/sys/byteorder.h>` if not already present (for `sys_get_le16`).

- [ ] **Step 3: Rewrite `sq_xfer_data_write`** to feed content and notify on completion:

```c
static ssize_t sq_xfer_data_write(struct bt_conn *conn, const struct bt_gatt_attr *attr,
				  const void *buf, uint16_t len, uint16_t offset, uint8_t flags)
{
	int result;

	ARG_UNUSED(conn);
	ARG_UNUSED(attr);
	ARG_UNUSED(offset);
	ARG_UNUSED(flags);

	result = sq_ble_transfer_feed_content(buf, len);
	if (result != 0) {
		sq_ble_transfer_abort();
		sq_xfer_notify_status(SQ_XFER_STATUS_ERROR);
		return BT_GATT_ERR(BT_ATT_ERR_UNLIKELY);
	}
	if (sq_ble_ots_pending_is_complete()) {
		LOG_INF("xfer content complete");
		sq_xfer_notify_status(SQ_XFER_STATUS_COMPLETE);
	}
	return len;
}
```

Remove the now-unused `sq_xfer_state` struct and `SQ_XFER_NAME_MAX` (state lives in the core now). The status notify helper and CCC stay. Now that the shim no longer calls them, also delete `sq_ble_transfer_begin` and `sq_ble_transfer_write_chunk` from `ble_object_transfer.c` and their decls from `ble_object_transfer.h` (kept alive through Task 1 only for build continuity).

- [ ] **Step 4: Build the firmware to verify it links**

Run: `ESPFLASH_PORT=/dev/ttyACM0 cargo run --quiet -p squidc -- target build --target xiao-esp32c3-gdeq0426t82-sd 2>&1 | tail -3`
Expected: `Linking C executable zephyr/zephyr.elf` and a DRAM line; no errors.

- [ ] **Step 5: Commit**

```bash
git add firmware/zephyr/src/ble_app_transfer.c
git commit -m "feat(zephyr): opcode control plane for the GATT app-transfer service"
```

---

## Task 4: bleak client — opcode framing

**Files:** Modify `tools/ots-push/ots_push/client.py`; Test `tools/ots-push/tests/test_ots_push.py`.

- [ ] **Step 1: Update the failing tests first** — in `test_ots_push.py`, change the framing expectations. Replace `build_begin_command` usage and add a NAME-chunk expectation. The happy-path test should assert: one BEGIN control write `[0x01|size:LE32|name_len:LE16]`, one-or-more NAME control writes `[0x02|name-bytes]` whose concatenated payloads equal the object name, then data writes summing to the content length. Update `test_build_begin_command_frames_opcode_size_name` to:

```python
def test_build_begin_command_frames_opcode_size_namelen():
    cmd = build_begin_command(0x01020304, 30)
    assert cmd[0] == OP_BEGIN
    assert cmd[1:5] == bytes([0x04, 0x03, 0x02, 0x01])  # size, little-endian
    assert cmd[5:7] == bytes([30, 0x00])                # name_len, little-endian
```

In `test_push_happy_path_writes_begin_then_chunks`, after asserting the BEGIN write, add:

```python
    name_writes = [w for w in client.writes if w[0] == CTRL_UUID and w[1][0] == OP_NAME]
    assert b"".join(w[1][1:] for w in name_writes) == b"ble-install/sqbc-install/.sqbc"
    begin = next(w[1] for w in client.writes if w[0] == CTRL_UUID and w[1][0] == OP_BEGIN)
    assert int.from_bytes(begin[1:5], "little") == len(payload)
    assert int.from_bytes(begin[5:7], "little") == len(b"ble-install/sqbc-install/.sqbc")
```

- [ ] **Step 2: Run — expect FAIL:** `cd tools/ots-push && PYTEST_DISABLE_PLUGIN_AUTOLOAD=1 python3 -m pytest -q` → the framing tests fail.

- [ ] **Step 3: Implement the new framing** in `client.py`. Add `OP_NAME = 0x02` (and keep `OP_BEGIN=0x01`, `OP_ABORT=0x03`). Replace `build_begin_command`:

```python
def build_begin_command(size: int, name_len: int) -> bytes:
    return bytes([OP_BEGIN]) + int(size).to_bytes(4, "little") + int(name_len).to_bytes(2, "little")
```

In `_push_via_gatt`, replace the BEGIN+data section with BEGIN, NAME chunks, then DATA chunks:

```python
        name_bytes = object_name.encode("utf-8")
        await client.start_notify(STAT_UUID, on_status)
        await client.write_gatt_char(CTRL_UUID, build_begin_command(file_size, len(name_bytes)),
                                     response=True)
        NAME_CHUNK = 18  # safe under the 23-byte default ATT MTU
        for off in range(0, len(name_bytes), NAME_CHUNK):
            await client.write_gatt_char(CTRL_UUID, bytes([OP_NAME]) + name_bytes[off:off + NAME_CHUNK],
                                         response=True)
        chunk = _resolve_chunk(client)
        sent = 0
        while sent < len(payload):
            await client.write_gatt_char(DATA_UUID, payload[sent:sent + chunk], response=False)
            sent += chunk
        sent = min(sent, len(payload))
```

(`push_file` already reads the whole file into `payload`; keep that. Remove the old single-`BEGIN`-with-name code.)

- [ ] **Step 4: Run — expect PASS:** `cd tools/ots-push && PYTEST_DISABLE_PLUGIN_AUTOLOAD=1 python3 -m pytest -q`

- [ ] **Step 5: Commit**

```bash
git add tools/ots-push/ots_push/client.py tools/ots-push/tests/test_ots_push.py
git commit -m "feat(tools): opcode framing (BEGIN/NAME/data) in the bleak client"
```

---

## Task 5: Web Bluetooth uploader — opcode framing

**Files:** Modify `tools/ble-web-uploader/index.html`.

- [ ] **Step 1: Update the JS framing.** Replace `beginCommand` and the upload body. New `beginCommand`:

```js
function beginCommand(size, nameLen) {
  const buf = new Uint8Array(7);
  buf[0] = OP_BEGIN;                               // 0x01
  const dv = new DataView(buf.buffer);
  dv.setUint32(1, size, true);                     // size, little-endian
  dv.setUint16(5, nameLen, true);                  // name_len, little-endian
  return buf;
}
```

Add `const OP_NAME = 0x02;`. In `upload()`, after `startNotifications`, replace the BEGIN + data loop with:

```js
  const nameBytes = new TextEncoder().encode(objectName);
  await ctrl.writeValue(beginCommand(bytes.length, nameBytes.length));
  const NAME_CHUNK = 18;
  for (let off = 0; off < nameBytes.length; off += NAME_CHUNK) {
    const c = new Uint8Array(1 + Math.min(NAME_CHUNK, nameBytes.length - off));
    c[0] = OP_NAME;
    c.set(nameBytes.subarray(off, off + NAME_CHUNK), 1);
    await ctrl.writeValue(c);
  }
  for (let off = 0; off < bytes.length; off += CHUNK) {
    await data.writeValueWithoutResponse(bytes.subarray(off, off + CHUNK));
  }
```

- [ ] **Step 2: Syntax-check** (no build step; just verify the file parses by opening it, or run a JS linter if available). Commit:

```bash
git add tools/ble-web-uploader/index.html
git commit -m "feat(tools): opcode framing in the Web Bluetooth uploader"
```

---

## Task 6: Example installs AND launches; add a distinct payload app

**Files:** Modify `examples/ble-install/main.squid`; Create `examples/hello/main.squid`.

- [ ] **Step 1: Create the payload app** `examples/hello/main.squid`:

```
app "hello"

event.on("app.start") {
  debug.print("hello from installed app")
}
```

- [ ] **Step 2: Make the receiver install then launch.** In `examples/ble-install/main.squid`, change the complete handler so it launches after install:

```
event.on("ble.object.complete", ev) {
  state.load()
  state.received = state.received + 1
  app.install(ev.upload, "installed-app")
  app.launch("installed-app")
  state.installed = state.installed + 1
  state.save()
}
```

- [ ] **Step 3: Compile both to verify the language accepts it**

```bash
cargo run --quiet -p squidc -- app build examples/ble-install/main.squid --out /tmp/ble-install.sqbc
cargo run --quiet -p squidc -- app build examples/hello/main.squid --out /tmp/hello.sqbc
ls -l /tmp/ble-install.sqbc /tmp/hello.sqbc
```
Expected: both produce `.sqbc` files; no compile error.

- [ ] **Step 4: Commit**

```bash
git add examples/ble-install/main.squid examples/hello/main.squid
git commit -m "feat(examples): ble-install launches the received app; add hello payload"
```

---

## Task 7: On-device end-to-end verification (the DoD)

This is the acceptance test. Hardware: XIAO on `/dev/ttyACM0`, host adapter `hci0`.

- [ ] **Step 1: Flash and confirm responsive + advertising**

```bash
export ESPFLASH_PORT=/dev/ttyACM0
cargo run -p squidc -- target flash --target xiao-esp32c3-gdeq0426t82-sd
sleep 3
cargo run -p squidc -- app list           # responds promptly (DoD #7 baseline)
```

- [ ] **Step 2: Install + launch the receiver (arms it)**

```bash
cargo run -p squidc -- app install /tmp/ble-install.sqbc
cargo run -p squidc -- app launch ble-install
```

- [ ] **Step 3: Push the distinct payload over BLE**

```bash
ADDR=$(python3 -c "import asyncio;from bleak import BleakScanner;\
import sys;\
print(asyncio.run(BleakScanner.find_device_by_filter(lambda d,a:'7e57c0de-0001-4a5b-8c6d-0123456789ab' in [u.lower() for u in (a.service_uuids or [])],timeout=10)).address)")
cd tools/ots-push && python3 -m ots_push push "$ADDR" ble-install sqbc-install /tmp/hello.sqbc; cd -
```
Expected: `OK ble-push uploaded …` (DoD #4).

- [ ] **Step 4: Verify install + launch + responsiveness**

```bash
cargo run -p squidc -- app list                 # shows app=hello sqbc_len=<size of hello.sqbc>  (DoD #5)
cargo run -p squidc -- device output            # shows "hello from installed app"  (DoD #6)
cargo run -p squidc -- app list                 # still prompt -> not wedged  (DoD #7)
```
Compare `sqbc_len` to `wc -c /tmp/hello.sqbc` for byte-exactness.

- [ ] **Step 5 (contingency): if the device wedges during the push**, the now-reached write callback runs LittleFS on the BT RX thread (`CONFIG_BT_RECV_WORKQ_BT`, stack 1536). Raise it and re-flash:
add `CONFIG_BT_RX_STACK_SIZE=4096` to `firmware/zephyr/prj.conf`, re-flash, repeat Steps 2-4. Keep the smallest value that is stable; record the chosen value and the observed stack high-water (`device resources` / a stack-usage build) in the commit message. Only commit the stack bump if it was actually needed.

- [ ] **Step 6: Error path** — push a non-SQBC file and confirm `ble.object.error` + responsiveness:

```bash
head -c 64 /dev/urandom > /tmp/bad.bin
cd tools/ots-push && python3 -m ots_push push "$ADDR" ble-install sqbc-install /tmp/bad.bin; cd -
cargo run -p squidc -- device output     # "transfer failed"; (DoD #8)
cargo run -p squidc -- app list          # still responsive
```

- [ ] **Step 7: Repeat without reset** — re-run Steps 3-4 once more; second upload must also succeed (DoD #9).

- [ ] **Step 8: Fold the happy-path into the wrapper** — update `scripts/zephyr-test-ble-object-transfer.sh` so it does flash → install+launch ble-install → push `/tmp/hello.sqbc` (or a built payload) → assert `hello` in `app list`. Commit:

```bash
git add scripts/zephyr-test-ble-object-transfer.sh
git commit -m "test(ble): on-device push->install->launch hardware wrapper"
```

---

## Task 8: No-regression + docs

- [ ] **Step 1: Full native_sim + protocol regression**

```bash
for t in ble-ots-parse ble-ots-staging ble-ots-dispatch ble-trigger-table ble-app-install; do
  bash scripts/zephyr-test-$t.sh --clobber-output 2>&1 | grep -E "executed test cases passed|FAILED|ERROR"
done
bash scripts/zephyr-test-protocol.sh --clobber-output 2>&1 | grep -E "executed test cases passed|FAILED"
```
Expected: all pass.

- [ ] **Step 2: Update docs to the opcode GATT protocol** — in `docs/hardware_target_tests.md` and `docs/specs/2026-06-05-ble-object-transfer-design.md`, describe the control opcodes (BEGIN/NAME/ABORT), the data char, and the status notify; mark the hardware wrapper as the real end-to-end test. Keep comments/docs factual (no references to the removed OTS path). Commit:

```bash
git add docs/hardware_target_tests.md docs/specs/2026-06-05-ble-object-transfer-design.md
git commit -m "docs(ble): describe the opcode GATT app-transfer protocol"
```

- [ ] **Step 3: Close out the tracker** — mark task #11 (the MTU blocker) resolved with the opcode protocol; note the chosen `CONFIG_BT_RX_STACK_SIZE` if Step 7.5 was needed.

---

## Notes / known risks
- The data char still streams at ~20 B/chunk under the default MTU (slow for large apps but correct). A future MTU-negotiation optimization can raise throughput without changing this protocol — out of scope here.
- This is the transfer mechanism only; the foreground-gated advertising/authorization design (ROADMAP, Runtime Services) is the next layer and builds on this path.
