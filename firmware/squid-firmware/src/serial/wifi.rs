extern crate alloc;

use core::{cell::Cell, fmt, mem::MaybeUninit, time::Duration};

use alloc::string::String;

use critical_section::Mutex;
use esp_hal::peripherals::WIFI;
use esp_radio::{
    wifi::{
        event::{self, EventExt},
        AccessPointConfig, AuthMethod, ClientConfig, Config, ModeConfig, ScanConfig,
        WifiController, WifiDevice,
    },
    Controller,
};
use esp_wifi_sys::include::{
    esp_wifi_ap_get_sta_list, esp_wifi_get_config, esp_wifi_get_country, esp_wifi_get_max_tx_power,
    esp_wifi_get_mode, wifi_config_t, wifi_country_t, wifi_interface_t_WIFI_IF_AP,
    wifi_mode_t_WIFI_MODE_AP, wifi_mode_t_WIFI_MODE_APSTA, wifi_mode_t_WIFI_MODE_NULL,
    wifi_mode_t_WIFI_MODE_STA, wifi_sta_list_t, ESP_OK,
};
use squidvm_core::{
    error::VmError,
    host::{
        SimWifiBackend, WifiAccessPoint, WifiActionResult, WifiApIp, WifiBackend, WifiScanResult,
        WifiStatus,
    },
};

const AP_IP: &str = "192.168.4.1";
const AP_NETMASK: &str = "255.255.255.0";
const WIFI_SCAN_RESULT_CAP: usize = 8;

static AP_START_EVENTS: Mutex<Cell<u32>> = Mutex::new(Cell::new(0));
static AP_STOP_EVENTS: Mutex<Cell<u32>> = Mutex::new(Cell::new(0));
static AP_PROBE_EVENTS: Mutex<Cell<u32>> = Mutex::new(Cell::new(0));
static AP_STA_CONNECTED_EVENTS: Mutex<Cell<u32>> = Mutex::new(Cell::new(0));
static AP_STA_DISCONNECTED_EVENTS: Mutex<Cell<u32>> = Mutex::new(Cell::new(0));
static STA_START_EVENTS: Mutex<Cell<u32>> = Mutex::new(Cell::new(0));
static STA_STOP_EVENTS: Mutex<Cell<u32>> = Mutex::new(Cell::new(0));
static STA_CONNECTED_EVENTS: Mutex<Cell<u32>> = Mutex::new(Cell::new(0));
static STA_DISCONNECTED_EVENTS: Mutex<Cell<u32>> = Mutex::new(Cell::new(0));
static STA_AUTHMODE_CHANGE_EVENTS: Mutex<Cell<u32>> = Mutex::new(Cell::new(0));
static STA_LAST_DISCONNECT_REASON: Mutex<Cell<u32>> = Mutex::new(Cell::new(0));
static STA_LAST_DISCONNECT_RSSI: Mutex<Cell<i32>> = Mutex::new(Cell::new(0));
static STA_LAST_AUTHMODE: Mutex<Cell<u32>> = Mutex::new(Cell::new(0));

pub fn install_wifi_event_diagnostics() {
    event::ApStart::update_handler(|_| {
        increment_counter(&AP_START_EVENTS);
    });
    event::ApStop::update_handler(|_| {
        increment_counter(&AP_STOP_EVENTS);
    });
    event::ApProbeReqReceived::update_handler(|_| {
        increment_counter(&AP_PROBE_EVENTS);
    });
    event::ApStaConnected::update_handler(|_| {
        increment_counter(&AP_STA_CONNECTED_EVENTS);
    });
    event::ApStaDisconnected::update_handler(|_| {
        increment_counter(&AP_STA_DISCONNECTED_EVENTS);
    });
    event::StaStart::update_handler(|_| {
        increment_counter(&STA_START_EVENTS);
    });
    event::StaStop::update_handler(|_| {
        increment_counter(&STA_STOP_EVENTS);
    });
    event::StaConnected::update_handler(|event| {
        increment_counter(&STA_CONNECTED_EVENTS);
        write_cell_u32(&STA_LAST_AUTHMODE, event.authmode());
    });
    event::StaDisconnected::update_handler(|event| {
        increment_counter(&STA_DISCONNECTED_EVENTS);
        write_cell_u32(&STA_LAST_DISCONNECT_REASON, u32::from(event.reason()));
        write_cell_i32(&STA_LAST_DISCONNECT_RSSI, i32::from(event.rssi()));
    });
    event::StaAuthmodeChange::update_handler(|event| {
        increment_counter(&STA_AUTHMODE_CHANGE_EVENTS);
        write_cell_u32(&STA_LAST_AUTHMODE, event.new_mode());
    });
}

