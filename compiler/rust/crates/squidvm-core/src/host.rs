use core::fmt;

use crate::{
    error::VmError,
    limits::{MAX_CODE_CHUNK_BYTES, MAX_SAVED_STATE_BYTES},
    strings::StringResolver,
    value::Value,
};

pub const MAX_STORAGE_TRANSFER_BYTES: usize = if MAX_CODE_CHUNK_BYTES > MAX_SAVED_STATE_BYTES {
    MAX_CODE_CHUNK_BYTES
} else {
    MAX_SAVED_STATE_BYTES
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StorageRequest {
    SqbcRead { offset: usize, len: usize },
    StateLoad,
    StateSave { len: usize, bytes: *const u8 },
    StateReset,
}

impl StorageRequest {
    pub const fn sqbc_read(offset: usize, len: usize) -> Self {
        Self::SqbcRead { offset, len }
    }

    pub const fn state_load() -> Self {
        Self::StateLoad
    }

    pub fn state_save(bytes: &[u8]) -> Result<Self, VmError> {
        if bytes.len() > MAX_SAVED_STATE_BYTES {
            return Err(VmError::InvalidStateRecord);
        }
        Ok(Self::StateSave {
            len: bytes.len(),
            bytes: bytes.as_ptr(),
        })
    }

    pub const fn state_reset() -> Self {
        Self::StateReset
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StorageCompletion<'a> {
    pub bytes: &'a [u8],
    pub len: Option<usize>,
}

impl StorageCompletion<'_> {
    pub const fn empty() -> Self {
        Self {
            bytes: &[],
            len: None,
        }
    }

    pub fn bytes(bytes: &[u8]) -> Result<StorageCompletion<'_>, VmError> {
        if bytes.len() > MAX_STORAGE_TRANSFER_BYTES {
            return Err(VmError::InvalidStateRecord);
        }
        Ok(StorageCompletion {
            bytes,
            len: Some(bytes.len()),
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VmDispatch {
    Complete,
    PendingStorage(StorageRequest),
}

pub trait TraceSink {
    fn trace(&mut self, message: &str);
    fn debug_print(&mut self, _strings: &StringResolver<'_>, _values: &[Value]) {}
    fn draw_clear(&mut self, _color: &str) {}
    fn draw_text(
        &mut self,
        _strings: &StringResolver<'_>,
        _text: Value,
        _options: DisplayTextOptions<'_>,
    ) {
    }
    fn draw_rect(&mut self, _options: DisplayRectOptions<'_>) {}
    fn draw_line(&mut self, _options: DisplayLineOptions<'_>) {}
    fn hardware_gpio_write(&mut self, _name: &str, _value: bool) -> Result<(), VmError> {
        Err(VmError::InvalidOperand)
    }
    fn hardware_gpio_toggle(&mut self, _name: &str) -> Result<(), VmError> {
        Err(VmError::InvalidOperand)
    }
    fn hardware_gpio_read(&mut self, _name: &str) -> Result<bool, VmError> {
        Err(VmError::InvalidOperand)
    }
    fn service_indicator_write(&mut self, _value: bool) -> Result<(), VmError> {
        Err(VmError::InvalidOperand)
    }
    fn service_indicator_toggle(&mut self) -> Result<(), VmError> {
        Err(VmError::InvalidOperand)
    }
    fn service_indicator_breathe(&mut self) -> Result<(), VmError> {
        Err(VmError::InvalidOperand)
    }
    fn service_indicator_read(&mut self) -> Result<bool, VmError> {
        Err(VmError::InvalidOperand)
    }
    fn app_launch(&mut self, _app: &str) -> Result<(), VmError> {
        Err(VmError::InvalidOperand)
    }
    fn app_arm(&mut self, _app: &str) -> Result<(), VmError> {
        Err(VmError::InvalidOperand)
    }
    fn app_disarm(&mut self, _app: &str) -> Result<(), VmError> {
        Err(VmError::InvalidOperand)
    }
    fn app_registry_list<'a>(&'a mut self) -> Result<AppRegistryList<'a>, VmError> {
        Err(VmError::InvalidOperand)
    }
    fn app_registry_get<'a>(&'a mut self, _app_id: &str) -> Result<AppRegistryEntry<'a>, VmError> {
        Err(VmError::InvalidOperand)
    }
    fn app_process_stack<'a>(&'a mut self) -> Result<AppProcessStack<'a>, VmError> {
        Err(VmError::InvalidOperand)
    }
    fn app_armed_stack<'a>(&'a mut self) -> Result<AppArmedStack<'a>, VmError> {
        Err(VmError::InvalidOperand)
    }
    fn service_timer_every(&mut self, _event: &str, _interval_ms: i32) -> Result<(), VmError> {
        Err(VmError::InvalidOperand)
    }
    fn service_timer_after(&mut self, _event: &str, _delay_ms: i32) -> Result<(), VmError> {
        Err(VmError::InvalidOperand)
    }
    fn service_wifi_start_ap<'a>(
        &'a mut self,
        _ssid: &str,
    ) -> Result<WifiActionResult<'a>, VmError> {
        Err(VmError::InvalidOperand)
    }
    fn service_wifi_stop_ap<'a>(&'a mut self) -> Result<WifiActionResult<'a>, VmError> {
        Err(VmError::InvalidOperand)
    }
    fn service_wifi_connect<'a>(
        &'a mut self,
        _profile: &str,
    ) -> Result<WifiActionResult<'a>, VmError> {
        Err(VmError::InvalidOperand)
    }
    fn service_wifi_disconnect<'a>(&'a mut self) -> Result<WifiActionResult<'a>, VmError> {
        Err(VmError::InvalidOperand)
    }
    fn service_wifi_status<'a>(&'a mut self) -> Result<WifiStatus<'a>, VmError> {
        Err(VmError::InvalidOperand)
    }
    fn service_wifi_get_ap_ip<'a>(&'a mut self) -> Result<WifiApIp<'a>, VmError> {
        Err(VmError::InvalidOperand)
    }
    fn service_wifi_scan<'a>(&'a mut self) -> Result<WifiScanResult<'a>, VmError> {
        Ok(WifiScanResult {
            ok: false,
            error: Some("unsupported"),
            networks: &[],
        })
    }
    fn service_wifi_teardown(&mut self) -> Result<(), VmError> {
        Ok(())
    }
    fn system_memory_text(&mut self, _out: &mut dyn fmt::Write) -> Result<(), VmError> {
        Err(VmError::InvalidOperand)
    }
    fn system_storage_text(
        &mut self,
        _name: &str,
        _out: &mut dyn fmt::Write,
    ) -> Result<(), VmError> {
        Err(VmError::InvalidOperand)
    }
    fn state_load(&mut self, _out: &mut [u8]) -> Result<Option<usize>, VmError> {
        Ok(None)
    }
    fn state_save(&mut self, _bytes: &[u8]) -> Result<(), VmError> {
        Ok(())
    }
    fn state_reset_persistent(&mut self) -> Result<(), VmError> {
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AppRegistryEntry<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub build: &'a str,
    pub description: &'a str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AppRegistryList<'a> {
    pub apps: &'a [AppRegistryEntry<'a>],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AppProcessStack<'a> {
    pub apps: &'a [&'a str],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AppArmedStackEntry<'a> {
    pub app_id: &'a str,
    pub event: &'a str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AppArmedStack<'a> {
    pub entries: &'a [AppArmedStackEntry<'a>],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DisplayTextOptions<'a> {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub font_height: i32,
    pub text_color: Option<&'a str>,
    pub background_color: Option<&'a str>,
    pub align: Option<&'a str>,
    pub valign: Option<&'a str>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DisplayRectOptions<'a> {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub fill_color: Option<&'a str>,
    pub stroke_color: Option<&'a str>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DisplayLineOptions<'a> {
    pub x1: i32,
    pub y1: i32,
    pub x2: i32,
    pub y2: i32,
    pub color: Option<&'a str>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WifiActionResult<'a> {
    pub ok: bool,
    pub error: Option<&'a str>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WifiStatus<'a> {
    pub active: bool,
    pub mode: Option<&'a str>,
    pub ip_address: Option<&'a str>,
    pub ssid: Option<&'a str>,
    pub clients: i32,
    pub error: Option<&'a str>,
    pub state: &'a str,
    pub backend: &'a str,
    pub driver_started: bool,
    pub configured: bool,
    pub driver_mode: Option<&'a str>,
    pub channel: i32,
    pub ap_start_events: i32,
    pub ap_stop_events: i32,
    pub probe_events: i32,
    pub sta_connected_events: i32,
    pub sta_disconnected_events: i32,
    pub last_backend_code: Option<&'a str>,
    pub profile: Option<&'a str>,
    pub connected: bool,
    pub scan_matches: i32,
    pub rssi: i32,
    pub auth: Option<&'a str>,
    pub bssid: Option<&'a str>,
    pub disconnect_reason: Option<&'a str>,
    pub disconnect_reason_code: i32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WifiApIp<'a> {
    pub ip: Option<&'a str>,
    pub gw: Option<&'a str>,
    pub netmask: Option<&'a str>,
    pub error: Option<&'a str>,
}

pub const WIFI_SCAN_SSID_CAP: usize = 32;
pub const WIFI_SCAN_BSSID_TEXT_LEN: usize = 17;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WifiAccessPoint {
    ssid: [u8; WIFI_SCAN_SSID_CAP],
    ssid_len: usize,
    bssid: [u8; WIFI_SCAN_BSSID_TEXT_LEN],
    has_bssid: bool,
    pub ssid_length: i32,
    pub channel: i32,
    pub rssi: i32,
    pub auth: Option<&'static str>,
    pub hidden: bool,
}

impl WifiAccessPoint {
    pub const fn empty() -> Self {
        Self {
            ssid: [0; WIFI_SCAN_SSID_CAP],
            ssid_len: 0,
            bssid: [0; WIFI_SCAN_BSSID_TEXT_LEN],
            has_bssid: false,
            ssid_length: 0,
            channel: 0,
            rssi: 0,
            auth: None,
            hidden: false,
        }
    }

    pub const fn from_fixed_parts(
        ssid: [u8; WIFI_SCAN_SSID_CAP],
        ssid_len: usize,
        bssid: [u8; WIFI_SCAN_BSSID_TEXT_LEN],
        has_bssid: bool,
        ssid_length: i32,
        channel: i32,
        rssi: i32,
        auth: Option<&'static str>,
        hidden: bool,
    ) -> Self {
        Self {
            ssid,
            ssid_len,
            bssid,
            has_bssid,
            ssid_length,
            channel,
            rssi,
            auth,
            hidden,
        }
    }

    pub fn new(
        ssid: &[u8],
        bssid: Option<[u8; 6]>,
        channel: i32,
        rssi: i32,
        auth: Option<&'static str>,
        hidden: bool,
    ) -> Result<Self, VmError> {
        if ssid.len() > WIFI_SCAN_SSID_CAP {
            return Err(VmError::InvalidOperand);
        }
        let mut ap = Self::empty();
        ap.ssid[..ssid.len()].copy_from_slice(ssid);
        ap.ssid_len = ssid.len();
        ap.ssid_length = ssid.len().min(i32::MAX as usize) as i32;
        if let Some(bssid) = bssid {
            ap.write_bssid(bssid);
            ap.has_bssid = true;
        }
        ap.channel = channel;
        ap.rssi = rssi;
        ap.auth = auth;
        ap.hidden = hidden;
        Ok(ap)
    }

    pub fn ssid(&self) -> Result<&str, VmError> {
        core::str::from_utf8(&self.ssid[..self.ssid_len]).map_err(|_| VmError::InvalidUtf8)
    }

    pub fn bssid(&self) -> Result<Option<&str>, VmError> {
        if self.has_bssid {
            Ok(Some(
                core::str::from_utf8(&self.bssid).map_err(|_| VmError::InvalidUtf8)?,
            ))
        } else {
            Ok(None)
        }
    }

    fn write_bssid(&mut self, bssid: [u8; 6]) {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut write = 0usize;
        let mut read = 0usize;
        while read < bssid.len() {
            if read > 0 {
                self.bssid[write] = b':';
                write += 1;
            }
            let byte = bssid[read];
            self.bssid[write] = HEX[(byte >> 4) as usize];
            self.bssid[write + 1] = HEX[(byte & 0x0f) as usize];
            write += 2;
            read += 1;
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WifiScanResult<'a> {
    pub ok: bool,
    pub error: Option<&'a str>,
    pub networks: &'a [WifiAccessPoint],
}

const SIM_WIFI_SSID_CAP: usize = 32;
const SIM_WIFI_AP_IP: &str = "192.168.4.1";
const SIM_WIFI_AP_NETMASK: &str = "255.255.255.0";

pub trait WifiBackend {
    fn start_ap(&mut self, ssid: &str) -> Result<WifiActionResult<'static>, VmError>;
    fn stop_ap(&mut self) -> Result<WifiActionResult<'static>, VmError>;
    fn connect(&mut self, profile: &str) -> Result<WifiActionResult<'static>, VmError>;
    fn disconnect(&mut self) -> Result<WifiActionResult<'static>, VmError>;
    fn status<'a>(&'a mut self) -> Result<WifiStatus<'a>, VmError>;
    fn ap_ip<'a>(&'a mut self) -> Result<WifiApIp<'a>, VmError>;
    fn scan<'a>(&'a mut self) -> Result<WifiScanResult<'a>, VmError>;
    fn teardown(&mut self) -> Result<bool, VmError>;
}

pub struct SimWifiBackend {
    active: bool,
    ssid: [u8; SIM_WIFI_SSID_CAP],
    ssid_len: usize,
    clients: i32,
    profile: [u8; SIM_WIFI_SSID_CAP],
    profile_len: usize,
    connected: bool,
}

impl SimWifiBackend {
    pub const fn new() -> Self {
        Self {
            active: false,
            ssid: [0; SIM_WIFI_SSID_CAP],
            ssid_len: 0,
            clients: 0,
            profile: [0; SIM_WIFI_SSID_CAP],
            profile_len: 0,
            connected: false,
        }
    }

    pub fn set_clients(&mut self, clients: i32) {
        self.clients = if self.active { clients.max(0) } else { 0 };
    }

    fn ssid(&self) -> Result<&str, VmError> {
        core::str::from_utf8(&self.ssid[..self.ssid_len]).map_err(|_| VmError::InvalidUtf8)
    }

    fn profile(&self) -> Result<&str, VmError> {
        core::str::from_utf8(&self.profile[..self.profile_len]).map_err(|_| VmError::InvalidUtf8)
    }
}

impl Default for SimWifiBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl WifiBackend for SimWifiBackend {
    fn start_ap(&mut self, ssid: &str) -> Result<WifiActionResult<'static>, VmError> {
        let bytes = ssid.as_bytes();
        if bytes.is_empty() || bytes.len() > self.ssid.len() {
            return Ok(WifiActionResult {
                ok: false,
                error: Some("invalid ssid"),
            });
        }
        self.ssid[..bytes.len()].copy_from_slice(bytes);
        self.ssid_len = bytes.len();
        self.active = true;
        Ok(WifiActionResult {
            ok: true,
            error: None,
        })
    }

    fn stop_ap(&mut self) -> Result<WifiActionResult<'static>, VmError> {
        self.active = false;
        self.ssid_len = 0;
        self.clients = 0;
        Ok(WifiActionResult {
            ok: true,
            error: None,
        })
    }

    fn connect(&mut self, profile: &str) -> Result<WifiActionResult<'static>, VmError> {
        let bytes = profile.as_bytes();
        if bytes.is_empty() || bytes.len() > self.profile.len() {
            return Ok(WifiActionResult {
                ok: false,
                error: Some("invalid profile"),
            });
        }
        self.profile[..bytes.len()].copy_from_slice(bytes);
        self.profile_len = bytes.len();
        self.connected = true;
        Ok(WifiActionResult {
            ok: true,
            error: None,
        })
    }

    fn disconnect(&mut self) -> Result<WifiActionResult<'static>, VmError> {
        self.connected = false;
        self.profile_len = 0;
        Ok(WifiActionResult {
            ok: true,
            error: None,
        })
    }

    fn status<'a>(&'a mut self) -> Result<WifiStatus<'a>, VmError> {
        Ok(WifiStatus {
            active: self.active,
            mode: if self.active { Some("ap") } else { None },
            ip_address: if self.active {
                Some(SIM_WIFI_AP_IP)
            } else {
                None
            },
            ssid: if self.active {
                Some(self.ssid()?)
            } else {
                None
            },
            clients: self.clients,
            error: None,
            state: if self.active || self.connected {
                "started"
            } else {
                "stopped"
            },
            backend: "sim",
            driver_started: self.active || self.connected,
            configured: self.active || self.connected,
            driver_mode: if self.active {
                Some("ap")
            } else if self.connected {
                Some("sta")
            } else {
                None
            },
            channel: if self.active { 1 } else { 0 },
            ap_start_events: if self.active { 1 } else { 0 },
            ap_stop_events: 0,
            probe_events: 0,
            sta_connected_events: 0,
            sta_disconnected_events: 0,
            last_backend_code: None,
            profile: if self.connected {
                Some(self.profile()?)
            } else {
                None
            },
            connected: self.connected,
            scan_matches: if self.connected { 1 } else { 0 },
            rssi: if self.connected { -42 } else { 0 },
            auth: if self.connected { Some("sim") } else { None },
            bssid: if self.connected {
                Some("00:00:00:00:00:00")
            } else {
                None
            },
            disconnect_reason: None,
            disconnect_reason_code: 0,
        })
    }

    fn ap_ip<'a>(&'a mut self) -> Result<WifiApIp<'a>, VmError> {
        Ok(WifiApIp {
            ip: if self.active {
                Some(SIM_WIFI_AP_IP)
            } else {
                None
            },
            gw: if self.active {
                Some(SIM_WIFI_AP_IP)
            } else {
                None
            },
            netmask: if self.active {
                Some(SIM_WIFI_AP_NETMASK)
            } else {
                None
            },
            error: None,
        })
    }

    fn scan<'a>(&'a mut self) -> Result<WifiScanResult<'a>, VmError> {
        if self.active || self.connected {
            Ok(WifiScanResult {
                ok: false,
                error: Some("wifi busy"),
                networks: &[],
            })
        } else {
            Ok(WifiScanResult {
                ok: false,
                error: Some("unsupported"),
                networks: &[],
            })
        }
    }

    fn teardown(&mut self) -> Result<bool, VmError> {
        let was_active = self.active || self.connected;
        self.active = false;
        self.ssid_len = 0;
        self.clients = 0;
        self.connected = false;
        self.profile_len = 0;
        Ok(was_active)
    }
}

