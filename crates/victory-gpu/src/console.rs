//! Which console the title runs on. Titles pick settings (resolution,
//! memory budgets) by it, so the user chooses; Series X unless told.

use std::sync::atomic::{AtomicBool, Ordering};

const GIB: usize = 1 << 30;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Console {
    #[default]
    SeriesX,
    SeriesS,
}

impl Console {
    /// Title memory in bytes: (GPU-optimal, standard). Series X in the
    /// advanced configuration; Series S has one range, counted as standard
    /// (7,776 MiB, the published console-mode figure).
    // shortcut: later system updates gave Series S titles more (amount not
    // published here); Grand Theft Auto VI sizes nothing from this figure
    // (it goes by the device type), so update it if a title does.
    pub const fn title_memory(self) -> (usize, usize) {
        match self {
            Console::SeriesX => (10 * GIB, 7 * GIB / 2),
            Console::SeriesS => (0, 7_776 << 20),
        }
    }

    /// Compute units of the GPU.
    pub const fn compute_units(self) -> u32 {
        match self {
            Console::SeriesX => 52,
            Console::SeriesS => 20,
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Console::SeriesX => "Xbox Series X",
            Console::SeriesS => "Xbox Series S",
        }
    }
}

/// Which console the title sees, by the adapter's dedicated video memory:
/// 12 GiB or less names the Series S profile — the Series X one sizes a
/// title's memory at 13.5 GiB (10 of them GPU-optimal) and paces 4K
/// targets, which such a card does not hold, and half the frame went to
/// the Series S mode on a 10 GB RTX 3080 (2,699 → 1,898 ms; #1317, H-1.1).
/// No answer names Series X, as before.
pub fn profile_for(dedicated_video: Option<usize>) -> Console {
    match dedicated_video {
        Some(d) if d <= 12 * GIB => Console::SeriesS,
        _ => Console::SeriesX,
    }
}

/// Which console the host runs: `profile_for` of the adapter the device
/// is created on when none is named — the high-performance one (#3073
/// selects the same; a hybrid machine's display-wired iGPU is not it).
/// `VICTORY_LOG=victory_gpu=info` names the adapter's own account; the
/// console can still be chosen by `--console`.
pub fn profile_by_adapter() -> (Console, Option<usize>) {
    let dedicated = dedicated_video_memory();
    (profile_for(dedicated), dedicated)
}

/// The dedicated video memory, in bytes, of the adapter the device takes
/// when none is named (`D3D12CreateDevice(None, …)`): the
/// high-performance one, skipping the software adapter (which has no
/// video memory to size a console by), and where the host does not answer
/// the preference, the first adapter. None when DXGI does not answer.
fn dedicated_video_memory() -> Option<usize> {
    use windows::Win32::Graphics::Dxgi::{
        CreateDXGIFactory2, DXGI_ADAPTER_FLAG_SOFTWARE, DXGI_CREATE_FACTORY_FLAGS,
        DXGI_GPU_PREFERENCE_HIGH_PERFORMANCE, IDXGIAdapter1, IDXGIFactory4, IDXGIFactory6,
    };
    use windows::core::Interface;
    let memory = |adapter: &IDXGIAdapter1| -> Option<usize> {
        // SAFETY: a plain adapter query.
        let desc = unsafe { adapter.GetDesc1() }.ok()?;
        if desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 != 0 {
            return None;
        }
        Some(desc.DedicatedVideoMemory)
    };
    // SAFETY: plain factory and adapter queries, with no side effects; the
    // factory and adapter are freed on drop.
    unsafe {
        let factory: IDXGIFactory4 = CreateDXGIFactory2(DXGI_CREATE_FACTORY_FLAGS(0)).ok()?;
        // A hybrid machine wires its iGPU to the display as the first
        // adapter, its high-performance one behind it; the device takes
        // the latter (#3073 picks the same).
        if let Ok(f6) = factory.cast::<IDXGIFactory6>()
            && let Ok(adapter) = f6.EnumAdapterByGpuPreference::<IDXGIAdapter1>(
                0,
                DXGI_GPU_PREFERENCE_HIGH_PERFORMANCE,
            )
            && let Some(bytes) = memory(&adapter)
        {
            return Some(bytes);
        }
        let first = factory.EnumAdapters1(0).ok()?;
        memory(&first)
    }
}

static SERIES_S: AtomicBool = AtomicBool::new(false);

/// Set before the title starts.
pub fn set(console: Console) {
    SERIES_S.store(console == Console::SeriesS, Ordering::Relaxed);
}

pub fn current() -> Console {
    if SERIES_S.load(Ordering::Relaxed) {
        Console::SeriesS
    } else {
        Console::SeriesX
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Dedicated video memory of 12 GiB or less names the Series S
    /// profile; more, or no answer, the Series X one.
    #[test]
    fn the_profile_follows_the_adapter_once_it_answers() {
        let gib = 1 << 30;
        assert_eq!(profile_for(None), Console::SeriesX, "no adapter answer");
        assert_eq!(profile_for(Some(0)), Console::SeriesS);
        assert_eq!(profile_for(Some(10 * gib)), Console::SeriesS);
        assert_eq!(
            profile_for(Some(12 * gib)),
            Console::SeriesS,
            "at most 12 GiB"
        );
        assert_eq!(
            profile_for(Some(12 * gib + 1)),
            Console::SeriesX,
            "past 12 GiB"
        );
        assert_eq!(profile_for(Some(16 * gib)), Console::SeriesX);
        assert_eq!(profile_for(Some(24 * gib)), Console::SeriesX);
    }
}