fn increment_counter(counter: &'static Mutex<Cell<u32>>) {
    critical_section::with(|cs| {
        let current = counter.borrow(cs).get();
        counter.borrow(cs).set(current.saturating_add(1));
    });
}

fn read_counter(counter: &'static Mutex<Cell<u32>>) -> u32 {
    critical_section::with(|cs| counter.borrow(cs).get())
}

fn read_counter_i32(counter: &'static Mutex<Cell<u32>>) -> i32 {
    read_counter(counter).min(i32::MAX as u32) as i32
}

fn write_cell_u32(cell: &'static Mutex<Cell<u32>>, value: u32) {
    critical_section::with(|cs| cell.borrow(cs).set(value));
}

fn read_cell_u32(cell: &'static Mutex<Cell<u32>>) -> u32 {
    critical_section::with(|cs| cell.borrow(cs).get())
}

fn write_cell_i32(cell: &'static Mutex<Cell<i32>>, value: i32) {
    critical_section::with(|cs| cell.borrow(cs).set(value));
}

fn read_cell_i32(cell: &'static Mutex<Cell<i32>>) -> i32 {
    critical_section::with(|cs| cell.borrow(cs).get())
}

pub enum FirmwareWifiBackend<'d> {
    Sim(SimWifiBackend),
    Esp(EspWifiBackend<'d>),
    Unavailable,
}

impl<'d> FirmwareWifiBackend<'d> {
    pub fn new_esp(radio: &'d Controller<'d>, wifi: WIFI<'d>) -> Self {
        match esp_radio::wifi::new(radio, wifi, Config::default()) {
            Ok((controller, interfaces)) => Self::Esp(EspWifiBackend::new(
                controller,
                interfaces.ap,
                interfaces.sta,
            )),
            Err(_) => Self::Unavailable,
        }
    }

    pub fn write_driver_diagnostics(&mut self, out: &mut dyn fmt::Write) {
        match self {
            Self::Sim(_) => {
                writeln!(out, "backend=sim").ok();
            }
            Self::Esp(backend) => backend.write_driver_diagnostics(out),
            Self::Unavailable => {
                writeln!(out, "backend=unavailable").ok();
            }
        }
    }

    pub fn poll(&mut self) {
        match self {
            Self::Sim(_) | Self::Unavailable => {}
            Self::Esp(backend) => backend.poll(),
        }
    }

    pub fn connect_profile(
        &mut self,
        profile: &str,
        ssid: &[u8],
        password: &[u8],
    ) -> Result<WifiActionResult<'static>, VmError> {
        match self {
            Self::Sim(backend) => backend.connect(profile),
            Self::Esp(backend) => backend.connect_profile(profile, ssid, password),
            Self::Unavailable => Ok(error_result("radio unavailable")),
        }
    }
}

impl Default for FirmwareWifiBackend<'_> {
    fn default() -> Self {
        Self::Sim(SimWifiBackend::new())
    }
}