#[cfg(test)]
mod wifi_backend_tests {
    use super::{SimWifiBackend, WifiAccessPoint, WifiBackend};

    #[test]
    fn wifi_access_point_formats_bssid_and_tracks_hidden_ssid_length() {
        let ap = WifiAccessPoint::new(
            b"",
            Some([0x00, 0x11, 0x22, 0xaa, 0xbb, 0xcc]),
            11,
            -80,
            Some("wpa2"),
            true,
        )
        .unwrap();

        assert_eq!(ap.ssid().unwrap(), "");
        assert_eq!(ap.ssid_length, 0);
        assert_eq!(ap.bssid().unwrap(), Some("00:11:22:aa:bb:cc"));
        assert_eq!(ap.channel, 11);
        assert_eq!(ap.rssi, -80);
        assert_eq!(ap.auth, Some("wpa2"));
        assert!(ap.hidden);
    }

    #[test]
    fn wifi_access_point_rejects_oversized_ssid() {
        let oversized = b"123456789012345678901234567890123";

        assert!(WifiAccessPoint::new(oversized, None, 1, -42, None, false).is_err());
    }

    #[test]
    fn sim_backend_tracks_ap_status_ip_and_teardown() {
        let mut backend = SimWifiBackend::new();

        let started = backend.start_ap("SquidScript").unwrap();
        assert!(started.ok);
        assert_eq!(started.error, None);

        let status = backend.status().unwrap();
        assert!(status.active);
        assert_eq!(status.mode, Some("ap"));
        assert_eq!(status.ip_address, Some("192.168.4.1"));
        assert_eq!(status.ssid, Some("SquidScript"));
        assert_eq!(status.clients, 0);

        let ip = backend.ap_ip().unwrap();
        assert_eq!(ip.ip, Some("192.168.4.1"));
        assert_eq!(ip.gw, Some("192.168.4.1"));
        assert_eq!(ip.netmask, Some("255.255.255.0"));

        backend.teardown().unwrap();
        let status = backend.status().unwrap();
        assert!(!status.active);
        assert_eq!(status.ssid, None);
    }

