#![no_std]
//! Shared SquidScript VM resource limits.
//!
//! Single source of truth for the constrained-device runtime caps. The VM
//! (`squidvm-core`) enforces these at load/execute time, and the compiler
//! (`squidc-core`) enforces them when emitting SQBC, so an app that the VM
//! cannot load or run is rejected at compile time rather than failing on-device
//! with an opaque error. Both crates depend on this one so the contract cannot
//! drift.

/// Maximum number of distinct interned strings in a program's string table.
pub const MAX_STRINGS: usize = 128;
/// Maximum number of declared state fields.
pub const MAX_STATE: usize = 16;
/// Maximum number of declared functions.
pub const MAX_FUNCTIONS: usize = 16;
/// Maximum number of event handlers.
pub const MAX_HANDLERS: usize = 16;
/// Maximum number of registered timer triggers.
pub const MAX_TRIGGERS: usize = 16;
/// Maximum UTF-8 byte length of an event name stored by constrained runtimes.
pub const MAX_EVENT_NAME_BYTES: usize = 24;
/// Maximum UTF-8 byte length of an app identifier.
pub const MAX_APP_ID_BYTES: usize = 40;
/// Maximum installed apps in the native app registry.
pub const MAX_INSTALLED_APPS: usize = 8;
/// Maximum suspended foreground app identifiers retained for return.
pub const MAX_PROCESS_STACK: usize = 2;
/// Maximum armed timer registrations across installed apps.
pub const MAX_ARMED_TIMERS: usize = 2;
/// Maximum armed logical-input registrations across installed apps.
pub const MAX_ARMED_INPUTS: usize = 8;
/// Maximum queued timer/input events awaiting bounded dispatch.
pub const MAX_PENDING_EVENTS: usize = 8;
/// Maximum foreground timer registrations in one app session.
pub const MAX_FOREGROUND_TIMERS: usize = 4;
/// Maximum physical logical-button slots exposed by a target.
pub const MAX_INPUT_BUTTONS: usize = 8;
/// Maximum number of device bindings.
pub const MAX_DEVICE_BINDINGS: usize = 8;
/// Maximum number of screens.
pub const MAX_SCREENS: usize = 16;
/// Maximum number of locals per frame.
pub const MAX_LOCALS: usize = 16;
/// Maximum operand stack depth.
pub const MAX_STACK: usize = 16;
/// Maximum function call depth.
pub const MAX_CALL_DEPTH: usize = 4;
/// Maximum VM instructions executed per dispatched event.
pub const MAX_INSTRUCTIONS_PER_EVENT: usize = 1000;
/// Maximum total compiled app (SQBC container) size in bytes.
pub const MAX_APP_BYTES: usize = 8 * 1024;
/// Maximum total bytes of interned program string content.
pub const MAX_PROGRAM_STRING_BYTES: usize = 1536;
/// Maximum compiled code bytes for a single frame (handler/function/screen).
/// The VM loads a whole frame into one code-chunk buffer, so a frame larger
/// than this cannot be executed.
pub const MAX_CODE_CHUNK_BYTES: usize = 640;
/// Maximum saved app-state blob size in bytes.
pub const MAX_SAVED_STATE_BYTES: usize = 512;
/// Maximum runtime (dynamically constructed) strings.
pub const MAX_RUNTIME_STRINGS: usize = 12;
/// Maximum bytes per runtime string.
pub const MAX_RUNTIME_STRING_BYTES: usize = 128;
/// Maximum service-result strings.
pub const MAX_SERVICE_STRINGS: usize = 32;
/// Maximum bytes per service-result string.
pub const MAX_SERVICE_STRING_BYTES: usize = 512;
/// Maximum runtime list values.
pub const MAX_RUNTIME_LISTS: usize = 2;
/// Maximum items per runtime list.
pub const MAX_RUNTIME_LIST_ITEMS: usize = 8;
/// Maximum runtime record values.
///
/// A full service result page needs one summary record plus one record for
/// each item in `MAX_RUNTIME_LIST_ITEMS`.
pub const MAX_RUNTIME_RECORDS: usize = MAX_RUNTIME_LIST_ITEMS + 1;
/// Maximum fields per runtime record.
pub const MAX_RUNTIME_RECORD_FIELDS: usize = 26;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_product_caps_match_the_runtime_contract() {
        assert_eq!(MAX_INSTALLED_APPS, 8);
        assert_eq!(MAX_PROCESS_STACK, 2);
        assert_eq!(MAX_ARMED_TIMERS, 2);
        assert_eq!(MAX_ARMED_INPUTS, 8);
        assert_eq!(MAX_PENDING_EVENTS, 8);
        assert_eq!(MAX_FOREGROUND_TIMERS, 4);
        assert_eq!(MAX_INPUT_BUTTONS, 8);
        assert_eq!(MAX_EVENT_NAME_BYTES, 24);
        assert_eq!(MAX_APP_ID_BYTES, 40);
    }
}