impl WifiBackend for FirmwareWifiBackend<'_> {
    fn start_ap(&mut self, ssid: &str) -> Result<WifiActionResult<'static>, VmError> {
        match self {
            Self::Sim(backend) => backend.start_ap(ssid),
            Self::Esp(backend) => backend.start_ap(ssid),
            Self::Unavailable => Ok(error_result("radio unavailable")),
        }
    }

    fn stop_ap(&mut self) -> Result<WifiActionResult<'static>, VmError> {
        match self {
            Self::Sim(backend) => backend.stop_ap(),
            Self::Esp(backend) => backend.stop_ap(),
            Self::Unavailable => Ok(WifiActionResult {
                ok: true,
                error: None,
            }),
        }
    }

    fn connect(&mut self, profile: &str) -> Result<WifiActionResult<'static>, VmError> {
        match self {
            Self::Sim(backend) => backend.connect(profile),
            Self::Esp(backend) => backend.connect(profile),
            Self::Unavailable => Ok(error_result("radio unavailable")),
        }
    }

    fn disconnect(&mut self) -> Result<WifiActionResult<'static>, VmError> {
        match self {
            Self::Sim(backend) => backend.disconnect(),
            Self::Esp(backend) => backend.disconnect(),
            Self::Unavailable => Ok(WifiActionResult {
                ok: true,
                error: None,
            }),
        }
    }

    fn status<'a>(&'a mut self) -> Result<WifiStatus<'a>, VmError> {
        match self {
            Self::Sim(backend) => backend.status(),
            Self::Esp(backend) => backend.status(),
            Self::Unavailable => Ok(WifiStatus {
                active: false,
                mode: None,
                ip_address: None,
                ssid: None,
                clients: 0,
                error: Some("radio unavailable"),
                state: "unavailable",
                backend: "unavailable",
                driver_started: false,
                configured: false,
                driver_mode: None,
                channel: 0,
                ap_start_events: read_counter_i32(&AP_START_EVENTS),
                ap_stop_events: read_counter_i32(&AP_STOP_EVENTS),
                probe_events: read_counter_i32(&AP_PROBE_EVENTS),
                sta_connected_events: read_counter_i32(&AP_STA_CONNECTED_EVENTS),
                sta_disconnected_events: read_counter_i32(&AP_STA_DISCONNECTED_EVENTS),
                last_backend_code: Some("radio unavailable"),
                profile: None,
                connected: false,
                scan_matches: 0,
                rssi: 0,
                auth: None,
                bssid: None,
                disconnect_reason: None,
                disconnect_reason_code: 0,
            }),
        }
    }

    fn ap_ip<'a>(&'a mut self) -> Result<WifiApIp<'a>, VmError> {
        match self {
            Self::Sim(backend) => backend.ap_ip(),
            Self::Esp(backend) => backend.ap_ip(),
            Self::Unavailable => Ok(WifiApIp {
                ip: None,
                gw: None,
                netmask: None,
                error: Some("radio unavailable"),
            }),
        }
    }

    fn scan<'a>(&'a mut self) -> Result<WifiScanResult<'a>, VmError> {
        match self {
            Self::Sim(backend) => backend.scan(),
            Self::Esp(backend) => backend.scan(),
            Self::Unavailable => Ok(WifiScanResult {
                ok: false,
                error: Some("unsupported"),
                networks: &[],
            }),
        }
    }

    fn teardown(&mut self) -> Result<bool, VmError> {
        match self {
            Self::Sim(backend) => backend.teardown(),
            Self::Esp(backend) => backend.teardown(),
            Self::Unavailable => Ok(false),
        }
    }
}

pub struct EspWifiBackend<'d> {
    controller: WifiController<'d>,
    ap_device: WifiDevice<'d>,
    sta_device: WifiDevice<'d>,
    active: bool,
    ssid: [u8; 32],
    ssid_len: usize,
    clients: i32,
    configured: bool,
    last_backend_code: Option<&'static str>,
    station_profile: [u8; 32],
    station_profile_len: usize,
    station_connected: bool,
    station_scan_matches: i32,
    station_rssi: i32,
    station_auth: Option<&'static str>,
    station_bssid: [u8; 17],
    station_has_bssid: bool,
    station_channel: i32,
    station_disconnect_reason: Option<&'static str>,
    station_disconnect_reason_code: i32,
    scan_networks: [WifiAccessPoint; WIFI_SCAN_RESULT_CAP],
    scan_len: usize,
}

impl<'d> EspWifiBackend<'d> {
    fn new(
        controller: WifiController<'d>,
        ap_device: WifiDevice<'d>,
        sta_device: WifiDevice<'d>,
    ) -> Self {
        Self {
            controller,
            ap_device,
            sta_device,
            active: false,
            ssid: [0; 32],
            ssid_len: 0,
            clients: 0,
            configured: false,
            last_backend_code: None,
            station_profile: [0; 32],
            station_profile_len: 0,
            station_connected: false,
            station_scan_matches: 0,
            station_rssi: 0,
            station_auth: None,
            station_bssid: [0; 17],
            station_has_bssid: false,
            station_channel: 0,
            station_disconnect_reason: None,
            station_disconnect_reason_code: 0,
            scan_networks: [WifiAccessPoint::empty(); WIFI_SCAN_RESULT_CAP],
            scan_len: 0,
        }
    }

    fn ssid(&self) -> Result<&str, VmError> {
        core::str::from_utf8(&self.ssid[..self.ssid_len]).map_err(|_| VmError::InvalidUtf8)
    }

    fn station_profile(&self) -> Result<&str, VmError> {
        core::str::from_utf8(&self.station_profile[..self.station_profile_len])
            .map_err(|_| VmError::InvalidUtf8)
    }

