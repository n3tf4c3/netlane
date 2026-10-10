Unicode true
ManifestDPIAware true
; Per-user installation: HKCU startup and uninstall belong to the same user.
; Elevation remains an explicit action of the routing agent inside the panel.
RequestExecutionLevel user
SetCompressor /SOLID zlib
SetOverwrite on
!include "MUI2.nsh"
!include "LogicLib.nsh"
!include "x64.nsh"
!include "WinVer.nsh"
!include "WordFunc.nsh"
!include "FileFunc.nsh"
!include "RustProcessGuard.nsh"
!include "${BUILD_DEFINES}"
!define PRODUCT_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\NetLaneRustPreview"
!define PRODUCT_DIR "$LOCALAPPDATA\Programs\NetLane Rust"
Name "NetLane Rust (prévia local)"
OutFile "${SETUP_OUTPUT}"
InstallDir "${PRODUCT_DIR}"
VIProductVersion "${PRODUCT_VERSION}.0"
VIAddVersionKey /LANG=1046 "ProductName" "NetLane Rust"
VIAddVersionKey /LANG=1046 "FileDescription" "Prévia local Rust do NetLane"
VIAddVersionKey /LANG=1046 "FileVersion" "${PRODUCT_VERSION}.0"
VIAddVersionKey /LANG=1046 "LegalCopyright" "NetLane contributors"
!define MUI_WELCOMEPAGE_TITLE "NetLane Rust — prévia local"
!define MUI_WELCOMEPAGE_TEXT "Instalação para este usuário, em uma pasta própria.$\r$\n$\r$\nFeche o NetLane e aguarde a limpeza do agente antes de continuar.$\r$\n$\r$\nAs regras ficam no perfil NetLane-Rust. A versão anterior permanece disponível. O painel começa em observação; a seleção de saída e as medições são ações manuais."
!define MUI_FINISHPAGE_TEXT "Abra NetLane Rust pelo menu Iniciar quando desejar.$\r$\n$\r$\nA opção Abrir com o Windows começa desligada e pode ser alterada nas Configurações."
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "PortugueseBR"
!insertmacro DefineRustProcessGuard ""
!insertmacro DefineRustProcessGuard "un."
Var SafePath

!macro CheckManagedDirectory Path
  System::Call 'kernel32::GetFileAttributesW(w "${Path}") i.r0 ?e'
  Pop $3
  ${If} $0 == -1
    ${If} $3 != 2
    ${AndIf} $3 != 3
      Goto unsafe_destination
    ${EndIf}
  ${Else}
    IntOp $1 $0 & 0x400
    IntOp $2 $0 & 0x10
    ${If} $1 != 0
    ${OrIf} $2 == 0
      Goto unsafe_destination
    ${EndIf}
  ${EndIf}
!macroend
!macro CheckManagedFile Path
  System::Call 'kernel32::GetFileAttributesW(w "${Path}") i.r0 ?e'
  Pop $3
  ${If} $0 == -1
    ${If} $3 != 2
    ${AndIf} $3 != 3
      Goto unsafe_destination
    ${EndIf}
  ${Else}
    IntOp $1 $0 & 0x410
    ${If} $1 != 0
      Goto unsafe_destination
    ${EndIf}
    System::Call 'kernel32::CreateFileW(w "${Path}", i 0xC0000000, i 0, p 0, i 3, i 0x80, p 0) p.r0'
    ${If} $0 == -1
      Goto file_in_use
    ${EndIf}
    System::Call 'kernel32::CloseHandle(p r0)'
  ${EndIf}
!macroend
!macro CheckPayloadDirectory Relative
  !insertmacro CheckManagedDirectory "$INSTDIR\${Relative}"
!macroend
!macro CheckPayloadFile Relative
  !insertmacro CheckManagedFile "$INSTDIR\${Relative}"
!macroend

!macro DefineDestinationCheck Prefix
Function ${Prefix}CheckDestination
  ; Fixed path and all existing ancestors: never honor /D, links or registry paths.
  StrCpy $INSTDIR "${PRODUCT_DIR}"
  StrCpy $SafePath "$INSTDIR"
ancestor_loop:
  !insertmacro CheckManagedDirectory "$SafePath"
  ${GetParent} "$SafePath" $SafePath
  ${If} $SafePath != ""
    Goto ancestor_loop
  ${EndIf}
  !include "${VALIDATE_PAYLOAD}"
  !insertmacro CheckManagedFile "$INSTDIR\Uninstall.exe"
  StrCpy $SafePath "$SMPROGRAMS"
