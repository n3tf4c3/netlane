//! WFP dynamic sessions. No policy is installed until explicitly constructed by the elevated agent.
use anyhow::{Result, bail, ensure};
use netlane_core::{RouteMode, engine::PolicySession, planning::PlannedRule};
use std::{ffi::c_void, ptr};
use uuid::Uuid;
use windows_sys::{
    Win32::{
        Foundation::{FreeLibrary, HANDLE, HMODULE},
        NetworkManagement::WindowsFilteringPlatform::*,
        System::LibraryLoader::{GetProcAddress, LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW},
    },
    core::GUID,
};

type AddPolicy = unsafe extern "system" fn(
    HANDLE,
    *const FWPM_PROVIDER_CONTEXT3,
    FWP_IP_VERSION,
    u64,
    u32,
    *const FWPM_FILTER_CONDITION0,
    *const c_void,
) -> u32;
type DeletePolicy = unsafe extern "system" fn(HANDLE, *const GUID) -> u32;
pub enum PolicyKey {
    Route(GUID),
    Block(u64),
}
pub struct NativePolicySession {
    handle: HANDLE,
    module: HMODULE,
    add_policy: AddPolicy,
    delete_policy: DeletePolicy,
    sublayer: GUID,
}
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
fn guid() -> GUID {
    GUID::from_u128(Uuid::new_v4().as_u128())
}
fn check(code: u32) -> Result<()> {
    if code != 0 {
        bail!("WFP: 0x{code:08X} ({code}).");
    }
    Ok(())
}
impl NativePolicySession {
    pub fn open() -> Result<Self> {
        ensure!(
            std::mem::size_of::<usize>() == 8,
            "WFP exige processo de 64 bits."
        );
        let name = wide("fwpuclnt.dll");
        // SAFETY: System32-only DLL load; pointers are nul-terminated and initialized. Module is retained until Drop.
        let module =
            unsafe { LoadLibraryExW(name.as_ptr(), ptr::null_mut(), LOAD_LIBRARY_SEARCH_SYSTEM32) };
        ensure!(!module.is_null(), "API WFP indisponível.");
        // SAFETY: Static export names are nul-terminated. Missing exports are handled before invocation.
        let exports = unsafe {
            (
                GetProcAddress(module, c"FwpmConnectionPolicyAdd0".as_ptr().cast()),
                GetProcAddress(module, c"FwpmConnectionPolicyDeleteByKey0".as_ptr().cast()),
            )
        };
        let (Some(add), Some(delete)) = exports else {
            // SAFETY: We own the successfully loaded module and have not retained any function pointers.
            unsafe {
                FreeLibrary(module);
            }
            bail!("API de políticas de conexão WFP indisponível.");
        };
        // SAFETY: Exact Microsoft fwpmu.h ABI, using SDK-generated FWPM/FWP structures; exports checked above.
        let add_policy =
            unsafe { std::mem::transmute::<unsafe extern "system" fn() -> isize, AddPolicy>(add) };
        // SAFETY: Exact Microsoft connection-policy-delete ABI from fwpmu.h.
        let delete_policy = unsafe {
            std::mem::transmute::<unsafe extern "system" fn() -> isize, DeletePolicy>(delete)
        };
        let label = wide("NetLane Rust session");
        let session = FWPM_SESSION0 {
            sessionKey: guid(),
            displayData: FWPM_DISPLAY_DATA0 {
                name: label.as_ptr() as _,
                description: ptr::null_mut(),
            },
            flags: FWPM_SESSION_FLAG_DYNAMIC,
            txnWaitTimeoutInMSec: 3000,
            ..Default::default()
        };
        let mut handle = ptr::null_mut();
        // SAFETY: Session/display buffers remain live during the synchronous call; handle output is valid.
        let code = unsafe { FwpmEngineOpen0(ptr::null(), 10, ptr::null(), &session, &mut handle) };
        if let Err(e) = check(code) {
            // SAFETY: Module is owned and no session was opened.
            unsafe {
                FreeLibrary(module);
            }
            return Err(e);
        }
        let mut result = Self {
            handle,
            module,
            add_policy,
            delete_policy,
            sublayer: guid(),
        };
        let sublayer = FWPM_SUBLAYER0 {
            subLayerKey: result.sublayer,
            displayData: session.displayData,
            weight: 0x100,
            ..Default::default()
        };
        // SAFETY: Handle is a live engine session; the sublayer/display buffers remain live for this call.
        let status = unsafe { FwpmSubLayerAdd0(handle, &sublayer, ptr::null_mut()) };
        if let Err(e) = check(status) {
            let _ = result.close();
            return Err(e);
        }
        Ok(result)
    }
    fn add_family(&mut self, rule: &PlannedRule, family: u32) -> Result<PolicyKey> {
        let executable = rule
            .executable
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("Caminho do executável ausente."))?;
        let path = std::path::Path::new(executable);
        ensure!(
            path.is_absolute() && path.is_file(),
            "É necessário um executável local existente."
        );
        ensure!(
            !executable.starts_with("\\\\") && !executable.starts_with("\\\\?\\"),
            "Caminho de executável remoto/dispositivo recusado."
        );
        let encoded = wide(executable);
        let mut blob = ptr::null_mut();
        // SAFETY: Encoded path is nul-terminated; output blob is freed exactly once below.
        check(unsafe { FwpmGetAppIdFromFileName0(encoded.as_ptr(), &mut blob) })?;
        let result = (|| {
            ensure!(!blob.is_null(), "WFP retornou AppId vazio.");
            let app = FWPM_FILTER_CONDITION0 {
                fieldKey: FWPM_CONDITION_ALE_APP_ID,
                matchType: FWP_MATCH_EQUAL,
                conditionValue: FWP_CONDITION_VALUE0 {
                    r#type: FWP_BYTE_BLOB_TYPE,
                    Anonymous: FWP_CONDITION_VALUE0_0 { byteBlob: blob },
                },
            };
            if rule.mode == RouteMode::Blocked {
                let mut condition = app;
                let filter = FWPM_FILTER0 {
                    filterKey: guid(),
                    layerKey: if family == 0 {
                        FWPM_LAYER_ALE_AUTH_CONNECT_V4
                    } else {
                        FWPM_LAYER_ALE_AUTH_CONNECT_V6
                    },
                    subLayerKey: self.sublayer,
                    numFilterConditions: 1,
                    filterCondition: &mut condition,
                    action: FWPM_ACTION0 {
                        r#type: FWP_ACTION_BLOCK,
                        ..Default::default()
                    },
                    ..Default::default()
                };
                let mut id = 0;
                // SAFETY: All filter pointers and the AppId blob stay live until the synchronous call returns.
                check(unsafe { FwpmFilterAdd0(self.handle, &filter, ptr::null_mut(), &mut id) })?;
                return Ok(PolicyKey::Block(id));
            }
            ensure!(rule.luid != 0, "Interface sem LUID válido.");
            let mut luid = rule.luid;
            let mut setting = FWPM_NETWORK_CONNECTION_POLICY_SETTING0 {
                r#type: FWP_NETWORK_CONNECTION_POLICY_NEXT_HOP_INTERFACE,
                value: FWP_VALUE0 {
                    r#type: FWP_UINT64,
                    Anonymous: FWP_VALUE0_0 { uint64: &mut luid },
                },
            };
            let mut settings = FWPM_NETWORK_CONNECTION_POLICY_SETTINGS0 {
                numSettings: 1,
                settings: &mut setting,
            };
            let key = guid();
            let context = FWPM_PROVIDER_CONTEXT3 {
                providerContextKey: key,
                r#type: FWPM_NETWORK_CONNECTION_POLICY_CONTEXT,
                Anonymous: FWPM_PROVIDER_CONTEXT3_0 {
                    networkConnectionPolicy: &mut settings,
                },
                ..Default::default()
            };
            let mut v4 = FWP_V4_ADDR_AND_MASK {
                addr: 0x7f000000,
                mask: 0xff000000,
            };
            let mut v6 = FWP_V6_ADDR_AND_MASK {
                addr: [0; 16],
                prefixLength: 128,
            };
            v6.addr[15] = 1;
            let loopback = FWPM_FILTER_CONDITION0 {
                fieldKey: FWPM_CONDITION_IP_REMOTE_ADDRESS,
                matchType: FWP_MATCH_NOT_EQUAL,
                conditionValue: if family == 0 {
                    FWP_CONDITION_VALUE0 {
                        r#type: FWP_V4_ADDR_MASK,
                        Anonymous: FWP_CONDITION_VALUE0_0 {
                            v4AddrMask: &mut v4,
                        },
                    }
                } else {
                    FWP_CONDITION_VALUE0 {
                        r#type: FWP_V6_ADDR_MASK,
                        Anonymous: FWP_CONDITION_VALUE0_0 {
                            v6AddrMask: &mut v6,
                        },
                    }
                },
            };
            let conditions = [
                app,
                loopback,
                FWPM_FILTER_CONDITION0 {
                    fieldKey: FWPM_CONDITION_FLAGS,
                    matchType: FWP_MATCH_FLAGS_NONE_SET,
                    conditionValue: FWP_CONDITION_VALUE0 {
                        r#type: FWP_UINT32,
                        Anonymous: FWP_CONDITION_VALUE0_0 { uint32: 1 },
                    },
                },
                FWPM_FILTER_CONDITION0 {
                    fieldKey: FWPM_CONDITION_IP_DESTINATION_ADDRESS_TYPE,
                    matchType: FWP_MATCH_EQUAL,
                    conditionValue: FWP_CONDITION_VALUE0 {
                        r#type: FWP_UINT8,
                        Anonymous: FWP_CONDITION_VALUE0_0 { uint8: 1 },
                    },
                },
            ];
            // SAFETY: fwpmu.h ABI; every nested pointer (including FWP_UINT64) references initialized stack data for the entire call.
            check(unsafe {
                (self.add_policy)(
                    self.handle,
                    &context,
                    family as i32,
                    0x10000000,
                    conditions.len() as u32,
                    conditions.as_ptr(),
                    ptr::null(),
                )
            })?;
            Ok(PolicyKey::Route(key))
        })();
        // SAFETY: Win32 owns the AppId allocation; FwpmFreeMemory0 receives its address exactly once.
        unsafe {
            FwpmFreeMemory0((&mut blob as *mut *mut FWP_BYTE_BLOB).cast());
        }
        result
    }
}
impl PolicySession for NativePolicySession {
    type Key = PolicyKey;
    fn begin(&mut self) -> Result<()> {
        ensure!(!self.handle.is_null(), "Sessão WFP encerrada.");
        // SAFETY: Only a live engine handle reaches this call.
        check(unsafe { FwpmTransactionBegin0(self.handle, 0) })
    }
    fn add(&mut self, rule: &PlannedRule) -> Result<Vec<PolicyKey>> {
        let mut keys = Vec::new();
        for family in 0..=1 {
            if rule.mode == RouteMode::Blocked
                || (family == 0 && rule.ipv4)
                || (family == 1 && rule.ipv6)
            {
                keys.push(self.add_family(rule, family)?);
            }
        }
        ensure!(!keys.is_empty(), "Interface sem IPv4/IPv6 habilitado.");
        Ok(keys)
    }
    fn delete(&mut self, key: &PolicyKey) -> Result<()> {
        // SAFETY: Keys are returned by this session only; the engine is live within a transaction.
        check(unsafe {
            match key {
                PolicyKey::Route(k) => (self.delete_policy)(self.handle, k),
                PolicyKey::Block(id) => FwpmFilterDeleteById0(self.handle, *id),
            }
        })
    }
    fn commit(&mut self) -> Result<()> {
        // SAFETY: Caller begins a transaction before committing.
        check(unsafe { FwpmTransactionCommit0(self.handle) })
    }
    fn abort(&mut self) -> Result<()> {
        // SAFETY: Caller only aborts after a failed operation in a transaction.
        check(unsafe { FwpmTransactionAbort0(self.handle) })
    }
    fn close(&mut self) -> Result<()> {
        if self.handle.is_null() {
            return Ok(());
        }
        // SAFETY: We own the handle. Dynamic-session close removes its policies, including on error/crash.
        let result = check(unsafe { FwpmEngineClose0(self.handle) });
        if result.is_ok() {
            self.handle = ptr::null_mut();
        }
        result
    }
}
impl Drop for NativePolicySession {
    fn drop(&mut self) {
        let _ = self.close();
        // SAFETY: No stored function pointer can be invoked after this instance is dropped.
        unsafe {
            FreeLibrary(self.module);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sdk_abi_preserves_pointer_valued_uint64() {
        assert_eq!(std::mem::size_of::<FWP_VALUE0>(), 16);
        assert_eq!(std::mem::offset_of!(FWP_VALUE0, Anonymous), 8);
        assert_eq!(std::mem::size_of::<FWPM_PROVIDER_CONTEXT3>(), 88);
        let mut luid = 123u64;
        let v = FWP_VALUE0 {
            r#type: FWP_UINT64,
            Anonymous: FWP_VALUE0_0 { uint64: &mut luid },
        };
        // SAFETY: Active union arm is uint64, initialized to the live luid above.
        assert_eq!(unsafe { *v.Anonymous.uint64 }, 123);
    }
}