    fn clear_active_state(&mut self) {
        self.active = false;
        self.ssid_len = 0;
        self.clients = 0;
        self.configured = false;
    }

    fn clear_station_state(&mut self) {
        self.station_profile_len = 0;
        self.station_connected = false;
        self.station_scan_matches = 0;
        self.station_rssi = 0;
        self.station_auth = None;
        self.station_bssid = [0; 17];
        self.station_has_bssid = false;
        self.station_channel = 0;
        self.station_disconnect_reason = None;
        self.station_disconnect_reason_code = 0;
    }

    fn clear_scan_results(&mut self) {
        self.scan_len = 0;
        self.scan_networks = [WifiAccessPoint::empty(); WIFI_SCAN_RESULT_CAP];
    }

    fn client_count(&mut self) -> i32 {
        if !self.active {
            self.clients = 0;
            return 0;
        }

        let mut list = MaybeUninit::<wifi_sta_list_t>::zeroed();
        let result = unsafe { esp_wifi_ap_get_sta_list(list.as_mut_ptr()) };
        if result == ESP_OK as _ {
            let list = unsafe { list.assume_init() };
            self.clients = list.num.max(0);
        }
        self.clients
    }

    fn write_driver_diagnostics(&mut self, out: &mut dyn fmt::Write) {
        writeln!(out, "backend=esp").ok();
        let mac = self.ap_device.mac_address();
        writeln!(
            out,
            "driver_ap_mac={:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
            mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
        )
        .ok();
        match self.controller.is_started() {
            Ok(started) => {
                writeln!(out, "driver_started={started}").ok();
            }
            Err(error) => {
                writeln!(out, "driver_started_error={error:?}").ok();
            }
        }

        let mut mode = wifi_mode_t_WIFI_MODE_NULL;
        let mode_result = unsafe { esp_wifi_get_mode(&mut mode) };
        if mode_result == ESP_OK as _ {
            writeln!(out, "driver_mode={}", mode_name(mode)).ok();
            writeln!(out, "driver_mode_raw={mode}").ok();
        } else {
            writeln!(out, "driver_mode_error={mode_result}").ok();
        }

        let mut tx_power: i8 = 0;
        let tx_power_result = unsafe { esp_wifi_get_max_tx_power(&mut tx_power) };
        if tx_power_result == ESP_OK as _ {
            writeln!(out, "driver_tx_power_quarter_dbm={tx_power}").ok();
        } else {
            writeln!(out, "driver_tx_power_error={tx_power_result}").ok();
        }

        let mut country = MaybeUninit::<wifi_country_t>::zeroed();
        let country_result = unsafe { esp_wifi_get_country(country.as_mut_ptr()) };
        if country_result == ESP_OK as _ {
            let country = unsafe { country.assume_init() };
            let cc = core::str::from_utf8(&country.cc[..2]).unwrap_or("");
            writeln!(out, "driver_country={cc}").ok();
            writeln!(out, "driver_country_op_class={}", country.cc[2]).ok();
            writeln!(out, "driver_country_schan={}", country.schan).ok();
            writeln!(out, "driver_country_nchan={}", country.nchan).ok();
            writeln!(out, "driver_country_max_tx_power={}", country.max_tx_power).ok();
            writeln!(out, "driver_country_policy={}", country.policy).ok();
        } else {
            writeln!(out, "driver_country_error={country_result}").ok();
        }

        let mut config = MaybeUninit::<wifi_config_t>::zeroed();
        let config_result =
            unsafe { esp_wifi_get_config(wifi_interface_t_WIFI_IF_AP, config.as_mut_ptr()) };
        if config_result != ESP_OK as _ {
            writeln!(out, "driver_ap_config_error={config_result}").ok();
            return;
        }

        let config = unsafe { config.assume_init() };
        let ap = unsafe { config.ap };
        let ssid_len = usize::from(ap.ssid_len).min(ap.ssid.len());
        let ssid = core::str::from_utf8(&ap.ssid[..ssid_len]).unwrap_or("<invalid-utf8>");
        writeln!(out, "driver_ap_ssid={ssid}").ok();
        writeln!(out, "driver_ap_ssid_len={ssid_len}").ok();
        writeln!(out, "driver_ap_channel={}", ap.channel).ok();
        writeln!(out, "driver_ap_hidden={}", ap.ssid_hidden).ok();
        writeln!(out, "driver_ap_auth={}", ap.authmode).ok();
        writeln!(out, "driver_ap_max_connections={}", ap.max_connection).ok();
        writeln!(out, "driver_ap_beacon_interval={}", ap.beacon_interval).ok();
        writeln!(out, "event_ap_start={}", read_counter(&AP_START_EVENTS)).ok();
        writeln!(out, "event_ap_stop={}", read_counter(&AP_STOP_EVENTS)).ok();
        writeln!(out, "event_ap_probe={}", read_counter(&AP_PROBE_EVENTS)).ok();
        writeln!(
            out,
            "event_ap_sta_connected={}",
            read_counter(&AP_STA_CONNECTED_EVENTS)
        )
        .ok();
        writeln!(
            out,
            "event_ap_sta_disconnected={}",
            read_counter(&AP_STA_DISCONNECTED_EVENTS)
        )
        .ok();
        writeln!(out, "event_sta_start={}", read_counter(&STA_START_EVENTS)).ok();
        writeln!(out, "event_sta_stop={}", read_counter(&STA_STOP_EVENTS)).ok();
        writeln!(
            out,
            "event_sta_connected={}",
            read_counter(&STA_CONNECTED_EVENTS)
        )
        .ok();
        writeln!(
            out,
            "event_sta_disconnected={}",
            read_counter(&STA_DISCONNECTED_EVENTS)
        )
        .ok();
        writeln!(
            out,
            "event_sta_authmode_change={}",
            read_counter(&STA_AUTHMODE_CHANGE_EVENTS)
        )
        .ok();
    }

