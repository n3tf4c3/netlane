; A harmless process for the guard test. Exits itself after 15 seconds.
Unicode true
RequestExecutionLevel user
SilentInstall silent
AutoCloseWindow true
Name "NetLane guard test fixture"
OutFile "${FIXTURE_OUTPUT}"
Section
  Sleep 15000
SectionEnd