    #[test]
    fn sim_backend_reports_client_count_only_while_ap_is_active() {
        let mut backend = SimWifiBackend::new();

        backend.set_clients(2);
        assert_eq!(backend.status().unwrap().clients, 0);

        backend.start_ap("SquidScript").unwrap();
        backend.set_clients(2);
        assert_eq!(backend.status().unwrap().clients, 2);

        backend.stop_ap().unwrap();
        assert_eq!(backend.status().unwrap().clients, 0);
    }

    #[test]
    fn sim_backend_rejects_invalid_ssids_without_starting_ap() {
        let mut backend = SimWifiBackend::new();

        let result = backend.start_ap("").unwrap();
        assert!(!result.ok);
        assert_eq!(result.error, Some("invalid ssid"));
        assert!(!backend.status().unwrap().active);

        let too_long = "123456789012345678901234567890123";
        let result = backend.start_ap(too_long).unwrap();
        assert!(!result.ok);
        assert_eq!(result.error, Some("invalid ssid"));
        assert!(!backend.status().unwrap().active);
    }

    #[test]
    fn sim_backend_reports_scan_busy_while_ap_or_station_is_active() {
        let mut backend = SimWifiBackend::new();

        backend.start_ap("SquidScript").unwrap();
        let scan = backend.scan().unwrap();
        assert!(!scan.ok);
        assert_eq!(scan.error, Some("wifi busy"));
        assert!(scan.networks.is_empty());

        backend.stop_ap().unwrap();
        backend.connect("dev").unwrap();
        let scan = backend.scan().unwrap();
        assert!(!scan.ok);
        assert_eq!(scan.error, Some("wifi busy"));
        assert!(scan.networks.is_empty());
    }
}