shortcut_ancestors:
  !insertmacro CheckManagedDirectory "$SafePath"
  ${GetParent} "$SafePath" $SafePath
  ${If} $SafePath != ""
    Goto shortcut_ancestors
  ${EndIf}
  !insertmacro CheckManagedFile "$SMPROGRAMS\NetLane Rust.lnk"
  System::Call 'kernel32::GetFileAttributesW(w "$INSTDIR") i.r0'
  ${If} $0 == -1
    Goto destination_ok
  ${EndIf}
  ClearErrors
  FileOpen $2 "$INSTDIR\netlane-rust-installed.layout" r
  IfErrors unsafe_destination
  FileRead $2 $3
  FileClose $2
  ${If} $3 != "1"
    Goto unsafe_destination
  ${EndIf}
destination_ok:
  Return
unsafe_destination:
  MessageBox MB_OK|MB_ICONSTOP "Destino existente sem identificação, link ou tipo incompatível. Nenhum arquivo externo será alterado." /SD IDOK
  SetErrorLevel 4
  Abort
file_in_use:
  MessageBox MB_OK|MB_ICONSTOP "Arquivo em uso ou sem permissão. Feche o NetLane normalmente antes de continuar." /SD IDOK
  SetErrorLevel 7
  Abort
FunctionEnd
!macroend
!insertmacro DefineDestinationCheck ""
!insertmacro DefineDestinationCheck "un."

!macro RequireSupportedUpgrade
  ReadRegStr $0 HKCU "${PRODUCT_KEY}" "InstallLocation"
  ${If} $0 != ""
  ${AndIf} $0 != "${PRODUCT_DIR}"
    MessageBox MB_OK|MB_ICONSTOP "Registro de instalação de outro local. Operação recusada." /SD IDOK
    SetErrorLevel 4
    Abort
  ${EndIf}
  ReadRegStr $0 HKCU "${PRODUCT_KEY}" "DisplayVersion"
  ${If} $0 != ""
    ${VersionCompare} $0 "${PRODUCT_VERSION}" $1
    ${If} $1 == 1
      MessageBox MB_OK|MB_ICONSTOP "Já existe uma versão Rust mais recente. Downgrade recusado." /SD IDOK
      SetErrorLevel 6
      Abort
    ${EndIf}
  ${EndIf}
  ; Also refuse downgrade of an interrupted package without a completed registry entry.
  IfFileExists "$INSTDIR\rust-package-version.txt" 0 upgrade_ok
  ClearErrors
  FileOpen $2 "$INSTDIR\rust-package-version.txt" r
  IfErrors upgrade_invalid
  FileRead $2 $0
  FileClose $2
  ${VersionCompare} $0 "${PRODUCT_VERSION}" $1
  ${If} $1 == 1
    SetErrorLevel 6
    Abort
  ${EndIf}
  Goto upgrade_ok
upgrade_invalid:
  SetErrorLevel 6
  Abort
upgrade_ok:
!macroend
Function CheckRuntime
  ClearErrors
  ${DisableX64FSRedirection}
  GetDLLVersion "$SYSDIR\vcruntime140.dll" $0 $1
  ${If} ${Errors}
    StrCpy $2 1
  ${Else}
    StrCpy $2 0
  ${EndIf}
  System::Call 'kernel32::GetFileAttributesW(w "$SYSDIR\icuuc.dll") i.r3'
  ${If} $3 == -1
    StrCpy $2 2
  ${EndIf}
  ${EnableX64FSRedirection}
  ${If} $2 == 2
    MessageBox MB_OK|MB_ICONSTOP "Componente ICU do Windows x64 ausente. Esta prévia requer Windows 10 com ICU ou Windows 11. Nenhum arquivo será copiado." /SD IDOK
    SetErrorLevel 5
    Abort
  ${EndIf}
  ${If} $2 == 1
    Goto runtime_missing
  ${EndIf}
  IntOp $3 $0 >> 16
  IntOp $4 $0 & 0xFFFF
  IntOp $5 $1 >> 16
  IntOp $5 $5 & 0xFFFF
  IntOp $6 $1 & 0xFFFF
  StrCpy $0 "$3.$4.$5.$6"
  ${VersionCompare} $0 "${VC_RUNTIME_MIN}" $1
  ${If} $1 == 2
    Goto runtime_missing
  ${EndIf}
  Return
runtime_missing:
  MessageBox MB_OK|MB_ICONSTOP "Esta prévia requer Microsoft Visual C++ Redistributable x64 ${VC_RUNTIME_MIN} ou posterior. Instale o componente Microsoft no laboratório antes de continuar. Nenhum componente será baixado ou instalado automaticamente." /SD IDOK
  SetErrorLevel 8
  Abort
