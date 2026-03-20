use std::sync::OnceLock;

#[cfg(target_os = "macos")]
#[repr(C)]
#[derive(Clone, Copy)]
struct MachTimebaseInfo {
    numer: u32,
    denom: u32,
}

#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn mach_absolute_time() -> u64;
    fn mach_timebase_info(info: *mut MachTimebaseInfo) -> i32;
}

#[cfg(target_os = "macos")]
fn mach_timebase_info_now() -> MachTimebaseInfo {
    static TIMEBASE: OnceLock<MachTimebaseInfo> = OnceLock::new();
    *TIMEBASE.get_or_init(|| {
        let mut info = MachTimebaseInfo { numer: 0, denom: 0 };
        let status = unsafe { mach_timebase_info(&mut info) };
        assert!(
            status == 0 && info.denom != 0,
            "mach_timebase_info failed with status {status}"
        );
        info
    })
}

pub fn terminal_ui_monotonic_now_ns() -> u64 {
    #[cfg(target_os = "macos")]
    {
        let info = mach_timebase_info_now();
        let ticks = unsafe { mach_absolute_time() };
        let nanos =
            u128::from(ticks).saturating_mul(u128::from(info.numer)) / u128::from(info.denom);
        return nanos.min(u128::from(u64::MAX)) as u64;
    }

    #[cfg(not(target_os = "macos"))]
    {
        use std::time::Instant;

        static START_INSTANT: OnceLock<Instant> = OnceLock::new();
        let start = START_INSTANT.get_or_init(Instant::now);

        let duration = Instant::now().duration_since(*start);
        duration.as_nanos().min(u128::from(u64::MAX)) as u64
    }
}

#[cfg(test)]
mod tests {
    use super::terminal_ui_monotonic_now_ns;

    #[test]
    fn monotonic_now_ns_is_non_decreasing() {
        let first = terminal_ui_monotonic_now_ns();
        let second = terminal_ui_monotonic_now_ns();
        assert!(second >= first);
    }
}