    fn poll(&mut self) {
        while self.ap_device.receive().is_some() {}
        while self.sta_device.receive().is_some() {}
        self.poll_station_link();
    }

    fn poll_station_link(&mut self) {
        if self.station_profile_len == 0 {
            self.station_connected = false;
            return;
        }

        self.station_connected = matches!(self.controller.is_connected(), Ok(true));
        if self.station_connected {
            if let Ok(rssi) = self.controller.rssi() {
                self.station_rssi = rssi;
            }
            self.station_disconnect_reason = None;
            self.station_disconnect_reason_code = 0;
            self.last_backend_code = None;
            return;
        }

        let reason = read_cell_u32(&STA_LAST_DISCONNECT_REASON);
        if reason != 0 {
            self.station_disconnect_reason_code = reason.min(i32::MAX as u32) as i32;
            self.station_disconnect_reason = Some(disconnect_reason_name(reason));
            self.last_backend_code = self.station_disconnect_reason;
        }
        let event_rssi = read_cell_i32(&STA_LAST_DISCONNECT_RSSI);
        if event_rssi != 0 {
            self.station_rssi = event_rssi;
        }
    }

    fn connect_profile(
        &mut self,
        profile: &str,
        ssid: &[u8],
        password: &[u8],
    ) -> Result<WifiActionResult<'static>, VmError> {
        let profile_bytes = profile.as_bytes();
        if profile_bytes.is_empty() || profile_bytes.len() > self.station_profile.len() {
            return Ok(error_result("invalid profile"));
        }
        let ssid = match core::str::from_utf8(ssid) {
            Ok(ssid) if !ssid.is_empty() => ssid,
            _ => return Ok(error_result("invalid station ssid")),
        };
        let password = match core::str::from_utf8(password) {
            Ok(password) => password,
            Err(_) => return Ok(error_result("invalid station password")),
        };

        if self.active || self.station_profile_len > 0 {
            let _ = self.controller.disconnect();
            let _ = self.controller.stop();
            self.clear_active_state();
            self.clear_station_state();
        }

        let base_client = ClientConfig::default()
            .with_ssid(String::from(ssid))
            .with_password(String::from(password))
            .with_auth_method(AuthMethod::WpaWpa2Personal);
        let config = ModeConfig::Client(base_client);
        if self.controller.set_config(&config).is_err() {
            self.last_backend_code = Some("station config failed");
            return Ok(error_result("station config failed"));
        }
        self.configured = true;
        if self.controller.start().is_err() {
            self.last_backend_code = Some("station start failed");
            return Ok(error_result("station start failed"));
        }

        self.station_profile[..profile_bytes.len()].copy_from_slice(profile_bytes);
        self.station_profile_len = profile_bytes.len();
        self.station_connected = false;
        self.station_scan_matches = 0;
        self.station_rssi = 0;
        self.station_auth = None;
        self.station_has_bssid = false;
        self.station_channel = 0;
        self.station_disconnect_reason = None;
        self.station_disconnect_reason_code = 0;
        self.last_backend_code = Some("station connect pending");
        write_cell_u32(&STA_LAST_DISCONNECT_REASON, 0);
        write_cell_i32(&STA_LAST_DISCONNECT_RSSI, 0);
        write_cell_u32(&STA_LAST_AUTHMODE, 0);

