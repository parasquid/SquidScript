#![no_std]
#![no_main]

use core::mem::MaybeUninit;

use blocking_network_stack::Stack;
use esp_backtrace as _;
use esp_hal::{
    clock::CpuClock,
    interrupt::software::SoftwareInterruptControl,
    main,
    rng::Rng,
    time::{self, Duration},
    timer::timg::TimerGroup,
};
use esp_println::println;
use esp_radio::wifi::{
    AccessPointConfig, ModeConfig,
    event::{self, EventExt},
};
use esp_wifi_sys::include::{
    esp_wifi_get_config, esp_wifi_get_country, esp_wifi_get_max_tx_power, esp_wifi_get_mode,
    wifi_config_t, wifi_country_t, wifi_interface_t_WIFI_IF_AP, wifi_mode_t,
    wifi_mode_t_WIFI_MODE_AP, wifi_mode_t_WIFI_MODE_APSTA, wifi_mode_t_WIFI_MODE_NULL,
    wifi_mode_t_WIFI_MODE_STA,
};
use smoltcp::iface::{SocketSet, SocketStorage};

esp_bootloader_esp_idf::esp_app_desc!();

const SSID: &str = "esp-radio";

#[main]
fn main() -> ! {
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    esp_alloc::heap_allocator!(size: 96 * 1024);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let software_interrupt = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    esp_rtos::start(timg0.timer0, software_interrupt.software_interrupt0);

    let mut connections = 0u32;
    _ = event::ApStart::replace_handler(|_| println!("ap start event"));
    event::ApStaConnected::update_handler(move |event| {
        connections += 1;
        println!("connected {}, mac: {:?}", connections, event.mac());
    });
    event::ApStaDisconnected::update_handler(|event| {
        println!(
            "disconnected mac: {:?}, reason: {:?}",
            event.mac(),
            event.reason()
        );
    });

    let radio = esp_radio::init().unwrap();
    let (mut controller, interfaces) =
        esp_radio::wifi::new(&radio, peripherals.WIFI, Default::default()).unwrap();
    let mut device = interfaces.ap;
    let iface = create_interface(&mut device);
    let now = || time::Instant::now().duration_since_epoch().as_millis();
    let rng = Rng::new();
    let mut socket_set_entries: [SocketStorage; 3] = Default::default();
    let socket_set = SocketSet::new(&mut socket_set_entries[..]);
    let mut stack = Stack::new(iface, device, socket_set, now, rng.random());

    let ap_config = ModeConfig::AccessPoint(AccessPointConfig::default().with_ssid(SSID.into()));
    println!(
        "wifi_set_configuration returned {:?}",
        controller.set_config(&ap_config)
    );
    print_wifi_diagnostics("after_config");
    controller.start().unwrap();
    println!("is wifi started: {:?}", controller.is_started());
    println!("{:?}", controller.capabilities());
    print_wifi_diagnostics("after_start");
    println!("AP probe SSID: {SSID}");

    stack
        .set_iface_configuration(&blocking_network_stack::ipv4::Configuration::Client(
            blocking_network_stack::ipv4::ClientConfiguration::Fixed(
                blocking_network_stack::ipv4::ClientSettings {
                    ip: blocking_network_stack::ipv4::Ipv4Addr::from(parse_ip("192.168.2.1")),
                    subnet: blocking_network_stack::ipv4::Subnet {
                        gateway: blocking_network_stack::ipv4::Ipv4Addr::from(parse_ip(
                            "192.168.2.1",
                        )),
                        mask: blocking_network_stack::ipv4::Mask(24),
                    },
                    dns: None,
                    secondary_dns: None,
                },
            ),
        ))
        .unwrap();

    let mut rx_buffer = [0u8; 1536];
    let mut tx_buffer = [0u8; 1536];
    let mut socket = stack.get_socket(&mut rx_buffer, &mut tx_buffer);
    socket.listen(8080).unwrap();

    loop {
        socket.work();
        if !socket.is_open() {
            socket.listen(8080).unwrap();
        }

        let start = time::Instant::now();
        while start.elapsed() < Duration::from_millis(100) {
            socket.work();
        }
    }
}

