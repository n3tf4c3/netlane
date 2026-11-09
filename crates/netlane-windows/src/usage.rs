//! Reads the per-app data usage Windows records for each connection profile
//! (`ConnectionProfile.GetAttributedNetworkUsageAsync`). Read-only, no elevation.
use anyhow::{Result, bail};
use netlane_core::usage::{AppBytes, NetworkKind, NetworkUsage, Period};
use windows::{
    Foundation::DateTime,
    Networking::Connectivity::{NetworkInformation, NetworkUsageStates, TriStates},
};
use windows_sys::Win32::{
    Foundation::{FILETIME, SYSTEMTIME},
    Storage::FileSystem::QueryDosDeviceW,
    System::{
        SystemInformation::{GetLocalTime, GetSystemTimeAsFileTime},
        Time::{FileTimeToSystemTime, SystemTimeToFileTime, TzSpecificLocalTimeToSystemTime},
    },
};

const TICKS_PER_DAY: i64 = 24 * 60 * 60 * 10_000_000;

fn ticks(time: FILETIME) -> i64 {
    ((time.dwHighDateTime as i64) << 32) | time.dwLowDateTime as i64
}
fn filetime(ticks: i64) -> FILETIME {
    FILETIME {
        dwLowDateTime: ticks as u32,
        dwHighDateTime: (ticks >> 32) as u32,
    }
}

/// UTC range from local midnight `days - 1` days ago until now, in FILETIME ticks.
fn range(period: Period) -> Result<(i64, i64)> {
    // SAFETY: Plain value conversions on stack structs; every out-pointer is valid.
    unsafe {
        let mut now = FILETIME::default();
        GetSystemTimeAsFileTime(&mut now);
        let mut local = SYSTEMTIME::default();
        GetLocalTime(&mut local);
        local.wHour = 0;
        local.wMinute = 0;
        local.wSecond = 0;
        local.wMilliseconds = 0;
        let mut midnight = FILETIME::default();
        if SystemTimeToFileTime(&local, &mut midnight) == 0 {
            bail!("Data local inválida.");
        }
        // Calendar arithmetic stays in local time; the zone (and its DST rule)
        // is applied only once, to the resulting day.
        let first = filetime(ticks(midnight) - i64::from(period.days() - 1) * TICKS_PER_DAY);
        let mut first_local = SYSTEMTIME::default();
        let mut first_utc = SYSTEMTIME::default();
        let mut start = FILETIME::default();
        if FileTimeToSystemTime(&first, &mut first_local) == 0
            || TzSpecificLocalTimeToSystemTime(std::ptr::null(), &first_local, &mut first_utc) == 0
            || SystemTimeToFileTime(&first_utc, &mut start) == 0
        {
            bail!("Não foi possível converter o início do período.");
        }
        Ok((ticks(start).min(ticks(now)), ticks(now)))
    }
}

/// `\device\harddiskvolume3` (lowercase) → `D:` for every mapped drive letter.
fn dos_devices() -> Vec<(String, String)> {
    let mut devices = Vec::new();
    let mut target = vec![0u16; 1024];
    for letter in b'A'..=b'Z' {
        let drive = format!("{}:", letter as char);
        let name: Vec<u16> = drive.encode_utf16().chain([0]).collect();
        // SAFETY: NUL-terminated input; the output length is the buffer's length.
        let written =
            unsafe { QueryDosDeviceW(name.as_ptr(), target.as_mut_ptr(), target.len() as u32) };
        if written == 0 {
            continue;
        }
        // The first NUL-terminated string is the current mapping.
        let end = target[..written as usize]
            .iter()
            .position(|c| *c == 0)
            .unwrap_or(written as usize);
        let device = String::from_utf16_lossy(&target[..end]).to_lowercase();
        if !device.is_empty() {
            devices.push((device, drive));
        }
    }
    devices
}

/// Drive-letter path of an NT image path, if one of `devices` holds it.
fn dos_path(id: &str, devices: &[(String, String)]) -> Option<String> {
    devices.iter().find_map(|(device, drive)| {
        // Device names are ASCII; `get` refuses a cut inside a multi-byte character.
        let rest = id.get(device.len()..)?;
        (id[..device.len()].eq_ignore_ascii_case(device) && rest.starts_with('\\'))
            .then(|| format!("{drive}{rest}"))
    })
}

pub fn read(period: Period) -> Result<Vec<NetworkUsage>> {
    let (start, end) = range(period)?;
    let devices = dos_devices();
    let states = NetworkUsageStates {
        Roaming: TriStates::DoNotCare,
        Shared: TriStates::DoNotCare,
    };
    let profiles = NetworkInformation::GetConnectionProfiles()?;
    let mut networks = Vec::new();
    let mut failure = None;
    for index in 0..profiles.Size()? {
        let profile = profiles.GetAt(index)?;
        let usage = profile
            .GetAttributedNetworkUsageAsync(
                DateTime {
                    UniversalTime: start,
                },
                DateTime { UniversalTime: end },
                states,
            )
            .and_then(|operation| operation.join());
        let usage = match usage {
            Ok(usage) => usage,
            Err(error) => {
                failure = Some(error);
                continue;
            }
        };
        let mut apps = Vec::new();
        for i in 0..usage.Size()? {
            let app = usage.GetAt(i)?;
            let id = app.AttributionId()?.to_string_lossy();
            apps.push(AppBytes {
                path: dos_path(&id, &devices),
                id,
                received: app.BytesReceived()?,
                sent: app.BytesSent()?,
            });
        }
        let kind = if profile.IsWlanConnectionProfile().unwrap_or(false) {
            NetworkKind::WiFi
        } else if profile.IsWwanConnectionProfile().unwrap_or(false) {
            NetworkKind::Mobile
        } else {
            NetworkKind::Wired
        };
        networks.push(NetworkUsage {
            name: profile.ProfileName()?.to_string_lossy(),
            kind,
            apps,
        });
    }
    if networks.is_empty()
        && let Some(error) = failure
    {
        bail!("O Windows não informou o uso de dados: {}", error.message());
    }
    Ok(networks)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_paths_map_to_drive_letters_only_on_whole_device_names() {
        let devices = vec![
            ("\\device\\harddiskvolume1".to_owned(), "C:".to_owned()),
            ("\\device\\harddiskvolume12".to_owned(), "E:".to_owned()),
        ];
        assert_eq!(
            dos_path("\\device\\harddiskvolume12\\Games\\steam.exe", &devices).as_deref(),
            Some("E:\\Games\\steam.exe"),
            "volume1 é prefixo de volume12, mas não é o mesmo volume."
        );
        assert_eq!(
            dos_path("\\device\\harddiskvolume1\\windows\\a.exe", &devices).as_deref(),
            Some("C:\\windows\\a.exe")
        );
        assert_eq!(dos_path("\\device\\harddiskvolume7\\a.exe", &devices), None);
        assert_eq!(
            dos_path("Microsoft.WindowsStore_8wekyb3d8bbwe", &devices),
            None
        );
        assert_eq!(dos_path("", &devices), None);
    }

    #[test]
    fn periods_start_at_a_local_midnight_and_end_now() {
        for period in Period::ALL {
            let (start, end) = range(period).unwrap();
            let span = end - start;
            assert!(span >= 0);
            // At most the requested days plus one DST hour.
            assert!(span <= i64::from(period.days()) * TICKS_PER_DAY + TICKS_PER_DAY / 24);
            if period != Period::Today {
                assert!(span > i64::from(period.days() - 1) * TICKS_PER_DAY - TICKS_PER_DAY / 24);
            }
        }
    }
}