        let mut best_auth = None;
        let mut best_bssid = None;
        let mut best_channel = None;
        match self.controller.scan_with_config(scan_config(Some(ssid), 4)) {
            Ok(records) => {
                self.station_scan_matches = records.len().min(i32::MAX as usize) as i32;
                if let Some(record) = records.iter().max_by_key(|record| record.signal_strength) {
                    self.station_rssi = i32::from(record.signal_strength);
                    best_auth = record.auth_method;
                    best_bssid = Some(record.bssid);
                    best_channel = Some(record.channel);
                    self.station_auth = best_auth.map(auth_method_name);
                    self.station_channel = i32::from(record.channel);
                    write_bssid(&mut self.station_bssid, record.bssid);
                    self.station_has_bssid = true;
                }
            }
            Err(_) => {
                self.last_backend_code = Some("station scan failed");
            }
        }

        if matches!(
            best_auth,
            Some(AuthMethod::Wpa2Enterprise | AuthMethod::WapiPersonal)
        ) {
            let _ = self.controller.stop();
            self.clear_station_state();
            self.last_backend_code = Some("unsupported auth");
            return Ok(error_result("unsupported auth"));
        }

        let mut client = ClientConfig::default()
            .with_ssid(String::from(ssid))
            .with_password(String::from(password))
            .with_auth_method(best_auth.unwrap_or(AuthMethod::WpaWpa2Personal));
        if let Some(bssid) = best_bssid {
            client = client.with_bssid(bssid);
        }
        if let Some(channel) = best_channel {
            client = client.with_channel(channel);
        }
        let config = ModeConfig::Client(client);
        if self.controller.set_config(&config).is_err() {
            self.last_backend_code = Some("station config failed");
            return Ok(error_result("station config failed"));
        }

        if self.controller.connect().is_err() {
            self.last_backend_code = Some("station connect failed");
            return Ok(error_result("station connect failed"));
        }
        self.poll_station_link();
        Ok(WifiActionResult {
            ok: true,
            error: None,
        })
    }
}

fn mode_name(mode: u32) -> &'static str {
    if mode == wifi_mode_t_WIFI_MODE_NULL {
        "null"
    } else if mode == wifi_mode_t_WIFI_MODE_STA {
        "sta"
    } else if mode == wifi_mode_t_WIFI_MODE_AP {
        "ap"
    } else if mode == wifi_mode_t_WIFI_MODE_APSTA {
        "apsta"
    } else {
        "unknown"
    }
}

fn auth_method_name(auth: AuthMethod) -> &'static str {
    match auth {
        AuthMethod::None => "open",
        AuthMethod::Wep => "wep",
        AuthMethod::Wpa => "wpa",
        AuthMethod::Wpa2Personal => "wpa2",
        AuthMethod::WpaWpa2Personal => "wpa/wpa2",
        AuthMethod::Wpa2Enterprise => "wpa2-enterprise",
        AuthMethod::Wpa3Personal => "wpa3",
        AuthMethod::Wpa2Wpa3Personal => "wpa2/wpa3",
        AuthMethod::WapiPersonal => "wapi",
        _ => "unknown",
    }
}

fn disconnect_reason_name(reason: u32) -> &'static str {
    match reason {
        2 => "auth expire",
        3 => "auth leave",
        4 => "assoc expire",
        5 => "assoc too many",
        6 => "not authed",
        7 => "not assoced",
        8 => "assoc leave",
        9 => "assoc not authed",
        15 => "4-way handshake timeout",
        201 => "no ap found",
        204 => "handshake timeout",
        205 => "connection fail",
        _ => "station disconnected",
    }
}

fn write_bssid(out: &mut [u8; 17], bssid: [u8; 6]) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut write = 0usize;
    let mut read = 0usize;
    while read < bssid.len() {
        if read > 0 {
            out[write] = b':';
            write += 1;
        }
        let byte = bssid[read];
        out[write] = HEX[(byte >> 4) as usize];
        out[write + 1] = HEX[(byte & 0x0f) as usize];
        write += 2;
        read += 1;
    }
}

