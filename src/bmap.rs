use crate::config::ConfigFile;
use anyhow::{bail, Context, Result};

const RFCOMM_TIMEOUT_MS: u32 = 8_000;

#[cfg(target_os = "macos")]
extern "C" {
    fn bose_rfcomm_send_recv(
        address: *const std::os::raw::c_char,
        channel_id: u8,
        request: *const u8,
        request_len: u16,
        response: *mut u8,
        response_cap: u16,
        response_len: *mut u16,
        timeout_ms: u32,
        error: *mut std::os::raw::c_char,
        error_cap: u16,
    ) -> i32;
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Operator {
    Set = 0,
    Get = 1,
    SetGet = 2,
    Status = 3,
    Error = 4,
    Start = 5,
    Result = 6,
    Processing = 7,
}

impl TryFrom<u8> for Operator {
    type Error = anyhow::Error;
    fn try_from(value: u8) -> Result<Self> {
        Ok(match value & 0x0f {
            0 => Self::Set,
            1 => Self::Get,
            2 => Self::SetGet,
            3 => Self::Status,
            4 => Self::Error,
            5 => Self::Start,
            6 => Self::Result,
            7 => Self::Processing,
            _ => bail!("unknown operator {value}"),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub fblock: u8,
    pub func: u8,
    pub op: Operator,
    pub payload: Vec<u8>,
}

pub fn build_frame(fblock: u8, func: u8, op: Operator, payload: &[u8]) -> Result<Vec<u8>> {
    if payload.len() > u8::MAX as usize {
        bail!("BMAP payload too long: {} bytes", payload.len());
    }
    let mut out = vec![fblock, func, op as u8, payload.len() as u8];
    out.extend_from_slice(payload);
    Ok(out)
}

pub fn parse_frame(bytes: &[u8]) -> Result<Frame> {
    if bytes.len() < 4 {
        bail!("frame too short");
    }
    let len = bytes[3] as usize;
    if bytes.len() < 4 + len {
        bail!("frame payload truncated");
    }
    Ok(Frame {
        fblock: bytes[0],
        func: bytes[1],
        op: Operator::try_from(bytes[2])?,
        payload: bytes[4..4 + len].to_vec(),
    })
}

pub fn parse_frames(bytes: &[u8]) -> Result<Vec<Frame>> {
    let mut frames = Vec::new();
    let mut index = 0;

    while index < bytes.len() {
        if bytes.len() - index < 4 {
            bail!("trailing partial BMAP frame");
        }
        let len = bytes[index + 3] as usize;
        let end = index + 4 + len;
        if end > bytes.len() {
            bail!("frame payload truncated");
        }
        frames.push(parse_frame(&bytes[index..end])?);
        index = end;
    }

    Ok(frames)
}

pub(crate) fn send(
    address: &str,
    channel: u8,
    fblock: u8,
    func: u8,
    op: Operator,
    payload: &[u8],
) -> Result<Frame> {
    send_expect(
        address,
        channel,
        fblock,
        func,
        op,
        payload,
        &[Operator::Status, Operator::Result],
    )
}

fn send_expect(
    address: &str,
    channel: u8,
    fblock: u8,
    func: u8,
    op: Operator,
    payload: &[u8],
    expected_ops: &[Operator],
) -> Result<Frame> {
    let frame = build_frame(fblock, func, op, payload)?;
    let response = transact(address, channel, &frame)?;
    let frames = parse_frames(&response)?;

    let mut saw_matching_address = None;
    for frame in frames {
        if frame.fblock != fblock || frame.func != func {
            continue;
        }
        ensure_not_error(&frame)?;
        if expected_ops.contains(&frame.op) {
            return Ok(frame);
        }
        saw_matching_address = Some(frame.op);
    }

    if let Some(op) = saw_matching_address {
        bail!(
            "unexpected BMAP response operator {:?} on [{}.{}]",
            op,
            fblock,
            func
        );
    }

    bail!("missing BMAP response for [{}.{}]", fblock, func)
}

pub fn send_raw(
    config: &ConfigFile,
    channel: u8,
    fblock: u8,
    func: u8,
    op: Operator,
    payload: &[u8],
) -> Result<Frame> {
    let device = config
        .selected_device
        .as_ref()
        .context("no selected device")?;
    let frame = build_frame(fblock, func, op, payload)?;
    let response = transact(&device.address, channel, &frame)?;
    parse_frames(&response)?
        .into_iter()
        .next()
        .context("empty BMAP response")
}

#[cfg(target_os = "macos")]
fn transact(address: &str, channel: u8, frame: &[u8]) -> Result<Vec<u8>> {
    let address = std::ffi::CString::new(address).context("Bluetooth address contains NUL")?;
    let request_len = u16::try_from(frame.len()).context("BMAP frame too large")?;
    let mut response = vec![0u8; 4096];
    let mut response_len = 0u16;
    let mut error = vec![0i8; 512];

    let status = unsafe {
        bose_rfcomm_send_recv(
            address.as_ptr(),
            channel,
            frame.as_ptr(),
            request_len,
            response.as_mut_ptr(),
            response.len() as u16,
            &mut response_len,
            RFCOMM_TIMEOUT_MS,
            error.as_mut_ptr(),
            error.len() as u16,
        )
    };

    if status != 0 {
        let message = unsafe { std::ffi::CStr::from_ptr(error.as_ptr()) }
            .to_string_lossy()
            .into_owned();
        bail!("{message}");
    }

    response.truncate(response_len as usize);
    Ok(response)
}

#[cfg(not(target_os = "macos"))]
fn transact(_address: &str, _channel: u8, _frame: &[u8]) -> Result<Vec<u8>> {
    bail!("headphone sync is only supported on macOS")
}

fn ensure_not_error(frame: &Frame) -> Result<()> {
    if frame.op != Operator::Error {
        return Ok(());
    }

    let code = frame.payload.first().copied().unwrap_or_default();
    bail!(
        "BMAP error {} on [{}.{}]",
        error_name(code),
        frame.fblock,
        frame.func
    )
}

fn error_name(code: u8) -> &'static str {
    match code {
        0 => "Unknown",
        1 => "Length",
        2 => "Checksum",
        3 => "FunctionBlockNotSupported",
        4 => "FunctionNotSupported",
        5 => "OperatorNotSupportedOrAuthRequired",
        6 => "InvalidData",
        7 => "DataUnavailable",
        8 => "Runtime",
        9 => "Timeout",
        10 => "InvalidState",
        15 => "InvalidTransition",
        20 => "InsecureTransport",
        _ => "Unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_and_parses_frame() {
        let bytes = build_frame(31, 10, Operator::SetGet, &[1, 2, 3]).unwrap();
        let frame = parse_frame(&bytes).unwrap();
        assert_eq!(frame.fblock, 31);
        assert_eq!(frame.func, 10);
        assert_eq!(frame.op, Operator::SetGet);
        assert_eq!(frame.payload, vec![1, 2, 3]);
    }

    #[test]
    fn parses_concatenated_frames() {
        let mut bytes = build_frame(31, 10, Operator::Status, &[1]).unwrap();
        bytes.extend(build_frame(1, 7, Operator::Result, &[2]).unwrap());

        let frames = parse_frames(&bytes).unwrap();

        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].fblock, 31);
        assert_eq!(frames[1].fblock, 1);
    }

    #[test]
    fn rejects_oversized_payload() {
        let payload = vec![0; 256];
        assert!(build_frame(1, 1, Operator::Set, &payload).is_err());
    }

    #[test]
    fn parses_operator_from_flags_low_nibble() {
        let frame = parse_frame(&[31, 3, 0x46, 1, 0]).unwrap();
        assert_eq!(frame.op, Operator::Result);
    }
}
