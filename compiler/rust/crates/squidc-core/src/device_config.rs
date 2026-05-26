use std::collections::BTreeMap;

const SQDC_MAGIC: &[u8; 4] = b"SQDC";
const TAG_NULL: u8 = 0;
const TAG_BOOL: u8 = 1;
const TAG_INT: u8 = 2;
const TAG_STRING: u8 = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceConfigValue {
    String(String),
    Int(i32),
    Bool(bool),
    Null,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceConfig {
    records: BTreeMap<String, DeviceConfigValue>,
}

impl DeviceConfig {
    pub fn records(&self) -> &BTreeMap<String, DeviceConfigValue> {
        &self.records
    }

    pub fn get(&self, key: &str) -> Option<&DeviceConfigValue> {
        self.records.get(key)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceConfigError {
    pub message: String,
}

impl DeviceConfigError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

pub fn parse_sqdevice(input: &str) -> Result<DeviceConfig, DeviceConfigError> {
    let mut lines = input.lines().enumerate();
    let Some((_, header)) = lines.find(|(_, line)| !is_ignored_line(line)) else {
        return Err(DeviceConfigError::new("missing SQDEVICE header"));
    };
    if header.trim() != "SQDEVICE" {
        return Err(DeviceConfigError::new("unsupported SQDEVICE header"));
    }

    let mut records = BTreeMap::new();
    for (index, line) in lines {
        let line = line.trim();
        if is_ignored_line(line) {
            continue;
        }
        let (key, rest) = split_word(line).ok_or_else(|| {
            DeviceConfigError::new(format!("invalid record on line {}", index + 1))
        })?;
        let (value_type, value_text) = split_word(rest).unwrap_or((rest.trim(), ""));
        if records.contains_key(key) {
            return Err(DeviceConfigError::new(format!("duplicate key {key}")));
        }
        records.insert(key.to_string(), parse_value(value_type, value_text)?);
    }

    Ok(DeviceConfig { records })
}

pub fn encode_sqdc(config: &DeviceConfig) -> Result<Vec<u8>, DeviceConfigError> {
    let mut out = Vec::new();
    out.extend_from_slice(SQDC_MAGIC);
    write_u16(
        &mut out,
        u16::try_from(config.records.len())
            .map_err(|_| DeviceConfigError::new("too many SQDC records"))?,
    );
    for (key, value) in &config.records {
        write_string(&mut out, key)?;
        match value {
            DeviceConfigValue::Null => out.push(TAG_NULL),
            DeviceConfigValue::Bool(value) => {
                out.push(TAG_BOOL);
                out.push(u8::from(*value));
            }
            DeviceConfigValue::Int(value) => {
                out.push(TAG_INT);
                out.extend_from_slice(&value.to_le_bytes());
            }
            DeviceConfigValue::String(value) => {
                out.push(TAG_STRING);
                write_string(&mut out, value)?;
            }
        }
    }
    Ok(out)
}

pub fn decode_sqdc(bytes: &[u8]) -> Result<DeviceConfig, DeviceConfigError> {
    if bytes.len() < 6 || &bytes[0..4] != SQDC_MAGIC {
        return Err(DeviceConfigError::new("invalid SQDC header"));
    }
    let count = read_u16(bytes, 4)? as usize;
    let mut cursor = 6usize;
    let mut records = BTreeMap::new();
    for _ in 0..count {
        let key = read_string(bytes, &mut cursor)?;
        if records.contains_key(&key) {
            return Err(DeviceConfigError::new(format!("duplicate key {key}")));
        }
        let tag = *bytes
            .get(cursor)
            .ok_or_else(|| DeviceConfigError::new("truncated SQDC value"))?;
        cursor += 1;
        let value = match tag {
            TAG_NULL => DeviceConfigValue::Null,
            TAG_BOOL => {
                let value = *bytes
                    .get(cursor)
                    .ok_or_else(|| DeviceConfigError::new("truncated SQDC bool"))?;
                cursor += 1;
                DeviceConfigValue::Bool(value != 0)
            }
            TAG_INT => {
                let end = cursor
                    .checked_add(4)
                    .ok_or_else(|| DeviceConfigError::new("invalid SQDC int"))?;
                let data = bytes
                    .get(cursor..end)
                    .ok_or_else(|| DeviceConfigError::new("truncated SQDC int"))?;
                cursor = end;
                DeviceConfigValue::Int(i32::from_le_bytes(data.try_into().unwrap()))
            }
            TAG_STRING => DeviceConfigValue::String(read_string(bytes, &mut cursor)?),
            _ => return Err(DeviceConfigError::new("unknown SQDC value tag")),
        };
        records.insert(key, value);
    }
    if cursor != bytes.len() {
        return Err(DeviceConfigError::new("trailing SQDC bytes"));
    }
    Ok(DeviceConfig { records })
}

pub fn is_safe_sqdevice_path(path: &str) -> bool {
    if path.is_empty()
        || !path.ends_with(".sqdevice")
        || path.starts_with('/')
        || path.starts_with("sd/")
        || path.starts_with("system/")
        || path.contains('\\')
    {
        return false;
    }
    path.split('/')
        .all(|part| !part.is_empty() && part != "." && part != "..")
}

pub fn is_safe_device_binding_resource(path: &str) -> bool {
    is_safe_sqdevice_path(path)
        || is_safe_gpio_binding_resource(path)
        || is_safe_gpio_button_binding_resource(path)
}

fn is_safe_gpio_binding_resource(path: &str) -> bool {
    let Some(pin) = path.strip_prefix("gpio:GPIO") else {
        return false;
    };
    !pin.is_empty() && pin.len() <= 2 && pin.bytes().all(|byte| byte.is_ascii_digit())
}

fn is_safe_gpio_button_binding_resource(path: &str) -> bool {
    let Some(rest) = path.strip_prefix("gpio-button:GPIO") else {
        return false;
    };
    let Some((pin, rest)) = rest.split_once(':') else {
        return false;
    };
    let Some((event, polarity)) = rest.split_once(':') else {
        return false;
    };
    !pin.is_empty()
        && pin.len() <= 2
        && pin.bytes().all(|byte| byte.is_ascii_digit())
        && is_valid_key_event(event)
        && matches!(polarity, "activeLow" | "activeHigh")
}

fn is_valid_key_event(event: &str) -> bool {
    let Some(key) = event.strip_prefix("key.") else {
        return false;
    };
    matches!(
        key,
        "UP" | "DOWN" | "LEFT" | "RIGHT" | "SELECT" | "BACK" | "MENU" | "HOME" | "POWER"
    )
}

fn is_ignored_line(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.is_empty() || trimmed.starts_with('#')
}

fn split_word(input: &str) -> Option<(&str, &str)> {
    let input = input.trim_start();
    if input.is_empty() {
        return None;
    }
    let split = input.find(char::is_whitespace)?;
    Some((&input[..split], input[split..].trim_start()))
}

fn parse_value(value_type: &str, value_text: &str) -> Result<DeviceConfigValue, DeviceConfigError> {
    match value_type {
        "string" => parse_len_string(value_text).map(DeviceConfigValue::String),
        "int" => value_text
            .parse::<i32>()
            .map(DeviceConfigValue::Int)
            .map_err(|_| DeviceConfigError::new("invalid SQDEVICE int")),
        "bool" => match value_text {
            "true" => Ok(DeviceConfigValue::Bool(true)),
            "false" => Ok(DeviceConfigValue::Bool(false)),
            _ => Err(DeviceConfigError::new("invalid SQDEVICE bool")),
        },
        "null" if value_text.is_empty() => Ok(DeviceConfigValue::Null),
        "null" => Err(DeviceConfigError::new("null records must not have a value")),
        _ => Err(DeviceConfigError::new("unknown SQDEVICE value type")),
    }
}

fn parse_len_string(input: &str) -> Result<String, DeviceConfigError> {
    let Some((len, value)) = input.split_once(':') else {
        return Err(DeviceConfigError::new("invalid length-prefixed string"));
    };
    let len = len
        .parse::<usize>()
        .map_err(|_| DeviceConfigError::new("invalid string length"))?;
    if value.as_bytes().len() != len {
        return Err(DeviceConfigError::new("string length mismatch"));
    }
    Ok(value.to_string())
}

fn write_string(out: &mut Vec<u8>, value: &str) -> Result<(), DeviceConfigError> {
    write_u16(
        out,
        u16::try_from(value.as_bytes().len())
            .map_err(|_| DeviceConfigError::new("SQDC string too long"))?,
    );
    out.extend_from_slice(value.as_bytes());
    Ok(())
}

fn read_string(bytes: &[u8], cursor: &mut usize) -> Result<String, DeviceConfigError> {
    let len = read_u16(bytes, *cursor)? as usize;
    *cursor += 2;
    let end = (*cursor)
        .checked_add(len)
        .ok_or_else(|| DeviceConfigError::new("invalid SQDC string"))?;
    let value = bytes
        .get(*cursor..end)
        .ok_or_else(|| DeviceConfigError::new("truncated SQDC string"))?;
    *cursor = end;
    std::str::from_utf8(value)
        .map(str::to_string)
        .map_err(|_| DeviceConfigError::new("SQDC string is not utf-8"))
}

fn write_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, DeviceConfigError> {
    let end = offset
        .checked_add(2)
        .ok_or_else(|| DeviceConfigError::new("invalid SQDC u16"))?;
    let data = bytes
        .get(offset..end)
        .ok_or_else(|| DeviceConfigError::new("truncated SQDC u16"))?;
    Ok(u16::from_le_bytes(data.try_into().unwrap()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sqdevice_records_and_comments() {
        let config = parse_sqdevice(
            r#"SQDEVICE
# comment
service string 17:indicator.default
backend string 4:gpio
gpio string 6:GPIO10
activeLow bool false
unused int 42
none null
"#,
        )
        .unwrap();

        assert_eq!(
            config.get("service"),
            Some(&DeviceConfigValue::String("indicator.default".to_string()))
        );
        assert_eq!(
            config.get("activeLow"),
            Some(&DeviceConfigValue::Bool(false))
        );
        assert_eq!(config.get("unused"), Some(&DeviceConfigValue::Int(42)));
        assert_eq!(config.get("none"), Some(&DeviceConfigValue::Null));
    }

    #[test]
    fn rejects_duplicate_keys_and_bad_lengths() {
        assert!(parse_sqdevice("SQDEVICE\ngpio string 5:GPIO10\n").is_err());
        assert!(parse_sqdevice("SQDEVICE\ngpio string 6:GPIO10\ngpio null\n").is_err());
    }

    #[test]
    fn round_trips_sqdc() {
        let config =
            parse_sqdevice("SQDEVICE\nservice string 17:indicator.default\nactiveLow bool true\n")
                .unwrap();
        let bytes = encode_sqdc(&config).unwrap();
        assert_eq!(decode_sqdc(&bytes).unwrap(), config);
    }

    #[test]
    fn validates_sqdevice_paths() {
        assert!(is_safe_sqdevice_path("device/indicator.sqdevice"));
        assert!(!is_safe_sqdevice_path("../indicator.sqdevice"));
        assert!(!is_safe_sqdevice_path("/device/indicator.sqdevice"));
        assert!(!is_safe_sqdevice_path("device\\indicator.sqdevice"));
        assert!(!is_safe_sqdevice_path("device/indicator.txt"));
        assert!(!is_safe_sqdevice_path("sd/apps/indicator.sqdevice"));
    }

    #[test]
    fn validates_inline_gpio_binding_resources() {
        assert!(is_safe_device_binding_resource("gpio:GPIO8"));
        assert!(is_safe_device_binding_resource("gpio:GPIO10"));
        assert!(is_safe_device_binding_resource(
            "gpio-button:GPIO9:key.SELECT:activeLow"
        ));
        assert!(!is_safe_device_binding_resource("gpio:"));
        assert!(!is_safe_device_binding_resource("gpio:PIN8"));
        assert!(!is_safe_device_binding_resource("gpio:GPIO100"));
        assert!(!is_safe_device_binding_resource("gpio:GPIO8/../x"));
        assert!(!is_safe_device_binding_resource(
            "gpio-button:GPIO9:key.BOOT:activeLow"
        ));
        assert!(!is_safe_device_binding_resource(
            "gpio-button:GPIO9:SELECT:activeLow"
        ));
        assert!(!is_safe_device_binding_resource(
            "gpio-button:GPIO9:key.SELECT:inverted"
        ));
    }
}
