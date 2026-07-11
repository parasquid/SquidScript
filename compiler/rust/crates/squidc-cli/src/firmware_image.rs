use std::fmt;

use sha2::{Digest, Sha256};

const IMAGE_HEADER_LEN: usize = 24;
const SEGMENT_HEADER_LEN: usize = 8;
const ESP_IMAGE_MAGIC: u8 = 0xe9;
const ESP32C3_CHIP_ID: u16 = 5;
const APP_DESCRIPTOR_MAGIC: u32 = 0xabcd_5432;
const APP_DESCRIPTOR_LEN: usize = 256;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FirmwareImage {
    pub version: String,
    pub project_name: String,
    pub image_len: usize,
    pub sha256: [u8; 32],
    pub elf_sha256: [u8; 32],
}

impl FirmwareImage {
    pub fn build_id(&self) -> String {
        let mut id = String::with_capacity(16);
        for byte in &self.elf_sha256[..8] {
            use fmt::Write as _;
            write!(&mut id, "{byte:02x}").expect("writing to String cannot fail");
        }
        id
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ImageError {
    Truncated(&'static str),
    InvalidMagic(u8),
    InvalidChip(u16),
    InvalidSegmentCount(u8),
    InvalidAppDescriptor,
    InvalidDescriptorText(&'static str),
    Checksum,
    MissingDigest,
    Digest,
    TrailingData,
}

impl fmt::Display for ImageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated(part) => write!(f, "truncated ESP app image {part}"),
            Self::InvalidMagic(magic) => write!(f, "invalid ESP app image magic 0x{magic:02x}"),
            Self::InvalidChip(chip) => write!(f, "image targets ESP chip id {chip}, not ESP32-C3"),
            Self::InvalidSegmentCount(count) => {
                write!(f, "invalid ESP app image segment count {count}")
            }
            Self::InvalidAppDescriptor => write!(f, "missing ESP application descriptor"),
            Self::InvalidDescriptorText(field) => {
                write!(f, "ESP application descriptor has invalid {field}")
            }
            Self::Checksum => write!(f, "ESP app image checksum mismatch"),
            Self::MissingDigest => write!(f, "ESP app image has no appended SHA-256 digest"),
            Self::Digest => write!(f, "ESP app image SHA-256 digest mismatch"),
            Self::TrailingData => write!(f, "ESP app image contains trailing data"),
        }
    }
}

impl std::error::Error for ImageError {}

pub fn validate(bytes: &[u8]) -> Result<FirmwareImage, ImageError> {
    let header = bytes
        .get(..IMAGE_HEADER_LEN)
        .ok_or(ImageError::Truncated("header"))?;
    if header[0] != ESP_IMAGE_MAGIC {
        return Err(ImageError::InvalidMagic(header[0]));
    }
    let segment_count = header[1];
    if !(1..=16).contains(&segment_count) {
        return Err(ImageError::InvalidSegmentCount(segment_count));
    }
    let chip_id = u16::from_le_bytes([header[12], header[13]]);
    if chip_id != ESP32C3_CHIP_ID {
        return Err(ImageError::InvalidChip(chip_id));
    }
    if header[23] != 1 {
        return Err(ImageError::MissingDigest);
    }

    let mut offset = IMAGE_HEADER_LEN;
    let mut checksum = 0xef;
    let mut descriptor = None;
    for index in 0..segment_count {
        let segment_header = bytes
            .get(offset..offset + SEGMENT_HEADER_LEN)
            .ok_or(ImageError::Truncated("segment header"))?;
        let len =
            u32::from_le_bytes(segment_header[4..8].try_into().expect("fixed slice")) as usize;
        offset = offset
            .checked_add(SEGMENT_HEADER_LEN)
            .ok_or(ImageError::Truncated("segment"))?;
        let data = bytes
            .get(
                offset
                    ..offset
                        .checked_add(len)
                        .ok_or(ImageError::Truncated("segment"))?,
            )
            .ok_or(ImageError::Truncated("segment data"))?;
        for byte in data {
            checksum ^= byte;
        }
        if index == 0 {
            descriptor = Some(parse_descriptor(data)?);
        }
        offset += len;
    }

    let checksum_offset = offset
        .checked_add(15usize.wrapping_sub(offset % 16))
        .ok_or(ImageError::Truncated("checksum"))?;
    let stored_checksum = *bytes
        .get(checksum_offset)
        .ok_or(ImageError::Truncated("checksum"))?;
    if stored_checksum != checksum {
        return Err(ImageError::Checksum);
    }
    let digest_start = checksum_offset + 1;
    let digest_end = digest_start + 32;
    let stored_digest = bytes
        .get(digest_start..digest_end)
        .ok_or(ImageError::Truncated("digest"))?;
    if bytes.len() != digest_end {
        return Err(ImageError::TrailingData);
    }
    let computed = Sha256::digest(&bytes[..digest_start]);
    if computed.as_slice() != stored_digest {
        return Err(ImageError::Digest);
    }
    let (version, project_name, elf_sha256) = descriptor.ok_or(ImageError::InvalidAppDescriptor)?;
    Ok(FirmwareImage {
        version,
        project_name,
        image_len: bytes.len(),
        sha256: Sha256::digest(bytes).into(),
        elf_sha256,
    })
}

fn parse_descriptor(segment: &[u8]) -> Result<(String, String, [u8; 32]), ImageError> {
    let descriptor = segment
        .get(..APP_DESCRIPTOR_LEN)
        .ok_or(ImageError::InvalidAppDescriptor)?;
    if u32::from_le_bytes(descriptor[..4].try_into().expect("fixed slice")) != APP_DESCRIPTOR_MAGIC
    {
        return Err(ImageError::InvalidAppDescriptor);
    }
    Ok((
        descriptor_text(&descriptor[16..48], "version")?,
        descriptor_text(&descriptor[48..80], "project name")?,
        descriptor[144..176]
            .try_into()
            .expect("fixed descriptor slice"),
    ))
}

fn descriptor_text(bytes: &[u8], field: &'static str) -> Result<String, ImageError> {
    let end = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    let value =
        std::str::from_utf8(&bytes[..end]).map_err(|_| ImageError::InvalidDescriptorText(field))?;
    if value.is_empty() {
        return Err(ImageError::InvalidDescriptorText(field));
    }
    Ok(value.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image() -> Vec<u8> {
        let mut bytes = vec![0u8; IMAGE_HEADER_LEN];
        bytes[0] = ESP_IMAGE_MAGIC;
        bytes[1] = 1;
        bytes[12..14].copy_from_slice(&ESP32C3_CHIP_ID.to_le_bytes());
        bytes[23] = 1;
        let mut descriptor = vec![0u8; APP_DESCRIPTOR_LEN];
        descriptor[..4].copy_from_slice(&APP_DESCRIPTOR_MAGIC.to_le_bytes());
        descriptor[16..21].copy_from_slice(b"1.2.3");
        descriptor[48..59].copy_from_slice(b"squidscript");
        descriptor[144..176].fill(0x5a);
        bytes.extend_from_slice(&0x3c00_0020u32.to_le_bytes());
        bytes.extend_from_slice(&(descriptor.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&descriptor);
        let checksum = descriptor
            .iter()
            .fold(0xef, |checksum, byte| checksum ^ byte);
        let checksum_offset = bytes.len() + 15usize.wrapping_sub(bytes.len() % 16);
        bytes.resize(checksum_offset, 0);
        bytes.push(checksum);
        let digest = Sha256::digest(&bytes);
        bytes.extend_from_slice(&digest);
        bytes
    }

    #[test]
    fn validates_structured_esp32c3_image() {
        let bytes = image();
        let parsed = validate(&bytes).unwrap();
        assert_eq!(parsed.version, "1.2.3");
        assert_eq!(parsed.project_name, "squidscript");
        assert_eq!(parsed.image_len, bytes.len());
        assert_eq!(parsed.build_id(), "5a5a5a5a5a5a5a5a");
    }

    #[test]
    fn rejects_corrupt_truncated_and_wrong_chip_images() {
        let bytes = image();
        assert!(matches!(
            validate(&bytes[..40]),
            Err(ImageError::Truncated(_))
        ));

        let mut corrupt = bytes.clone();
        corrupt[100] ^= 1;
        assert_eq!(validate(&corrupt), Err(ImageError::Checksum));

        let mut wrong_chip = bytes;
        wrong_chip[12..14].copy_from_slice(&2u16.to_le_bytes());
        assert_eq!(validate(&wrong_chip), Err(ImageError::InvalidChip(2)));
    }
}
