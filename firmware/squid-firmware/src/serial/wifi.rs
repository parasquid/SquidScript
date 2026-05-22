extern crate alloc;

use core::{cell::Cell, fmt, mem::MaybeUninit};

use alloc::string::String;

use critical_section::Mutex;
use esp_hal::peripherals::WIFI;
use esp_radio::{
    wifi::{
        event::{self, EventExt},
        AccessPointConfig, Config, ModeConfig, WifiController, WifiDevice,
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
    host::{SimWifiBackend, WifiActionResult, WifiApIp, WifiBackend, WifiStatus},
};

const AP_IP: &str = "192.168.4.1";
const AP_NETMASK: &str = "255.255.255.0";

static AP_START_EVENTS: Mutex<Cell<u32>> = Mutex::new(Cell::new(0));
static AP_STOP_EVENTS: Mutex<Cell<u32>> = Mutex::new(Cell::new(0));
static AP_PROBE_EVENTS: Mutex<Cell<u32>> = Mutex::new(Cell::new(0));
static AP_STA_CONNECTED_EVENTS: Mutex<Cell<u32>> = Mutex::new(Cell::new(0));
static AP_STA_DISCONNECTED_EVENTS: Mutex<Cell<u32>> = Mutex::new(Cell::new(0));

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

pub enum FirmwareWifiBackend<'d> {
    Sim(SimWifiBackend),
    Esp(EspWifiBackend<'d>),
    Unavailable,
}

impl<'d> FirmwareWifiBackend<'d> {
    pub fn new_esp(radio: &'d Controller<'d>, wifi: WIFI<'d>) -> Self {
        match esp_radio::wifi::new(radio, wifi, Config::default()) {
            Ok((controller, interfaces)) => {
                Self::Esp(EspWifiBackend::new(controller, interfaces.ap))
            }
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
    active: bool,
    ssid: [u8; 32],
    ssid_len: usize,
    clients: i32,
}

impl<'d> EspWifiBackend<'d> {
    fn new(controller: WifiController<'d>, ap_device: WifiDevice<'d>) -> Self {
        Self {
            controller,
            ap_device,
            active: false,
            ssid: [0; 32],
            ssid_len: 0,
            clients: 0,
        }
    }

    fn ssid(&self) -> Result<&str, VmError> {
        core::str::from_utf8(&self.ssid[..self.ssid_len]).map_err(|_| VmError::InvalidUtf8)
    }

    fn clear_active_state(&mut self) {
        self.active = false;
        self.ssid_len = 0;
        self.clients = 0;
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
    }

    fn poll(&mut self) {
        while self.ap_device.receive().is_some() {}
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

impl WifiBackend for EspWifiBackend<'_> {
    fn start_ap(&mut self, ssid: &str) -> Result<WifiActionResult<'static>, VmError> {
        let bytes = ssid.as_bytes();
        if bytes.is_empty() || bytes.len() > self.ssid.len() {
            return Ok(error_result("invalid ssid"));
        }

        if self.active {
            let _ = self.controller.stop();
            self.clear_active_state();
        }

        let config = ModeConfig::AccessPoint(
            AccessPointConfig::default()
                .with_ssid(String::from(ssid))
                .with_max_connections(4),
        );
        if self.controller.set_config(&config).is_err() {
            return Ok(error_result("radio config failed"));
        }
        if self.controller.start().is_err() {
            return Ok(error_result("radio start failed"));
        }

        self.ssid[..bytes.len()].copy_from_slice(bytes);
        self.ssid_len = bytes.len();
        self.active = true;
        self.clients = 0;
        Ok(WifiActionResult {
            ok: true,
            error: None,
        })
    }

    fn stop_ap(&mut self) -> Result<WifiActionResult<'static>, VmError> {
        if self.active && self.controller.stop().is_err() {
            return Ok(error_result("radio stop failed"));
        }
        self.clear_active_state();
        Ok(WifiActionResult {
            ok: true,
            error: None,
        })
    }

    fn status<'a>(&'a mut self) -> Result<WifiStatus<'a>, VmError> {
        let started = self.controller.is_started().unwrap_or(self.active);
        if !started {
            self.clear_active_state();
        }
        let clients = self.client_count();
        Ok(WifiStatus {
            active: self.active,
            mode: if self.active { Some("ap") } else { None },
            ip_address: if self.active { Some(AP_IP) } else { None },
            ssid: if self.active {
                Some(self.ssid()?)
            } else {
                None
            },
            clients,
            error: None,
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

    fn teardown(&mut self) -> Result<bool, VmError> {
        let was_active = self.active;
        if was_active {
            let _ = self.controller.stop();
        }
        self.clear_active_state();
        Ok(was_active)
    }
}

fn error_result(error: &'static str) -> WifiActionResult<'static> {
    WifiActionResult {
        ok: false,
        error: Some(error),
    }
}
