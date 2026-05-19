use crate::target;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Color {
    White,
    Black,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TextCommand {
    pub x: u16,
    pub y: u16,
    pub text: &'static str,
    pub color: Color,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BuildInfo {
    pub git_hash: &'static str,
    pub profile: &'static str,
}

impl BuildInfo {
    pub const fn new(git_hash: &'static str, profile: &'static str) -> Self {
        Self { git_hash, profile }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Step {
    Init,
    Clear,
    DrawText(&'static str),
    Refresh,
    Sleep,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DisplayError {
    Init,
    Clear,
    Draw,
    Refresh,
    BusyTimeout,
    Sleep,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FirmwareError {
    Display { step: Step, source: DisplayError },
}

pub trait DisplaySurface {
    fn init(&mut self) -> Result<(), DisplayError>;
    fn clear(&mut self, color: Color) -> Result<(), DisplayError>;
    fn draw_text(&mut self, command: TextCommand) -> Result<(), DisplayError>;
    fn refresh(&mut self) -> Result<(), DisplayError>;
    fn sleep(&mut self) -> Result<(), DisplayError>;
}

pub trait Console {
    fn log(&mut self, message: &str);
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SerialProbeInfo {
    pub target_id: &'static str,
    pub target_name: &'static str,
    pub mcu: &'static str,
    pub flash: &'static str,
    pub build_id: &'static str,
    pub transport: &'static str,
}

impl SerialProbeInfo {
    pub const fn esp32c3_super_mini(build_id: &'static str) -> Self {
        Self {
            target_id: "esp32c3-super-mini",
            target_name: "ESP32-C3 Super Mini",
            mcu: "ESP32-C3",
            flash: "4MB",
            build_id,
            transport: "USB Serial/JTAG",
        }
    }
}

pub fn run_serial_probe<C>(console: &mut C, info: SerialProbeInfo)
where
    C: Console,
{
    console.log("SquidScript serial bring-up firmware");
    console.log(info.target_id);
    console.log(info.target_name);
    console.log(info.mcu);
    console.log(info.flash);
    console.log(info.transport);
    console.log(info.build_id);
    console.log("serial probe ready");
}

pub const DIAGNOSTIC_LINES: [TextCommand; 6] = [
    TextCommand {
        x: 32,
        y: 48,
        text: "SquidScript",
        color: Color::Black,
    },
    TextCommand {
        x: 32,
        y: 96,
        text: "XTEINK X4 bring-up",
        color: Color::Black,
    },
    TextCommand {
        x: 32,
        y: 144,
        text: "ESP32-C3 / SSD1677",
        color: Color::Black,
    },
    TextCommand {
        x: 32,
        y: 192,
        text: "Display: 800x480 GDEQ0426T82",
        color: Color::Black,
    },
    TextCommand {
        x: 32,
        y: 288,
        text: "If you can read this,",
        color: Color::Black,
    },
    TextCommand {
        x: 32,
        y: 328,
        text: "Squid firmware is running.",
        color: Color::Black,
    },
];

pub fn run_bringup<D, C>(
    display: &mut D,
    console: &mut C,
    build: BuildInfo,
) -> Result<(), FirmwareError>
where
    D: DisplaySurface,
    C: Console,
{
    console.log("SquidScript X4 bring-up firmware");
    console.log(target::TARGET_ID);
    console.log(build.git_hash);
    console.log(build.profile);

    display.init().map_err(|source| FirmwareError::Display {
        step: Step::Init,
        source,
    })?;
    display
        .clear(Color::White)
        .map_err(|source| FirmwareError::Display {
            step: Step::Clear,
            source,
        })?;

    for command in DIAGNOSTIC_LINES {
        display
            .draw_text(command)
            .map_err(|source| FirmwareError::Display {
                step: Step::DrawText(command.text),
                source,
            })?;
    }

    let build_label = TextCommand {
        x: 32,
        y: 240,
        text: "Build:",
        color: Color::Black,
    };
    display
        .draw_text(build_label)
        .map_err(|source| FirmwareError::Display {
            step: Step::DrawText(build_label.text),
            source,
        })?;

    let build_id = TextCommand {
        x: 160,
        y: 240,
        text: build.git_hash,
        color: Color::Black,
    };
    display
        .draw_text(build_id)
        .map_err(|source| FirmwareError::Display {
            step: Step::DrawText(build_id.text),
            source,
        })?;

    display.refresh().map_err(|source| FirmwareError::Display {
        step: Step::Refresh,
        source,
    })?;
    console.log("display refresh complete");

    display.sleep().map_err(|source| FirmwareError::Display {
        step: Step::Sleep,
        source,
    })?;
    console.log("display sleep complete");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Eq, PartialEq)]
    enum Event {
        Init,
        Clear(Color),
        Draw(TextCommand),
        Refresh,
        Sleep,
        Log(&'static str),
    }

    #[derive(Default)]
    struct MockDisplay {
        events: Vec<Event>,
        fail_refresh: bool,
    }

    impl DisplaySurface for MockDisplay {
        fn init(&mut self) -> Result<(), DisplayError> {
            self.events.push(Event::Init);
            Ok(())
        }

        fn clear(&mut self, color: Color) -> Result<(), DisplayError> {
            self.events.push(Event::Clear(color));
            Ok(())
        }

        fn draw_text(&mut self, command: TextCommand) -> Result<(), DisplayError> {
            self.events.push(Event::Draw(command));
            Ok(())
        }

        fn refresh(&mut self) -> Result<(), DisplayError> {
            self.events.push(Event::Refresh);
            if self.fail_refresh {
                Err(DisplayError::BusyTimeout)
            } else {
                Ok(())
            }
        }

        fn sleep(&mut self) -> Result<(), DisplayError> {
            self.events.push(Event::Sleep);
            Ok(())
        }
    }

    #[derive(Default)]
    struct MockConsole {
        events: Vec<Event>,
    }

    impl Console for MockConsole {
        fn log(&mut self, message: &str) {
            let message = match message {
                "SquidScript X4 bring-up firmware" => "SquidScript X4 bring-up firmware",
                "SquidScript serial bring-up firmware" => "SquidScript serial bring-up firmware",
                "xteink-x4" => "xteink-x4",
                "esp32c3-super-mini" => "esp32c3-super-mini",
                "ESP32-C3 Super Mini" => "ESP32-C3 Super Mini",
                "ESP32-C3" => "ESP32-C3",
                "4MB" => "4MB",
                "USB Serial/JTAG" => "USB Serial/JTAG",
                "test-hash" => "test-hash",
                "test" => "test",
                "serial probe ready" => "serial probe ready",
                "display refresh complete" => "display refresh complete",
                "display sleep complete" => "display sleep complete",
                _ => "unexpected",
            };
            self.events.push(Event::Log(message));
        }
    }

    #[test]
    fn diagnostic_lines_identify_firmware_target_and_display() {
        let text: Vec<_> = DIAGNOSTIC_LINES.iter().map(|line| line.text).collect();

        assert!(text.contains(&"SquidScript"));
        assert!(text.contains(&"XTEINK X4 bring-up"));
        assert!(text.contains(&"ESP32-C3 / SSD1677"));
        assert!(text.contains(&"Display: 800x480 GDEQ0426T82"));
        assert!(text.contains(&"Squid firmware is running."));
    }

    #[test]
    fn bringup_initializes_clears_draws_refreshes_and_sleeps_in_order() {
        let mut display = MockDisplay::default();
        let mut console = MockConsole::default();

        run_bringup(
            &mut display,
            &mut console,
            BuildInfo::new("test-hash", "test"),
        )
        .expect("bring-up should succeed");

        assert_eq!(display.events.first(), Some(&Event::Init));
        assert_eq!(display.events.get(1), Some(&Event::Clear(Color::White)));
        assert!(matches!(display.events.last(), Some(Event::Sleep)));
        assert!(display
            .events
            .iter()
            .any(|event| matches!(event, Event::Draw(command) if command.text == "Build:")));
        assert!(display
            .events
            .iter()
            .any(|event| matches!(event, Event::Draw(command) if command.text == "test-hash")));
        assert!(display
            .events
            .windows(2)
            .any(|window| window == [Event::Refresh, Event::Sleep]));
    }

    #[test]
    fn bringup_reports_busy_timeout_instead_of_hanging() {
        let mut display = MockDisplay {
            fail_refresh: true,
            ..MockDisplay::default()
        };
        let mut console = MockConsole::default();

        let error = run_bringup(
            &mut display,
            &mut console,
            BuildInfo::new("test-hash", "test"),
        )
        .expect_err("refresh failure should be reported");

        assert_eq!(
            error,
            FirmwareError::Display {
                step: Step::Refresh,
                source: DisplayError::BusyTimeout,
            }
        );
        assert!(!display.events.contains(&Event::Sleep));
    }

    #[test]
    fn serial_probe_identifies_super_mini_target() {
        let mut console = MockConsole::default();

        run_serial_probe(
            &mut console,
            SerialProbeInfo::esp32c3_super_mini("test-hash"),
        );

        assert_eq!(
            console.events,
            [
                Event::Log("SquidScript serial bring-up firmware"),
                Event::Log("esp32c3-super-mini"),
                Event::Log("ESP32-C3 Super Mini"),
                Event::Log("ESP32-C3"),
                Event::Log("4MB"),
                Event::Log("USB Serial/JTAG"),
                Event::Log("test-hash"),
                Event::Log("serial probe ready"),
            ]
        );
    }
}
