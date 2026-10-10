use anyhow::Result;
use netlane_core::NetworkSnapshot;

#[cfg(windows)]
pub fn snapshot() -> Result<NetworkSnapshot> {
    native::snapshot()
}
#[cfg(not(windows))]
pub fn snapshot() -> Result<NetworkSnapshot> {
    anyhow::bail!("A observação de rede exige Windows.")
}

#[cfg(windows)]
mod native {
    use anyhow::{Result, bail};
    use netlane_core::{Adapter, Connection, NetworkSnapshot, file_name};
    use std::{
        ffi::CStr,
        net::{Ipv4Addr, Ipv6Addr},
        ptr,
    };
    use windows_sys::Win32::{
        Foundation::{CloseHandle, FILETIME},
        NetworkManagement::{IpHelper::*, Ndis::NET_LUID_LH},
        Networking::WinSock::{AF_INET, AF_INET6, SOCKADDR_IN, SOCKADDR_IN6},
        System::Threading::{
            GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
            QueryFullProcessImageNameW,
        },
    };

    pub fn snapshot() -> Result<NetworkSnapshot> {
        let adapters = adapters()?;
        let connections = connections(&adapters)?;
        Ok(NetworkSnapshot {
            adapters,
            connections,
            error: None,
        })
    }
    fn check(status: u32) -> Result<()> {
        if status != 0 {
            bail!(
                "Consulta Win32: {}",
                std::io::Error::from_raw_os_error(status as i32)
            );
        }
        Ok(())
    }
    fn wide_string(p: *const u16) -> String {
        if p.is_null() {
            return String::new();
        }
        // SAFETY: Win32 returns nul-terminated UTF-16 within the still-owned adapter buffer.
        unsafe {
            let mut len = 0;
            while *p.add(len) != 0 && len < 32768 {
                len += 1;
            }
            String::from_utf16_lossy(std::slice::from_raw_parts(p, len))
        }
    }
    pub fn adapters() -> Result<Vec<Adapter>> {
        let mut bytes = 16384u32;
        let mut storage;
        for _ in 0..3 {
            if bytes > 16 * 1024 * 1024 {
                bail!("Tabela de interfaces muito grande.");
            }
            storage = vec![0u64; (bytes as usize).div_ceil(8)];
            // SAFETY: u64 storage satisfies the alignment; size is the allocated capacity, and reserved is null.
            let status = unsafe {
                GetAdaptersAddresses(
                    0,
                    GAA_FLAG_INCLUDE_GATEWAYS,
                    ptr::null(),
                    storage.as_mut_ptr().cast(),
                    &mut bytes,
                )
            };
            if status == 111 {
                continue;
            } // ERROR_BUFFER_OVERFLOW
            check(status)?;
            let mut result = Vec::new();
            let mut p = storage.as_ptr().cast::<IP_ADAPTER_ADDRESSES_LH>();
            // SAFETY: Successful GetAdaptersAddresses links initialized records in storage; all backing storage stays live.
            unsafe {
                while !p.is_null() {
                    let a = &*p;
                    if a.IfType != 24
                    /* loopback */
                    {
                        let id = CStr::from_ptr(a.AdapterName.cast())
                            .to_string_lossy()
                            .trim_matches(['{', '}'])
                            .to_owned();
                        let mut addresses = Vec::new();
                        let mut unicast = a.FirstUnicastAddress;
                        while !unicast.is_null() {
                            let address = &(*unicast).Address;
                            if !address.lpSockaddr.is_null() {
                                match (*address.lpSockaddr).sa_family {
                                    AF_INET => {
                                        let s = &*address.lpSockaddr.cast::<SOCKADDR_IN>();
                                        addresses.push(
                                            Ipv4Addr::from(s.sin_addr.S_un.S_addr.to_ne_bytes())
                                                .to_string(),
                                        );
                                    }
                                    AF_INET6 => {
                                        let s = &*address.lpSockaddr.cast::<SOCKADDR_IN6>();
                                        addresses
                                            .push(Ipv6Addr::from(s.sin6_addr.u.Byte).to_string());
                                    }
                                    _ => {}
                                }
                            }
                            unicast = (*unicast).Next;
                        }
                        let luid = a.Luid.Value;
                        let mut counters = MIB_IF_ROW2 {
                            InterfaceLuid: NET_LUID_LH { Value: luid },
                            ..Default::default()
                        };
                        let available = GetIfEntry2(&mut counters) == 0;
                        let flags = a.Anonymous2.Flags;
                        result.push(Adapter {
                            id,
                            name: wide_string(a.FriendlyName),
                            kind: match a.IfType {
                                6 => "Ethernet",
                                71 => "Wireless80211",
                                _ => "Virtual/Other",
                            }
                            .to_owned(),
                            addresses,
                            connected: a.OperStatus == 1,
                            has_gateway: !a.FirstGatewayAddress.is_null(),
                            luid,
                            ipv4: flags & 0x80 != 0,
                            ipv6: flags & 0x100 != 0,
                            received: available.then_some(counters.InOctets),
                            sent: available.then_some(counters.OutOctets),
                        });
                    }
                    p = a.Next;
                }
            }
            result.sort_by_key(|a| a.name.to_lowercase());
            return Ok(result);
        }
        bail!("As interfaces mudaram durante a coleta; tente novamente.")
    }
    fn identity(pid: u32, captured: u64) -> (String, Option<String>) {
        // SAFETY: Read-only handle to the exact PID; all output buffers have the declared lengths and the handle is closed.
        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if handle.is_null() {
                return (format!("PID {pid}"), None);
            }
            let mut created = FILETIME::default();
            let mut exited = FILETIME::default();
            let mut kernel = FILETIME::default();
            let mut user = FILETIME::default();
            let alive = GetProcessTimes(handle, &mut created, &mut exited, &mut kernel, &mut user)
                != 0
                && exited.dwHighDateTime == 0
                && exited.dwLowDateTime == 0
                && (((created.dwHighDateTime as u64) << 32) | created.dwLowDateTime as u64)
                    <= captured;
            let mut path = vec![0u16; 32768];
            let mut length = path.len() as u32;
            let ok =
                alive && QueryFullProcessImageNameW(handle, 0, path.as_mut_ptr(), &mut length) != 0;
            CloseHandle(handle);
            if ok {
                let path = String::from_utf16_lossy(&path[..length as usize]);
                (file_name(&path).to_owned(), Some(path))
            } else {
                (format!("PID {pid}"), None)
            }
        }
    }
    fn connections(adapters: &[Adapter]) -> Result<Vec<Connection>> {
        let captured = (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
            / 100) as u64
            + 116_444_736_000_000_000;
        let mut bytes = 0;
        // SAFETY: The first call only queries the required length using a null buffer.
        let status = unsafe {
            GetExtendedTcpTable(
                ptr::null_mut(),
                &mut bytes,
                0,
                AF_INET as u32,
                TCP_TABLE_OWNER_PID_ALL,
                0,
            )
        };
        if status != 122 && status != 0 {
            check(status)?;
        }
        for _ in 0..3 {
            if bytes > 64 * 1024 * 1024 {
                bail!("Tabela de conexões muito grande.");
            }
            let mut storage = vec![0u64; (bytes as usize).div_ceil(8).max(1)];
            // SAFETY: Storage is sufficiently aligned and large; table class/family determine the row layout below.
            let status = unsafe {
                GetExtendedTcpTable(
                    storage.as_mut_ptr().cast(),
                    &mut bytes,
                    0,
                    AF_INET as u32,
                    TCP_TABLE_OWNER_PID_ALL,
                    0,
                )
            };
            if status == 122 {
                continue;
            }
            check(status)?;
            let data = storage.as_ptr().cast::<u8>();
            // SAFETY: Success initializes the row count; rows are bounded against the returned buffer length before reading.
            let count = unsafe { ptr::read_unaligned(data.cast::<u32>()) as usize };
            let row_size = std::mem::size_of::<MIB_TCPROW_OWNER_PID>();
            if count.saturating_mul(row_size).saturating_add(4) > bytes as usize {
                bail!("Tabela TCP inconsistente.");
            }
            let mut identities = std::collections::HashMap::new();
            let mut result = Vec::new();
            for i in 0..count {
                // SAFETY: Row range was checked and read_unaligned handles the DWORD table header.
                let row = unsafe {
                    ptr::read_unaligned(data.add(4 + i * row_size).cast::<MIB_TCPROW_OWNER_PID>())
                };
                if row.dwState != 5 || row.dwOwningPid == 0 {
                    continue;
                }
                let local = Ipv4Addr::from(row.dwLocalAddr.to_ne_bytes());
                let remote = Ipv4Addr::from(row.dwRemoteAddr.to_ne_bytes());
                let local_port = u16::from_be(row.dwLocalPort as u16);
                let remote_port = u16::from_be(row.dwRemotePort as u16);
                if local.is_unspecified()
                    || local.is_loopback()
                    || local.is_broadcast()
                    || remote.is_unspecified()
                    || remote.is_loopback()
                    || remote.is_broadcast()
                    || local_port == 0
                    || remote_port == 0
                {
                    continue;
                }
                let local_address = local.to_string();
                let (name, executable) = identities
                    .entry(row.dwOwningPid)
                    .or_insert_with(|| identity(row.dwOwningPid, captured))
                    .clone();
                let matching: Vec<_> = adapters
                    .iter()
                    .filter(|a| a.connected && a.addresses.contains(&local_address))
                    .collect();
                let interface = if matching.len() == 1 {
                    Some(matching[0].name.clone())
                } else {
                    None
                };
                result.push(Connection {
                    pid: row.dwOwningPid,
                    name,
                    executable,
                    local_address,
                    local_port,
                    remote_address: remote.to_string(),
                    remote_port,
                    interface,
                });
            }
            result.sort_by_key(|c| {
                (
                    c.name.to_lowercase(),
                    c.pid,
                    c.remote_address.clone(),
                    c.remote_port,
                )
            });
            return Ok(result);
        }
        bail!("As conexões mudaram durante a coleta; tente novamente.")
    }
}
