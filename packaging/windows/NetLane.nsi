Unicode true
ManifestDPIAware true
RequestExecutionLevel admin
SetCompressor /SOLID zlib
SetOverwrite on

!include "MUI2.nsh"
!include "LogicLib.nsh"
!include "x64.nsh"
!include "WinVer.nsh"
!include "WordFunc.nsh"
!include "ProcessGuard.nsh"
!include "${BUILD_DEFINES}"

; NSIS 3.12 exposes the uninstaller at compile time; no installer execution.
!ifdef SIGN_UNINSTALLER_COMMAND
  !uninstfinalize '${SIGN_UNINSTALLER_COMMAND}' = 0
!endif

!define PRODUCT_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\NetLane"
Name "NetLane (prévia local)"
OutFile "${SETUP_OUTPUT}"
InstallDir "$PROGRAMFILES64\NetLane"
VIProductVersion "${PRODUCT_VERSION}.0"
VIAddVersionKey /LANG=1046 "ProductName" "NetLane"
VIAddVersionKey /LANG=1046 "FileDescription" "Instalador local de avaliação do NetLane"
VIAddVersionKey /LANG=1046 "FileVersion" "${PRODUCT_VERSION}.0"
VIAddVersionKey /LANG=1046 "ProductVersion" "${PRODUCT_VERSION}"
VIAddVersionKey /LANG=1046 "LegalCopyright" "NetLane contributors"

!define MUI_WELCOMEPAGE_TITLE "Instalação local do NetLane"
!define MUI_WELCOMEPAGE_TEXT "Versão experimental para Windows x64.$\r$\n$\r$\nO programa será instalado em Program Files. Suas regras e preferências ficam no perfil do usuário e são preservadas na remoção.$\r$\n$\r$\nNão inicia o painel ou serviço, não configura início automático e não altera opções de rede. Feche o NetLane, inclusive a bandeja, antes de continuar."
!define MUI_FINISHPAGE_TEXT "Arquivos instalados. Abra o NetLane pelo menu Iniciar quando desejar.$\r$\n$\r$\nO painel abre sem elevação. Iniciar o serviço exige confirmação e UAC dentro do painel.$\r$\n$\r$\nRegras do checkout não são importadas automaticamente."
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "PortugueseBR"

!insertmacro DefineProcessGuard ""
!insertmacro DefineProcessGuard "un."

!macro CheckManagedDirectory Path
  System::Call 'kernel32::GetFileAttributesW(w "${Path}") i.r0'
  ${If} $0 != -1
    IntOp $1 $0 & 0x400
    IntOp $2 $0 & 0x10
    ${If} $1 != 0
    ${OrIf} $2 == 0
      MessageBox MB_OK|MB_ICONSTOP "Uma pasta do programa é um link/junção ou foi substituída por um arquivo. Operação recusada para preservar arquivos externos." /SD IDOK
      SetErrorLevel 4
      Abort
    ${EndIf}
  ${EndIf}
!macroend

!macro CheckPayloadDirectory Relative
  !insertmacro CheckManagedDirectory "$INSTDIR\${Relative}"
!macroend

!macro CheckManagedFile Path
  System::Call 'kernel32::GetFileAttributesW(w "${Path}") i.r0'
  ${If} $0 != -1
    IntOp $1 $0 & 0x400
    IntOp $2 $0 & 0x10
    ${If} $1 != 0
    ${OrIf} $2 != 0
      MessageBox MB_OK|MB_ICONSTOP "Um arquivo do programa é um link ou foi substituído por uma pasta. Operação recusada para preservar arquivos externos." /SD IDOK
      SetErrorLevel 4
      Abort
    ${EndIf}
    ; OPEN_EXISTING: never create, truncate or write. Check locks before copying/deleting.
    System::Call 'kernel32::CreateFileW(w "${Path}", i 0xC0000000, i 0, p 0, i 3, i 0x80, p 0) p.r0'
    ${If} $0 == -1
      MessageBox MB_OK|MB_ICONSTOP "Há arquivos do NetLane em uso ou sem permissão de acesso. Feche o painel e aguarde a parada normal do serviço antes de continuar." /SD IDOK
      SetErrorLevel 7
      Abort
    ${EndIf}
    System::Call 'kernel32::CloseHandle(p r0)'
  ${EndIf}
