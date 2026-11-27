//! Flash images as Intel HEX: the pinned 51.292 and planner 51.30 builds embedded at compile time, and the
//! rollback image cut from avrdude's read-back of the controller's flash.

use std::sync::Arc;

use sha2::{Digest, Sha256};

/// The ATmega2560's bootloader section (4096 words) starts here; it is never written back.
pub const BOOTLOADER_START: usize = 0x3E000;
const RECORD_BYTES: usize = 16;
/// `rust/firmware/holey-51.292.sha256`, which `rust/firmware/build.sh` checks the CI build against.
pub const PINNED_SHA256: &str = include_str!("../../../firmware/holey-51.292.sha256");
const EMBEDDED: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/holey-51.292.hex"));
/// `rust/firmware/holey-51.30.sha256`, the planner build's pin.
pub const PLANNER_PINNED_SHA256: &str = include_str!("../../../firmware/holey-51.30.sha256");
const EMBEDDED_PLANNER: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/holey-51.30.hex"));

pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

/// A hex file and the digest it had when it was accepted; [`Image::intact`] checks it again before a flash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    pub hex: Arc<str>,
    pub sha256: String,
}

impl Image {
    pub fn new(hex: &str) -> Image {
        Image { sha256: sha256_hex(hex.as_bytes()), hex: hex.into() }
    }

    /// The image this binary was built with, if `WC_FIRMWARE_HEX` named one; `build.rs` refuses any other digest.
    pub fn embedded() -> Option<Image> {
        Image::pinned(EMBEDDED, PINNED_SHA256)
    }

    /// The planner 51.30 image, if `WC_PLANNER_HEX` named one.
    pub fn embedded_planner() -> Option<Image> {
        Image::pinned(EMBEDDED_PLANNER, PLANNER_PINNED_SHA256)
    }

    pub fn pinned(bytes: &[u8], sha256: &str) -> Option<Image> {
        let hex = std::str::from_utf8(bytes).ok().filter(|h| !h.is_empty())?;
        let image = Image::new(hex);
        (image.sha256 == sha256.trim()).then_some(image)
    }

    pub fn intact(&self) -> bool {
        sha256_hex(self.hex.as_bytes()) == self.sha256
    }
}

fn byte_at(record: &str, at: usize) -> Result<u8, String> {
    record.get(at..at + 2).and_then(|b| u8::from_str_radix(b, 16).ok()).ok_or_else(|| format!("bad hex in {record}"))
}

/// One record's type, address and data, its checksum checked.
fn record(line: &str) -> Result<(u8, usize, Vec<u8>), String> {
    let body = line.strip_prefix(':').ok_or_else(|| format!("not a hex record: {line}"))?;
    if body.len() < 10 || body.len() % 2 != 0 {
        return Err(format!("short hex record: {line}"));
    }
    let bytes = (0..body.len()).step_by(2).map(|at| byte_at(body, at)).collect::<Result<Vec<u8>, String>>()?;
    if bytes.iter().fold(0u8, |sum, b| sum.wrapping_add(*b)) != 0 {
        return Err(format!("checksum mismatch: {line}"));
    }
    let length = usize::from(bytes[0]);
    if bytes.len() != length + 5 {
        return Err(format!("length mismatch: {line}"));
    }
    let address = usize::from(bytes[1]) << 8 | usize::from(bytes[2]);
    Ok((bytes[3], address, bytes[4..4 + length].to_vec()))
}

/// The flash an image writes below the bootloader, unwritten bytes as 0xFF, trailing 0xFF dropped.
pub fn application_bytes(hex: &str) -> Result<Vec<u8>, String> {
    let mut flash = vec![0xFF; BOOTLOADER_START];
    let mut base = 0;
    let mut ended = false;
    for line in hex.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let (kind, address, data) = record(line)?;
        let word = || data.iter().fold(0usize, |value, b| value << 8 | usize::from(*b));
        match kind {
            0 => data.iter().enumerate().for_each(|(i, b)| {
                if let Some(slot) = flash.get_mut(base + address + i) {
                    *slot = *b;
                }
            }),
            1 => {
                ended = true;
                break;
            }
            2 => base = word() << 4,
            4 => base = word() << 16,
            3 | 5 => {}
            other => return Err(format!("unknown record type {other:02X}")),
        }
    }
    let used = flash.iter().rposition(|b| *b != 0xFF).map_or(0, |at| at + 1);
    match (ended, used) {
        (false, _) => Err("the image has no end-of-file record".into()),
        (true, 0) => Err("the image holds no application".into()),
        (true, used) => {
            flash.truncate(used);
            Ok(flash)
        }
    }
}

fn hex_record(kind: u8, address: usize, data: &[u8]) -> String {
    let header = [data.len() as u8, (address >> 8) as u8, address as u8, kind];
    let sum = header.iter().chain(data).fold(0u8, |sum, b| sum.wrapping_add(*b));
    let text: String = header.iter().chain(data).map(|b| format!("{b:02X}")).collect();
    format!(":{text}{:02X}\n", sum.wrapping_neg())
}

/// The application part of an avrdude read-back as a hex file to flash with `-D`: the bootloader stays untouched.
pub fn rollback_image(read_back: &str) -> Result<Image, String> {
    let bytes = application_bytes(read_back)?;
    let mut text = String::new();
    bytes.chunks(RECORD_BYTES).enumerate().for_each(|(i, chunk)| {
        let address = i * RECORD_BYTES;
        if address.is_multiple_of(0x10000) && address > 0 {
            text.push_str(&hex_record(4, 0, &[(address >> 24) as u8, (address >> 16) as u8]));
        }
        text.push_str(&hex_record(0, address & 0xFFFF, chunk));
    });
    text.push_str(&hex_record(1, 0, &[]));
    Ok(Image::new(&text))
}
