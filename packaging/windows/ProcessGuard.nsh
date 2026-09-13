!ifndef NETLANE_PROCESS_GUARD
!define NETLANE_PROCESS_GUARD

; Read-only Toolhelp snapshot, including processes in other user sessions.
; NSIS uses a 32-bit Unicode stub: PROCESSENTRY32W is 556 bytes, name at offset 36.
; No process handles with terminate/suspend rights, messages, or service actions.
; Returns 0 (clear), 2 (NetLane process present), 3 (cannot verify) in $GuardResult.
Var GuardResult
!macro DefineProcessGuard Prefix
Function ${Prefix}CheckNetLaneProcesses
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
  ${If} $5 == 18 ; ERROR_NO_MORE_FILES: complete, not a failed/partial enumeration.
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

!macro RequireNetLaneClosed Prefix
  Call ${Prefix}CheckNetLaneProcesses
  ${If} $GuardResult != 0
    MessageBox MB_OK|MB_ICONEXCLAMATION "Feche o NetLane normalmente antes de continuar, inclusive o ícone na bandeja. Aguarde a parada do serviço. Não foi possível confirmar que todos os processos estão encerrados; nada será encerrado à força." /SD IDOK
    SetErrorLevel $GuardResult
    Abort
  ${EndIf}
!macroend
!endif
