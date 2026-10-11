//! Local authenticated pipes across the UAC boundary; OS PID and creation-time checks.
use anyhow::{Result, ensure};
use std::{
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
    path::Path,
    ptr,
};
use tokio::net::windows::named_pipe::{
    ClientOptions, NamedPipeClient, NamedPipeServer, ServerOptions,
};
use windows_sys::Win32::{
    Foundation::{CloseHandle, FILETIME, HANDLE, LocalFree, WAIT_TIMEOUT},
    Security::{
        Authorization::{
            ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
        },
        GetTokenInformation, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER, TokenUser,
    },
    System::{
        Pipes::{GetNamedPipeClientProcessId, GetNamedPipeServerProcessId},
        Threading::{
            GetCurrentProcess, GetProcessId, GetProcessTimes, OpenProcess, OpenProcessToken,
            PROCESS_QUERY_LIMITED_INFORMATION, WaitForSingleObject,
        },
    },
    UI::{
        Shell::{SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW},
        WindowsAndMessaging::SW_HIDE,
    },
};
pub const PREFIX: &str = "NetLane.Rust.UI.";
pub fn valid_name(name: &str) -> bool {
    name.strip_prefix(PREFIX)
        .is_some_and(|s| uuid::Uuid::parse_str(s).is_ok() && s.len() == 32)
}
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
fn user_sid() -> Result<String> {
    let mut token = ptr::null_mut();
    // SAFETY: Token query is read-only and output pointers are initialized. Token is closed on all paths below.
    ensure!(
        // SAFETY: Current process token query with a valid output pointer.
        unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } != 0,
        "Token do usuário indisponível."
    );
    let result = (|| {
        let mut size = 0;
        // SAFETY: First call queries size only; second has aligned storage of that size.
        unsafe {
            GetTokenInformation(token, TokenUser, ptr::null_mut(), 0, &mut size);
        }
        let mut data = vec![0u64; (size as usize).div_ceil(8)];
        // SAFETY: Sized/aligned buffer with TOKEN_USER layout selected above.
        ensure!(
            // SAFETY: The aligned buffer has the size returned by the preceding query.
            unsafe {
                GetTokenInformation(token, TokenUser, data.as_mut_ptr().cast(), size, &mut size)
            } != 0,
            "SID do usuário indisponível."
        );
        let mut string = ptr::null_mut();
        // SAFETY: TOKEN_USER's SID points into the live token-information allocation.
        ensure!(
            // SAFETY: SID lies within the initialized TOKEN_USER allocation above.
            unsafe {
                ConvertSidToStringSidW((*data.as_ptr().cast::<TOKEN_USER>()).User.Sid, &mut string)
            } != 0,
            "Conversão de SID falhou."
        );
        // SAFETY: Windows owns a nul-terminated UTF-16 allocation, freed once after copying.
        let sid = unsafe {
            let mut len = 0;
            while *string.add(len) != 0 {
                len += 1;
            }
            let sid = String::from_utf16_lossy(std::slice::from_raw_parts(string, len));
            LocalFree(string.cast());
            sid
        };
        Ok(sid)
    })();
    // SAFETY: Owned token is no longer used by result.
    unsafe {
        CloseHandle(token);
    }
    result
}
pub fn create_pipe(name: &str) -> Result<NamedPipeServer> {
    ensure!(
        valid_name(name)
            || matches!(
                name,
                "NetLane.RoutingSession.v1" | "NetLane.DefaultConnection.v1"
            ),
        "Nome de canal inválido."
    );
    let path = format!("\\\\.\\pipe\\{name}");
    let sddl = wide(&format!(
        "D:P(D;;GA;;;NU)(A;;GA;;;{})(A;;GA;;;BA)",
        user_sid()?
    ));
    let mut descriptor = ptr::null_mut();
    // SAFETY: SDDL contains the current OS SID and fixed ACL text; Windows allocates descriptor.
    ensure!(
        // SAFETY: Nul-terminated SDDL with valid descriptor output storage.
        unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                1,
                &mut descriptor,
                ptr::null_mut(),
            )
        } != 0,
        "ACL do canal inválida."
    );
    let attributes = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor,
        bInheritHandle: 0,
    };
    // SAFETY: Descriptor and SECURITY_ATTRIBUTES remain live during synchronous pipe creation. Clients are local only.
    let result = unsafe {
        ServerOptions::new()
            .first_pipe_instance(true)
            .reject_remote_clients(true)
            .create_with_security_attributes_raw(
                path,
                (&attributes as *const SECURITY_ATTRIBUTES)
                    .cast_mut()
                    .cast(),
            )
    };
    // SAFETY: CreateNamedPipe copied the descriptor into the kernel object; free the source exactly once.
    unsafe {
        LocalFree(descriptor);
    }
    Ok(result?)
}
pub fn connect_pipe(name: &str) -> Result<NamedPipeClient> {
    ensure!(valid_name(name), "Nome de canal inválido.");
    // SECURITY_ANONYMOUS prevents an unelevated server impersonating the elevated client.
    Ok(ClientOptions::new()
        .security_qos_flags(0x00100000)
        .open(format!("\\\\.\\pipe\\{name}"))?)
}
pub fn verify_client(pipe: &NamedPipeServer, expected: u32) -> Result<()> {
    let mut pid = 0;
    // SAFETY: Tokio owns a live pipe handle; output PID is initialized.
    ensure!(
        // SAFETY: Tokio keeps this server pipe handle live for the call.
        unsafe { GetNamedPipeClientProcessId(pipe.as_raw_handle(), &mut pid) } != 0
            && pid == expected,
        "Cliente do canal não corresponde ao processo iniciado."
    );
    Ok(())
}
pub fn verify_server(pipe: &NamedPipeClient, expected: u32) -> Result<()> {
    let mut pid = 0;
    // SAFETY: Tokio owns a live pipe handle; output PID is initialized.
    ensure!(
        // SAFETY: Tokio keeps this client pipe handle live for the call.
        unsafe { GetNamedPipeServerProcessId(pipe.as_raw_handle(), &mut pid) } != 0
            && pid == expected,
        "Servidor do canal não corresponde ao painel."
    );
    Ok(())
}
fn process_start(handle: HANDLE) -> Result<u64> {
    let mut created = FILETIME::default();
    let mut exited = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    // SAFETY: Handle must grant query information; each FILETIME output lives until return.
    ensure!(
        // SAFETY: Private callers provide a live query-capable process handle.
        unsafe { GetProcessTimes(handle, &mut created, &mut exited, &mut kernel, &mut user) } != 0,
        "Identidade do processo indisponível."
    );
    Ok(((created.dwHighDateTime as u64) << 32) | created.dwLowDateTime as u64)
}
pub fn current_start() -> Result<u64> {
    // SAFETY: Borrowed pseudo-handle for the current process; never closed.
    process_start(unsafe { GetCurrentProcess() })
}
pub fn open_owner(pid: u32, expected_start: u64) -> Result<OwnedHandle> {
    // SAFETY: Query-only/synchronize access to exact PID; no process is modified.
    let handle = unsafe {
        OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | 0x00100000, /* SYNCHRONIZE */
            0,
            pid,
        )
    };
    ensure!(!handle.is_null(), "Painel proprietário indisponível.");
    // SAFETY: Successful OpenProcess transfers ownership to this RAII wrapper.
    let owned = unsafe { OwnedHandle::from_raw_handle(handle) };
    ensure!(
        process_start(handle)? == expected_start && running(&owned),
        "Identidade do painel mudou."
    );
    Ok(owned)
}
pub fn running(process: &OwnedHandle) -> bool {
    // SAFETY: Owned process handle remains valid for the nonblocking wait.
    unsafe { WaitForSingleObject(process.as_raw_handle(), 0) == WAIT_TIMEOUT }
}
pub fn exit_code(process: &OwnedHandle) -> Result<u32> {
    let mut code = 0;
    // SAFETY: Query-only owned process handle and initialized output pointer.
    ensure!(
        // SAFETY: Owned process handle and initialized DWORD output remain live.
        unsafe {
            windows_sys::Win32::System::Threading::GetExitCodeProcess(
                process.as_raw_handle(),
                &mut code,
            )
        } != 0,
        "Saída do componente não confirmada."
    );
    Ok(code)
}
pub fn require_elevation() -> Result<()> {
    use windows_sys::Win32::Security::{TOKEN_ELEVATION, TokenElevation};
    let mut token = ptr::null_mut();
    // SAFETY: Query-only process token and initialized pointer.
    ensure!(
        // SAFETY: Query-only process token with valid output storage.
        unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } != 0,
        "Token indisponível."
    );
    // SAFETY: Successful token open transfers ownership to this wrapper.
    let owned = unsafe { OwnedHandle::from_raw_handle(token) };
    let mut info = TOKEN_ELEVATION { TokenIsElevated: 0 };
    let mut size = 0;
    // SAFETY: TOKEN_ELEVATION buffer is correctly sized and aligned; owned token remains live.
    let ok = unsafe {
        GetTokenInformation(
            owned.as_raw_handle(),
            TokenElevation,
            (&mut info as *mut TOKEN_ELEVATION).cast(),
            std::mem::size_of_val(&info) as u32,
            &mut size,
        )
    };
    ensure!(
        ok != 0 && info.TokenIsElevated != 0,
        "O comando exige autorização do Windows."
    );
    Ok(())
}

