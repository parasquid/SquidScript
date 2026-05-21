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
