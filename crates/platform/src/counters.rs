//! Pure helpers for interpreting Windows performance-counter instance names.
//! Kept platform-independent so they are unit-tested on every OS.

/// Instance names that are real adapters rather than virtual/loopback plumbing,
/// so traffic is not counted twice (a VPN or bridge would otherwise duplicate it).
pub fn is_physical_nic(instance: &str) -> bool {
    let l = instance.to_lowercase();
    const VIRTUAL: [&str; 14] = [
        "loopback", "isatap", "teredo", "6to4", "vethernet", "virtualbox", "vmware", "hyper-v", "wan miniport",
        "pseudo", "wintun", "tap-windows", "wireguard", "bluetooth",
    ];
    !VIRTUAL.iter().any(|v| l.contains(v)) && l != "_total"
}

/// `"0,3"` (processor group, logical processor) → `Some(3 + 64*group)`.
pub fn parse_logical_cpu(instance: &str) -> Option<usize> {
    let (g, c) = instance.split_once(',')?;
    Some(g.parse::<usize>().ok()? * 64 + c.parse::<usize>().ok()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nic_filter_skips_virtual_adapters() {
        assert!(is_physical_nic("Intel[R] Ethernet Controller I225-V"));
        assert!(is_physical_nic("Realtek PCIe GbE Family Controller"));
        assert!(is_physical_nic("Intel[R] Wi-Fi 6E AX211 160MHz"));
        assert!(!is_physical_nic("Hyper-V Virtual Ethernet Adapter"));
        assert!(!is_physical_nic("Loopback Pseudo-Interface 1"));
        assert!(!is_physical_nic("_Total"));
    }

    #[test]
    fn logical_cpu_instances() {
        assert_eq!(parse_logical_cpu("0,3"), Some(3));
        assert_eq!(parse_logical_cpu("1,0"), Some(64));
        assert_eq!(parse_logical_cpu("_Total"), None);
        assert_eq!(parse_logical_cpu("0,_Total"), None);
    }
}
