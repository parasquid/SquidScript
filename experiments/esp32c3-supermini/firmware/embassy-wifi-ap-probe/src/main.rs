#![no_std]
#![no_main]

use core::{net::Ipv4Addr, str::FromStr};

use embassy_executor::Spawner;
use embassy_net::{
    IpListenEndpoint, Ipv4Cidr, Runner, Stack, StackResources, StaticConfigV4, tcp::TcpSocket,
};
use embassy_time::{Duration, Timer};
use esp_alloc as _;
use esp_backtrace as _;
use esp_hal::{
    clock::CpuClock,
    interrupt::software::SoftwareInterruptControl,
    rng::Rng,
    timer::timg::TimerGroup,
};
use esp_println::{print, println};
use esp_radio::wifi::{Config, ControllerConfig, Interface, WifiController, ap::AccessPointConfig};

esp_bootloader_esp_idf::esp_app_desc!();

macro_rules! mk_static {
    ($t:ty, $val:expr) => {{
        static STATIC_CELL: static_cell::StaticCell<$t> = static_cell::StaticCell::new();
        STATIC_CELL.uninit().write($val)
    }};
}

const SSID: &str = "esp-radio";
const GW_IP_ADDR: &str = "192.168.2.1";
const AP_CHANNEL: u8 = 2;

#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    esp_println::logger::init_logger_from_env();
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    esp_alloc::heap_allocator!(size: 96 * 1024);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let sw_int = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    esp_rtos::start(timg0.timer0, sw_int.software_interrupt0);

    let ap_config =
        Config::AccessPoint(AccessPointConfig::default().with_ssid(SSID).with_channel(AP_CHANNEL));

    println!("Starting embassy AP probe");
    let (controller, interfaces) = esp_radio::wifi::new(
        peripherals.WIFI,
        ControllerConfig::default().with_initial_config(ap_config),
    )
    .unwrap();
    let device = interfaces.access_point;
    println!("Wi-Fi controller created for AP SSID `{SSID}` on channel {AP_CHANNEL}");

    let gw_ip_addr = Ipv4Addr::from_str(GW_IP_ADDR).unwrap();
    let net_config = embassy_net::Config::ipv4_static(StaticConfigV4 {
        address: Ipv4Cidr::new(gw_ip_addr, 24),
        gateway: Some(gw_ip_addr),
        dns_servers: Default::default(),
    });

    let rng = Rng::new();
    let seed = (rng.random() as u64) << 32 | rng.random() as u64;
    let (stack, runner) = embassy_net::new(
        device,
        net_config,
        mk_static!(StackResources<3>, StackResources::<3>::new()),
        seed,
    );

    spawner.spawn(connection(controller).unwrap());
    spawner.spawn(net_task(runner).unwrap());
    spawner.spawn(run_dhcp(stack).unwrap());

    println!(
        "Embassy AP probe ready: SSID `{SSID}`, channel {AP_CHANNEL}, gateway http://{GW_IP_ADDR}:8080/"
    );

    stack.wait_config_up().await;
    stack.config_v4().inspect(|c| println!("ipv4 config: {c:?}"));

    let mut rx_buffer = [0; 1536];
    let mut tx_buffer = [0; 1536];
    let mut socket = TcpSocket::new(stack, &mut rx_buffer, &mut tx_buffer);
    socket.set_timeout(Some(Duration::from_secs(10)));

    loop {
        println!("Wait for HTTP connection...");
        let accepted = socket
            .accept(IpListenEndpoint {
                addr: None,
                port: 8080,
            })
            .await;
        println!("HTTP accept result: {:?}", accepted);

        if accepted.is_err() {
            continue;
        }

        use embedded_io_async::Write;

        let mut buffer = [0u8; 1024];
        let mut pos = 0;
        loop {
            match socket.read(&mut buffer[pos..]).await {
                Ok(0) => break,
                Ok(len) => {
                    pos += len;
                    let request = unsafe { core::str::from_utf8_unchecked(&buffer[..pos]) };
                    if request.contains("\r\n\r\n") {
                        print!("{request}");
                        println!();
                        break;
                    }
                }
                Err(e) => {
                    println!("read error: {:?}", e);
                    break;
                }
            }
        }

        let _ = socket
            .write_all(b"HTTP/1.0 200 OK\r\n\r\n<html><body><h1>Embassy AP probe</h1></body></html>\r\n")
            .await
            .inspect_err(|e| println!("write error: {:?}", e));
        let _ = socket
            .flush()
            .await
            .inspect_err(|e| println!("flush error: {:?}", e));
        Timer::after(Duration::from_millis(1000)).await;
        socket.close();
        Timer::after(Duration::from_millis(1000)).await;
        socket.abort();
    }
}

#[embassy_executor::task]
async fn run_dhcp(stack: Stack<'static>) {
    use core::net::{Ipv4Addr, SocketAddrV4};

    use edge_dhcp::{
        io::{self, DEFAULT_SERVER_PORT},
        server::{Server, ServerOptions},
    };
    use edge_nal::UdpBind;
    use edge_nal_embassy::{Udp, UdpBuffers};

    let ip = Ipv4Addr::from_str(GW_IP_ADDR).unwrap();
    let mut buf = [0u8; 1500];
    let mut gw_buf = [Ipv4Addr::UNSPECIFIED];
    let buffers = UdpBuffers::<3, 1024, 1024, 10>::new();
    let unbound_socket = Udp::new(stack, &buffers);
    let mut bound_socket = unbound_socket
        .bind(core::net::SocketAddr::V4(SocketAddrV4::new(
            Ipv4Addr::UNSPECIFIED,
            DEFAULT_SERVER_PORT,
        )))
        .await
        .unwrap();

    loop {
        _ = io::server::run(
            &mut Server::<_, 64>::new_with_et(ip),
            &ServerOptions::new(ip, Some(&mut gw_buf)),
            &mut bound_socket,
            &mut buf,
        )
        .await
        .inspect_err(|e| log::warn!("DHCP server error: {e:?}"));
        Timer::after(Duration::from_millis(500)).await;
    }
}

#[embassy_executor::task]
async fn connection(controller: WifiController<'static>) {
    println!("Start AP event task");
    loop {
        match controller.wait_for_access_point_connected_event_async().await {
            Ok(esp_radio::wifi::AccessPointStationEventInfo::Connected(info)) => {
                println!("Station connected: {:?}", info);
            }
            Ok(esp_radio::wifi::AccessPointStationEventInfo::Disconnected(info)) => {
                println!("Station disconnected: {:?}", info);
            }
            Err(e) => println!("AP event error: {:?}", e),
        }
        Timer::after(Duration::from_millis(5000)).await;
    }
}

#[embassy_executor::task]
async fn net_task(mut runner: Runner<'static, Interface<'static>>) {
    runner.run().await
}