!macroend

!macro CheckPayloadFile Relative
  !insertmacro CheckManagedFile "$INSTDIR\${Relative}"
!macroend

; Fixed destination: never honor /D or a writable directory from the registry.
; Reject junctions/symlinks rather than following them during elevated file operations.
!macro RequireSafeDestination
  StrCpy $INSTDIR "$PROGRAMFILES64\NetLane"
  System::Call 'kernel32::GetFileAttributesW(w "$INSTDIR") i.r0'
  ${If} $0 != -1
    IntOp $1 $0 & 0x400
    ${If} $1 != 0
      MessageBox MB_OK|MB_ICONSTOP "A pasta de instalação é um link/junção. Instalação ou remoção recusada." /SD IDOK
      SetErrorLevel 4
      Abort
    ${EndIf}
    ClearErrors
    FileOpen $2 "$INSTDIR\netlane-installed.layout" r
    ${IfNot} ${Errors}
      FileRead $2 $3
      FileClose $2
      StrLen $4 "NetLane installed layout v1"
      StrCpy $3 $3 $4
    ${Else}
      StrCpy $3 ""
    ${EndIf}
    ${If} $3 != "NetLane installed layout v1"
      MessageBox MB_OK|MB_ICONSTOP "A pasta NetLane já existe sem identificação de instalação. Nenhum arquivo foi removido. Confira a pasta antes de continuar." /SD IDOK
      SetErrorLevel 4
      Abort
    ${EndIf}
  ${EndIf}
  !include "${VALIDATE_PAYLOAD}"
  !insertmacro CheckPayloadFile "Uninstall.exe"
  !insertmacro CheckManagedDirectory "$SMPROGRAMS\NetLane"
  !insertmacro CheckManagedFile "$SMPROGRAMS\NetLane\NetLane.lnk"
!macroend

!macro RequireSupportedUpgrade
  ReadRegStr $0 HKLM "${PRODUCT_KEY}" "DisplayVersion"
  ${If} $0 != ""
    ${VersionCompare} $0 "${PRODUCT_VERSION}" $1
    ${If} $1 == 1
      MessageBox MB_OK|MB_ICONSTOP "Já existe uma versão mais recente do NetLane. Downgrade recusado." /SD IDOK
      SetErrorLevel 6
      Abort
    ${EndIf}
  ${EndIf}
!macroend

Function .onInit
  ${IfNot} ${IsNativeAMD64}
    MessageBox MB_OK|MB_ICONSTOP "Esta prévia requer Windows x64 (Intel/AMD). ARM64 ainda não foi validado." /SD IDOK
    SetErrorLevel 5
    Abort
  ${EndIf}
  ${IfNot} ${AtLeastWin10}
    MessageBox MB_OK|MB_ICONSTOP "Esta prévia requer Windows 10 ou posterior. O serviço também verifica a disponibilidade da API de roteamento." /SD IDOK
    SetErrorLevel 5
    Abort
  ${EndIf}
  SetRegView 64
  SetShellVarContext all
  ; Process guard first: an open panel locks payload files, and the user must be
  ; told to close NetLane (code 2), not that files are in use (code 7).
  !insertmacro RequireNetLaneClosed ""
  !insertmacro RequireSafeDestination
  !insertmacro RequireSupportedUpgrade
FunctionEnd