impl WifiBackend for EspWifiBackend<'_> {
    fn start_ap(&mut self, ssid: &str) -> Result<WifiActionResult<'static>, VmError> {
        let bytes = ssid.as_bytes();
        if bytes.is_empty() || bytes.len() > self.ssid.len() {
            return Ok(error_result("invalid ssid"));
        }

        if self.active || self.station_profile_len > 0 {
            let _ = self.controller.disconnect();
            let _ = self.controller.stop();
            self.clear_active_state();
            self.clear_station_state();
        }

        let config = ModeConfig::AccessPoint(
            AccessPointConfig::default()
                .with_ssid(String::from(ssid))
                .with_max_connections(4),
        );
        if self.controller.set_config(&config).is_err() {
            self.last_backend_code = Some("radio config failed");
            return Ok(error_result("radio config failed"));
        }
        self.configured = true;
        if self.controller.start().is_err() {
            self.last_backend_code = Some("radio start failed");
            return Ok(error_result("radio start failed"));
        }

        self.ssid[..bytes.len()].copy_from_slice(bytes);
        self.ssid_len = bytes.len();
        self.active = true;
        self.clients = 0;
        self.last_backend_code = None;
        Ok(WifiActionResult {
            ok: true,
            error: None,
        })
    }

    fn stop_ap(&mut self) -> Result<WifiActionResult<'static>, VmError> {
        if self.active && self.controller.stop().is_err() {
            self.last_backend_code = Some("radio stop failed");
            return Ok(error_result("radio stop failed"));
        }
        self.clear_active_state();
        self.last_backend_code = None;
        Ok(WifiActionResult {
            ok: true,
            error: None,
        })
    }

    fn connect(&mut self, profile: &str) -> Result<WifiActionResult<'static>, VmError> {
        let _ = profile;
        Ok(error_result("station credentials required"))
    }

    fn disconnect(&mut self) -> Result<WifiActionResult<'static>, VmError> {
        if self.station_profile_len > 0 {
            let _ = self.controller.disconnect();
        }
        if !self.active && self.controller.stop().is_err() {
            self.last_backend_code = Some("radio stop failed");
            return Ok(error_result("radio stop failed"));
        }
        self.clear_station_state();
        if !self.active {
            self.configured = false;
        }
        self.last_backend_code = None;
        Ok(WifiActionResult {
            ok: true,
            error: None,
        })
    }

    fn status<'a>(&'a mut self) -> Result<WifiStatus<'a>, VmError> {
        self.poll_station_link();
        let started = self.controller.is_started().unwrap_or(self.active);
        if !started {
            self.clear_active_state();
        }
        let clients = self.client_count();
        let driver_mode = current_driver_mode_name();
        let channel = if self.active {
            current_ap_channel()
        } else {
            self.station_channel
        };
        Ok(WifiStatus {
            active: self.active,
            mode: if self.active {
                Some("ap")
            } else if self.station_profile_len > 0 {
                Some("sta")
            } else {
                None
            },
            ip_address: if self.active { Some(AP_IP) } else { None },
            ssid: if self.active {
                Some(self.ssid()?)
            } else {
                None
            },
            clients,
            error: None,
            state: if self.active {
                "started"
            } else if self.station_profile_len > 0 && self.last_backend_code.is_none() {
                "started"
            } else if self.last_backend_code.is_some() {
                "error"
            } else {
                "stopped"
            },
            backend: "esp",
            driver_started: started,
            configured: self.configured,
            driver_mode,
            channel,
            ap_start_events: read_counter_i32(&AP_START_EVENTS),
            ap_stop_events: read_counter_i32(&AP_STOP_EVENTS),
            probe_events: read_counter_i32(&AP_PROBE_EVENTS),
            sta_connected_events: read_counter_i32(&AP_STA_CONNECTED_EVENTS),
            sta_disconnected_events: read_counter_i32(&AP_STA_DISCONNECTED_EVENTS),
            last_backend_code: self.last_backend_code,
            profile: if self.station_profile_len > 0 {
                Some(self.station_profile()?)
            } else {
                None
            },
            connected: self.station_connected,
            scan_matches: self.station_scan_matches,
            rssi: self.station_rssi,
            auth: self.station_auth,
            bssid: if self.station_has_bssid {
                core::str::from_utf8(&self.station_bssid).ok()
            } else {
                None
            },
            disconnect_reason: self.station_disconnect_reason,
            disconnect_reason_code: self.station_disconnect_reason_code,
        })
    }

    fn ap_ip<'a>(&'a mut self) -> Result<WifiApIp<'a>, VmError> {
        Ok(WifiApIp {
            ip: if self.active { Some(AP_IP) } else { None },
            gw: if self.active { Some(AP_IP) } else { None },
            netmask: if self.active { Some(AP_NETMASK) } else { None },
            error: None,
        })
    }

    fn scan<'a>(&'a mut self) -> Result<WifiScanResult<'a>, VmError> {
        if self.active || self.station_profile_len > 0 {
            Ok(WifiScanResult {
                ok: false,
                error: Some("wifi busy"),
                networks: &[],
            })
        } else {
            self.clear_scan_results();
            let started_before_scan = self.controller.is_started().unwrap_or(false);
            if !started_before_scan {
                let config = ModeConfig::Client(ClientConfig::default());
                if self.controller.set_config(&config).is_err() {
                    self.last_backend_code = Some("scan config failed");
                    return Ok(WifiScanResult {
                        ok: false,
                        error: Some("scan config failed"),
                        networks: &[],
                    });
                }
                self.configured = true;
                if self.controller.start().is_err() {
                    self.last_backend_code = Some("scan start failed");
                    self.configured = false;
                    return Ok(WifiScanResult {
                        ok: false,
                        error: Some("scan start failed"),
                        networks: &[],
                    });
                }
            }

            let scan_result = match self
                .controller
                .scan_with_config(scan_config(None, WIFI_SCAN_RESULT_CAP))
            {
                Ok(records) => {
                    for (index, record) in records.iter().take(WIFI_SCAN_RESULT_CAP).enumerate() {
                        self.scan_networks[index] = wifi_access_point_from_scan(record)?;
                    }
                    self.scan_len = records.len().min(WIFI_SCAN_RESULT_CAP);
                    Ok(WifiScanResult {
                        ok: true,
                        error: None,
                        networks: &self.scan_networks[..self.scan_len],
                    })
                }
                Err(_) => {
                    self.last_backend_code = Some("scan failed");
                    Ok(WifiScanResult {
                        ok: false,
                        error: Some("scan failed"),
                        networks: &[],
                    })
                }
            };

            if !started_before_scan {
                let _ = self.controller.stop();
                self.configured = false;
            }

            scan_result
        }
    }

    fn teardown(&mut self) -> Result<bool, VmError> {
        let was_active = self.active || self.station_profile_len > 0;
        if self.active {
            let _ = self.controller.stop();
        }
        self.clear_active_state();
        self.last_backend_code = None;
        self.clear_station_state();
        self.clear_scan_results();
        Ok(was_active)
    }
}

