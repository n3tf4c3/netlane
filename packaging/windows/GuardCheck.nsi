; Diagnostic-only executable. Never installs, elevates, or changes network state.
Unicode true
RequestExecutionLevel user
SilentInstall silent
AutoCloseWindow true
!include "LogicLib.nsh"
!include "ProcessGuard.nsh"
Name "NetLane installer guard check"
OutFile "${CHECK_OUTPUT}"
!insertmacro DefineProcessGuard ""
Section
  Call CheckNetLaneProcesses
  SetErrorLevel $GuardResult
SectionEnd