fn print_wifi_diagnostics(label: &str) {
    unsafe {
        let mut mode = MaybeUninit::<wifi_mode_t>::uninit();
        let mode_result = esp_wifi_get_mode(mode.as_mut_ptr());
        if mode_result == 0 {
            let mode = mode.assume_init();
            println!("wifi_diag {label}: mode={} ({})", mode, wifi_mode_name(mode));
        } else {
            println!("wifi_diag {label}: get_mode_err={mode_result}");
        }

        let mut cfg = MaybeUninit::<wifi_config_t>::zeroed();
        let config_result = esp_wifi_get_config(wifi_interface_t_WIFI_IF_AP, cfg.as_mut_ptr());
        if config_result == 0 {
            let ap = cfg.assume_init().ap;
            let ssid_len = ap.ssid_len as usize;
            let mut ssid = [0u8; 32];
            let copy_len = ssid_len.min(ap.ssid.len());
            ssid[..copy_len].copy_from_slice(&ap.ssid[..copy_len]);
            let ssid_text = core::str::from_utf8(&ssid[..copy_len]).unwrap_or("<non-utf8>");
            println!(
                "wifi_diag {label}: ap ssid={ssid_text} ssid_len={} channel={} hidden={} authmode={} max_connection={} beacon_interval={} dtim_period={}",
                ap.ssid_len,
                ap.channel,
                ap.ssid_hidden,
                ap.authmode,
                ap.max_connection,
                ap.beacon_interval,
                ap.dtim_period
            );
        } else {
            println!("wifi_diag {label}: get_ap_config_err={config_result}");
        }

        let mut country = MaybeUninit::<wifi_country_t>::uninit();
        let country_result = esp_wifi_get_country(country.as_mut_ptr());
        if country_result == 0 {
            let country = country.assume_init();
            let cc0 = country.cc[0] as u8 as char;
            let cc1 = country.cc[1] as u8 as char;
            let cc2 = country.cc[2] as u8 as char;
            println!(
                "wifi_diag {label}: country={}{}{} schan={} nchan={} max_tx_power={} policy={}",
                cc0,
                cc1,
                cc2,
                country.schan,
                country.nchan,
                country.max_tx_power,
                country.policy
            );
        } else {
            println!("wifi_diag {label}: get_country_err={country_result}");
        }

        let mut tx_power = MaybeUninit::<i8>::uninit();
        let tx_power_result = esp_wifi_get_max_tx_power(tx_power.as_mut_ptr());
        if tx_power_result == 0 {
            println!(
                "wifi_diag {label}: max_tx_power_quarter_dbm={}",
                tx_power.assume_init()
            );
        } else {
            println!("wifi_diag {label}: get_max_tx_power_err={tx_power_result}");
        }
    }
}

fn wifi_mode_name(mode: wifi_mode_t) -> &'static str {
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

fn parse_ip(ip: &str) -> [u8; 4] {
    let mut result = [0u8; 4];
    for (idx, octet) in ip.split('.').enumerate() {
        result[idx] = u8::from_str_radix(octet, 10).unwrap();
    }
    result
}

fn create_interface(device: &mut esp_radio::wifi::WifiDevice) -> smoltcp::iface::Interface {
    smoltcp::iface::Interface::new(
        smoltcp::iface::Config::new(smoltcp::wire::HardwareAddress::Ethernet(
            smoltcp::wire::EthernetAddress::from_bytes(&device.mac_address()),
        )),
        device,
        timestamp(),
    )
}

fn timestamp() -> smoltcp::time::Instant {
    smoltcp::time::Instant::from_micros(
        esp_hal::time::Instant::now()
            .duration_since_epoch()
            .as_micros() as i64,
    )
}