fn wifi_access_point_from_scan(
    record: &esp_radio::wifi::AccessPointInfo,
) -> Result<WifiAccessPoint, VmError> {
    let ssid = record.ssid.as_bytes();
    WifiAccessPoint::new(
        ssid,
        Some(record.bssid),
        i32::from(record.channel),
        i32::from(record.signal_strength),
        record.auth_method.map(auth_method_name),
        ssid.is_empty(),
    )
}

fn scan_config(ssid: Option<&str>, max: usize) -> ScanConfig<'_> {
    let config = ScanConfig::default()
        .with_show_hidden(true)
        .with_scan_type(esp_radio::wifi::ScanTypeConfig::Active {
            min: Duration::from_millis(100),
            max: Duration::from_millis(300),
        })
        .with_max(max);
    if let Some(ssid) = ssid {
        config.with_ssid(ssid)
    } else {
        config
    }
}

fn current_driver_mode_name() -> Option<&'static str> {
    let mut mode = wifi_mode_t_WIFI_MODE_NULL;
    let mode_result = unsafe { esp_wifi_get_mode(&mut mode) };
    if mode_result == ESP_OK as _ {
        Some(mode_name(mode))
    } else {
        None
    }
}

fn current_ap_channel() -> i32 {
    let mut config = MaybeUninit::<wifi_config_t>::zeroed();
    let config_result =
        unsafe { esp_wifi_get_config(wifi_interface_t_WIFI_IF_AP, config.as_mut_ptr()) };
    if config_result != ESP_OK as _ {
        return 0;
    }
    let config = unsafe { config.assume_init() };
    let ap = unsafe { config.ap };
    i32::from(ap.channel)
}

fn error_result(error: &'static str) -> WifiActionResult<'static> {
    WifiActionResult {
        ok: false,
        error: Some(error),
    }
}