/// First call of the elevated agent: later LoadLibrary calls search System32 only,
/// so a DLL planted beside a user-writable executable is never loaded elevated.
pub fn harden_dll_search() -> Result<()> {
    use windows_sys::Win32::System::LibraryLoader::{
        LOAD_LIBRARY_SEARCH_SYSTEM32, SetDefaultDllDirectories,
    };
    // SAFETY: Process-wide flag with a documented constant; no pointers involved.
    ensure!(
        // SAFETY: Documented flag value only; the call reads no caller memory.
        unsafe { SetDefaultDllDirectories(LOAD_LIBRARY_SEARCH_SYSTEM32) } != 0,
        "Não foi possível restringir a busca de DLLs: {}",
        std::io::Error::last_os_error()
    );
    Ok(())
}
pub fn launch_agent(executable: &Path, name: &str) -> Result<(OwnedHandle, u32)> {
    launch(executable, name, "--ui-session")
}
pub fn launch_default_agent(executable: &Path, name: &str) -> Result<(OwnedHandle, u32)> {
    launch(executable, name, "--default-connection-session")
}
fn launch(executable: &Path, name: &str, command: &str) -> Result<(OwnedHandle, u32)> {
    ensure!(valid_name(name), "Nome de canal inválido.");
    ensure!(
        executable.is_absolute() && executable.is_file(),
        "Compile netlane-agent antes de iniciar."
    );
    let arguments = wide(&format!(
        "{command} {name} {} {}",
        std::process::id(),
        current_start()?
    ));
    let file: Vec<_> = executable
        .as_os_str()
        .to_string_lossy()
        .encode_utf16()
        .chain(Some(0))
        .collect();
    let verb = wide("runas");
    let mut shell = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS,
        lpVerb: verb.as_ptr(),
        lpFile: file.as_ptr(),
        lpParameters: arguments.as_ptr(),
        nShow: SW_HIDE,
        ..Default::default()
    };
    // SAFETY: All ShellExecute buffers remain live through the synchronous UAC request. Process handle is adopted below.
    ensure!(
        // SAFETY: Initialized ShellExecuteInfo and all UTF-16 buffers remain live.
        unsafe { ShellExecuteExW(&mut shell) } != 0,
        "Autorização cancelada ou agente não iniciado: {}",
        std::io::Error::last_os_error()
    );
    ensure!(
        !shell.hProcess.is_null(),
        "Windows não retornou a identidade do agente."
    );
    // SAFETY: SEE_MASK_NOCLOSEPROCESS transfers the returned handle to the caller.
    let process = unsafe { OwnedHandle::from_raw_handle(shell.hProcess) };
    // SAFETY: Successfully opened process handle grants PID query.
    let pid = unsafe { GetProcessId(process.as_raw_handle()) };
    ensure!(pid != 0, "PID do agente indisponível.");
    Ok((process, pid))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn private_pipe_authenticates_real_peers_and_refuses_a_second_server() {
        // Test-only private pipe; no routing lease, agent, elevation or WFP engine.
        let name = format!("{PREFIX}{}", uuid::Uuid::new_v4().simple());
        let server = create_pipe(&name).unwrap();
        assert!(create_pipe(&name).is_err());
        let client = connect_pipe(&name).unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(2), server.connect())
            .await
            .unwrap()
            .unwrap();
        let pid = std::process::id();
        verify_client(&server, pid).unwrap();
        verify_server(&client, pid).unwrap();
        assert!(verify_client(&server, 0).is_err());
        assert!(verify_server(&client, 0).is_err());
        server.disconnect().unwrap();
        drop(client);
        drop(server);
        // Tokio's I/O driver drains deferred handle releases on its next poll.
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        create_pipe(&name).unwrap();
    }

    #[test]
    fn owner_requires_pid_and_creation_time_and_disallows_other_pipe_names() {
        let start = current_start().unwrap();
        let owner = open_owner(std::process::id(), start).unwrap();
        assert!(running(&owner));
        assert!(open_owner(std::process::id(), start + 1).is_err());
        for name in [
            "OtherProduct",
            "NetLane.Rust.UI.invalid",
            "\\\\remote\\pipe\\x",
        ] {
            assert!(create_pipe(name).is_err());
            assert!(connect_pipe(name).is_err());
        }
    }
}