Section "NetLane" MainSection
  ; Recheck after the welcome page; do not close or kill a process automatically.
  !insertmacro RequireNetLaneClosed ""
  !insertmacro RequireSafeDestination
  !insertmacro RequireSupportedUpgrade
  !include "${INSTALL_FILES}"
  ClearErrors
  WriteUninstaller "$INSTDIR\Uninstall.exe"
  IfErrors install_failed
  CreateDirectory "$SMPROGRAMS\NetLane"
  CreateShortcut "$SMPROGRAMS\NetLane\NetLane.lnk" "$INSTDIR\NetLane.UI.exe" "" "$INSTDIR\NetLane.UI.exe" 0
  IfErrors install_failed
  WriteRegStr HKLM "${PRODUCT_KEY}" "DisplayName" "NetLane (prévia local)"
  WriteRegStr HKLM "${PRODUCT_KEY}" "DisplayVersion" "${PRODUCT_VERSION}"
  WriteRegStr HKLM "${PRODUCT_KEY}" "Publisher" "NetLane"
  WriteRegStr HKLM "${PRODUCT_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKLM "${PRODUCT_KEY}" "DisplayIcon" "$INSTDIR\NetLane.UI.exe"
  WriteRegStr HKLM "${PRODUCT_KEY}" "UninstallString" '$\"$INSTDIR\Uninstall.exe$\"'
  WriteRegDWORD HKLM "${PRODUCT_KEY}" "NoModify" 1
  WriteRegDWORD HKLM "${PRODUCT_KEY}" "NoRepair" 1
  WriteRegDWORD HKLM "${PRODUCT_KEY}" "EstimatedSize" ${PAYLOAD_KIB}
  IfErrors install_failed
  Goto install_done
install_failed:
  MessageBox MB_OK|MB_ICONSTOP "Não foi possível gravar todos os arquivos. A instalação não foi concluída. Feche o NetLane e execute novamente este instalador; dados do perfil não foram removidos." /SD IDOK
  SetErrorLevel 7
  Abort
install_done:
SectionEnd

Function un.onInit
  SetRegView 64
  SetShellVarContext all
  ; A copied/moved uninstaller must never delete files from an unrelated directory.
  ${If} $INSTDIR != "$PROGRAMFILES64\NetLane"
    MessageBox MB_OK|MB_ICONSTOP "Remoção recusada fora da pasta original do NetLane." /SD IDOK
    SetErrorLevel 4
    Abort
  ${EndIf}
  !insertmacro RequireNetLaneClosed "un."
  !insertmacro RequireSafeDestination
FunctionEnd

Section "Uninstall"
  !insertmacro RequireNetLaneClosed "un."
  !insertmacro RequireSafeDestination
  ; Exact compile-time file list; no recursive deletion, profile access, or wildcard.
  !include "${UNINSTALL_FILES}"
  ; RMDir deliberately leaves unknown files/directories intact. Their presence
  ; must not turn a successful removal of the known payload into a false failure.
  ClearErrors
  Delete "$SMPROGRAMS\NetLane\NetLane.lnk"
  IfErrors uninstall_failed
  RMDir "$SMPROGRAMS\NetLane"
  ; Remove only this exact per-user startup command; preserve entries from other locations.
  ReadRegStr $0 HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "NetLane"
  ${If} $0 == '$\"$INSTDIR\NetLane.UI.exe$\" --startup'
    ClearErrors
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "NetLane"
    IfErrors uninstall_failed
  ${EndIf}
  ClearErrors
  DeleteRegKey HKLM "${PRODUCT_KEY}"
  IfErrors uninstall_failed
  ; Keep the marker/uninstaller until other removal operations have succeeded.
  ClearErrors
  Delete "$INSTDIR\Uninstall.exe"
  IfErrors uninstall_failed
  Delete "$INSTDIR\netlane-installed.layout"
  IfErrors uninstall_failed
  RMDir "$INSTDIR"
  Goto uninstall_done
uninstall_failed:
  MessageBox MB_OK|MB_ICONSTOP "Não foi possível remover todos os arquivos do programa. A remoção não foi concluída. Dados do perfil foram preservados." /SD IDOK
  SetErrorLevel 7
  Abort
uninstall_done:
SectionEnd
