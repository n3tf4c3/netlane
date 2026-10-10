!ifndef NETLANE_RUST_PROCESS_GUARD
!define NETLANE_RUST_PROCESS_GUARD
; Toolhelp read-only, across user sessions. Never terminate/suspend a process.
Var GuardResult
!macro DefineRustProcessGuard Prefix
Function ${Prefix}CheckRustProcesses
  System::Store "S"
  StrCpy $GuardResult 3
  System::Call 'kernel32::CreateToolhelp32Snapshot(i 2, i 0) p.r0'
  ${If} $0 == -1
    Goto guard_done
  ${EndIf}
  System::Alloc 556
  Pop $1
  ${If} $1 == 0
    Goto guard_close
  ${EndIf}
  System::Call '*$1(i 556)'
  System::Call 'kernel32::Process32FirstW(p r0, p r1) i.r2'
  ${If} $2 == 0
    Goto guard_free
  ${EndIf}
guard_loop:
  IntOp $3 $1 + 36
  System::Call '*$3(&w260 .r4)'
  ${If} $4 == "netlane.exe"
  ${OrIf} $4 == "netlane-agent.exe"
    StrCpy $GuardResult 2
    Goto guard_free
  ${EndIf}
  ; Also refuse concurrent legacy sessions; they share session/default-connection leases.
  StrCpy $4 $4 8
  ${If} $4 == "NetLane."
    StrCpy $GuardResult 2
    Goto guard_free
  ${EndIf}
  System::Call 'kernel32::Process32NextW(p r0, p r1) i.r2 ?e'
  Pop $5
  ${If} $2 != 0
    Goto guard_loop
  ${EndIf}
  ${If} $5 == 18
    StrCpy $GuardResult 0
  ${EndIf}
guard_free:
  System::Free $1
guard_close:
  System::Call 'kernel32::CloseHandle(p r0)'
guard_done:
  System::Store "L"
FunctionEnd
!macroend
!macro RequireRustClosed Prefix
  Call ${Prefix}CheckRustProcesses
  ${If} $GuardResult != 0
    MessageBox MB_OK|MB_ICONEXCLAMATION "Feche o NetLane normalmente, inclusive a bandeja, e aguarde a limpeza do agente. Nada será encerrado à força." /SD IDOK
    SetErrorLevel $GuardResult
    Abort
  ${EndIf}
!macroend
!endif
