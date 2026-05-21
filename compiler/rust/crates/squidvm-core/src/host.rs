use core::fmt;

use crate::{error::VmError, strings::StringResolver, value::Value};

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
    fn service_wifi_status<'a>(&'a mut self) -> Result<WifiStatus<'a>, VmError> {
        Err(VmError::InvalidOperand)
    }
    fn service_wifi_get_ap_ip<'a>(&'a mut self) -> Result<WifiApIp<'a>, VmError> {
        Err(VmError::InvalidOperand)
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
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WifiApIp<'a> {
    pub ip: Option<&'a str>,
    pub gw: Option<&'a str>,
    pub netmask: Option<&'a str>,
    pub error: Option<&'a str>,
}

const SIM_WIFI_SSID_CAP: usize = 32;
const SIM_WIFI_AP_IP: &str = "192.168.4.1";
const SIM_WIFI_AP_NETMASK: &str = "255.255.255.0";

pub trait WifiBackend {
    fn start_ap(&mut self, ssid: &str) -> Result<WifiActionResult<'static>, VmError>;
    fn stop_ap(&mut self) -> Result<WifiActionResult<'static>, VmError>;
    fn status<'a>(&'a mut self) -> Result<WifiStatus<'a>, VmError>;
    fn ap_ip<'a>(&'a mut self) -> Result<WifiApIp<'a>, VmError>;
    fn teardown(&mut self) -> Result<bool, VmError>;
}

pub struct SimWifiBackend {
    active: bool,
    ssid: [u8; SIM_WIFI_SSID_CAP],
    ssid_len: usize,
    clients: i32,
}

impl SimWifiBackend {
    pub const fn new() -> Self {
        Self {
            active: false,
            ssid: [0; SIM_WIFI_SSID_CAP],
            ssid_len: 0,
            clients: 0,
        }
    }

    pub fn set_clients(&mut self, clients: i32) {
        self.clients = if self.active { clients.max(0) } else { 0 };
    }

    fn ssid(&self) -> Result<&str, VmError> {
        core::str::from_utf8(&self.ssid[..self.ssid_len]).map_err(|_| VmError::InvalidUtf8)
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

    fn teardown(&mut self) -> Result<bool, VmError> {
        let was_active = self.active;
        self.active = false;
        self.ssid_len = 0;
        self.clients = 0;
        Ok(was_active)
    }
}

#[cfg(test)]
mod wifi_backend_tests {
    use super::{SimWifiBackend, WifiBackend};

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
}
