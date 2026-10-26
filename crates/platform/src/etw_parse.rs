//! Interpreting ETW "present" events. Pure and platform-independent so it is
//! unit-tested everywhere; the Windows-only `windows::etw` feeds it raw records.
//!
//! We only use the cheap, stable part of the events: *which provider*, *which
//! event id*, and the swap-chain pointer at the start of the payload. Frame time
//! is the difference between consecutive present timestamps of the same swap chain
//! (what Intel PresentMon reports as "MsBetweenPresents").

use crate::capture::Api;

/// Microsoft-Windows-DXGI {CA11C036-0102-4A2D-A6AD-F03CFED5D3C9}
pub const DXGI_PROVIDER: u128 = 0xCA11C036_0102_4A2D_A6AD_F03CFED5D3C9;
/// Microsoft-Windows-D3D9 {783ACA0A-790E-4D7F-8451-AA850511C6B9}
pub const D3D9_PROVIDER: u128 = 0x783ACA0A_790E_4D7F_8451_AA850511C6B9;
/// `IDXGISwapChain::Present` entry (also covers Present1).
pub const DXGI_PRESENT_START: u16 = 42;
/// `IDirect3DDevice9::Present` entry.
pub const D3D9_PRESENT_START: u16 = 1;

/// `EVENT_HEADER_FLAG_32_BIT_HEADER`: the traced process is 32-bit (4-byte pointers in the payload).
const FLAG_32_BIT: u16 = 0x0020;
/// `DXGI_PRESENT_TEST`: a probe call that presents nothing.
const DXGI_PRESENT_TEST: u32 = 0x0000_0001;

fn read_ptr(data: &[u8], is_32: bool) -> Option<(u64, usize)> {
    if is_32 {
        let b = data.get(0..4)?;
        Some((u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as u64, 4))
    } else {
        let b = data.get(0..8)?;
        Some((u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]), 8))
    }
}

/// If this event is "a frame was presented", return its swap chain and API.
pub fn parse_present(provider: u128, event_id: u16, header_flags: u16, payload: &[u8]) -> Option<(u64, Api)> {
    let is_32 = header_flags & FLAG_32_BIT != 0;
    match (provider, event_id) {
        (DXGI_PROVIDER, DXGI_PRESENT_START) => {
            let (chain, off) = read_ptr(payload, is_32)?;
            if let Some(f) = payload.get(off..off + 4) {
                let flags = u32::from_le_bytes([f[0], f[1], f[2], f[3]]);
                if flags & DXGI_PRESENT_TEST != 0 {
                    return None;
                }
            }
            Some((chain, Api::Dxgi))
        }
        (D3D9_PROVIDER, D3D9_PRESENT_START) => {
            // Payload shape differs between Windows builds; the swap chain is optional here.
            let chain = read_ptr(payload, is_32).map(|(c, _)| c).unwrap_or(0);
            Some((chain, Api::D3d9))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn payload64(chain: u64, flags: u32) -> Vec<u8> {
        let mut v = chain.to_le_bytes().to_vec();
        v.extend(flags.to_le_bytes());
        v.extend(1u32.to_le_bytes());
        v
    }

    #[test]
    fn dxgi_present_yields_swapchain() {
        let r = parse_present(DXGI_PROVIDER, 42, 0x0040, &payload64(0xDEAD_BEEF_1234, 0));
        assert_eq!(r, Some((0xDEAD_BEEF_1234, Api::Dxgi)));
    }

    #[test]
    fn test_presents_are_ignored() {
        assert_eq!(parse_present(DXGI_PROVIDER, 42, 0x0040, &payload64(1, 1)), None);
    }

    #[test]
    fn other_events_are_ignored() {
        assert_eq!(parse_present(DXGI_PROVIDER, 43, 0, &payload64(1, 0)), None, "Present_Stop is not a frame");
        assert_eq!(parse_present(0x1234, 42, 0, &payload64(1, 0)), None);
    }

    #[test]
    fn thirty_two_bit_processes_use_four_byte_pointers() {
        let mut p = 0xAABB_CCDDu32.to_le_bytes().to_vec();
        p.extend(0u32.to_le_bytes());
        p.extend(1u32.to_le_bytes());
        assert_eq!(parse_present(DXGI_PROVIDER, 42, 0x0020, &p), Some((0xAABB_CCDD, Api::Dxgi)));
    }

    #[test]
    fn short_payload_never_panics() {
        for n in 0..12 {
            let _ = parse_present(DXGI_PROVIDER, 42, 0, &vec![0u8; n]);
            let _ = parse_present(D3D9_PROVIDER, 1, 0x20, &vec![0u8; n]);
        }
        assert_eq!(parse_present(DXGI_PROVIDER, 42, 0, &[1, 2, 3]), None, "no swap chain => not usable");
        assert_eq!(parse_present(D3D9_PROVIDER, 1, 0, &[]), Some((0, Api::D3d9)));
    }
}