FunctionEnd
Function .onInit
  ${IfNot} ${IsNativeAMD64}
  ${OrIfNot} ${AtLeastWin10}
    MessageBox MB_OK|MB_ICONSTOP "Esta prévia requer Windows 10 ou posterior, x64 Intel/AMD." /SD IDOK
    SetErrorLevel 5
    Abort
  ${EndIf}
  SetRegView 64
  SetShellVarContext current
  Call CheckRuntime
  Call CheckDestination
  !insertmacro RequireSupportedUpgrade
  !insertmacro RequireRustClosed ""
FunctionEnd
Section "NetLane Rust"
  Call CheckRuntime
  Call CheckDestination
  !insertmacro RequireSupportedUpgrade
  !insertmacro RequireRustClosed ""
  !include "${INSTALL_FILES}"
  ClearErrors
  WriteUninstaller "$INSTDIR\Uninstall.exe"
  IfErrors install_failed
  CreateShortcut "$SMPROGRAMS\NetLane Rust.lnk" "$INSTDIR\netlane.exe" "" "$INSTDIR\netlane.exe" 0
  IfErrors install_failed
  WriteRegStr HKCU "${PRODUCT_KEY}" "DisplayName" "NetLane Rust (prévia local)"
  WriteRegStr HKCU "${PRODUCT_KEY}" "DisplayVersion" "${PRODUCT_VERSION}"
  WriteRegStr HKCU "${PRODUCT_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${PRODUCT_KEY}" "DisplayIcon" "$INSTDIR\netlane.exe"
  WriteRegStr HKCU "${PRODUCT_KEY}" "UninstallString" '$\"$INSTDIR\Uninstall.exe$\"'
  WriteRegDWORD HKCU "${PRODUCT_KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${PRODUCT_KEY}" "NoRepair" 1
  WriteRegDWORD HKCU "${PRODUCT_KEY}" "EstimatedSize" ${PAYLOAD_KIB}
  IfErrors install_failed
  Goto install_done
install_failed:
  MessageBox MB_OK|MB_ICONSTOP "Instalação incompleta. Feche o NetLane e execute novamente este instalador para reparar. O perfil foi preservado." /SD IDOK
  SetErrorLevel 7
  Abort
install_done:
SectionEnd
Function un.CheckPackageVersion
  ClearErrors
  FileOpen $2 "$INSTDIR\rust-package-version.txt" r
  IfErrors wrong_version
  FileRead $2 $3
  FileClose $2
  ${If} $3 == "${PRODUCT_VERSION}"
    Return
  ${EndIf}
wrong_version:
  MessageBox MB_OK|MB_ICONSTOP "A versão do desinstalador não corresponde ao pacote. Repare com o instalador correspondente antes de remover." /SD IDOK
  SetErrorLevel 6
  Abort
FunctionEnd
Function un.onInit
  SetRegView 64
  SetShellVarContext current
  ${If} $INSTDIR != "${PRODUCT_DIR}"
    MessageBox MB_OK|MB_ICONSTOP "Remoção recusada fora da pasta original deste usuário." /SD IDOK
    SetErrorLevel 4
    Abort
  ${EndIf}
  Call un.CheckDestination
  !insertmacro RequireRustClosed "un."
  ; A stale/copied uninstaller must not remove a newer payload.
  Call un.CheckPackageVersion
FunctionEnd
Section "Uninstall"
  Call un.CheckDestination
  Call un.CheckPackageVersion
  !insertmacro RequireRustClosed "un."
  ; Only the exact command, same user and Rust value; preserve foreign/legacy Run entries.
  ReadRegStr $0 HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "NetLaneRust"
  ${If} $0 == '$\"$INSTDIR\netlane.exe$\" --startup'
    ClearErrors
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "NetLaneRust"
    IfErrors uninstall_failed
  ${EndIf}
  ; Compile-time allowlist. Never consume an installed manifest as deletion instructions.
  !include "${UNINSTALL_FILES}"
  ClearErrors
  Delete "$SMPROGRAMS\NetLane Rust.lnk"
  IfErrors uninstall_failed
  DeleteRegKey HKCU "${PRODUCT_KEY}"
  IfErrors uninstall_failed
  ; Retain repair identity until the other steps succeed. No profile access.
  Delete "$INSTDIR\Uninstall.exe"
  IfErrors uninstall_failed
  Delete "$INSTDIR\rust-package-version.txt"
  IfErrors uninstall_failed
  Delete "$INSTDIR\netlane-rust-installed.layout"
  IfErrors uninstall_failed
  RMDir "$INSTDIR"
  Goto uninstall_done
uninstall_failed:
  MessageBox MB_OK|MB_ICONSTOP "Remoção incompleta. O perfil foi preservado. Repare a instalação antes de tentar novamente." /SD IDOK
  SetErrorLevel 7
  Abort
uninstall_done:
SectionEnd
