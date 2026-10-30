; Angry Birds 1.2.0 (Rust rebuild) - Windows installer
; Copyright (c) 2026 Shohail Mahmud. MIT licensed - see LICENSE.
;
; A normal graphical setup wizard: no console window, no "type Y and press Enter".
; Browsing to an install folder, a progress bar and a real Uninstall entry.
;
; Built by CI with:  makensis installer/angry_birds.nsi
; Payload it expects in installer/payload/:
;   game.exe   the compiled AngryBirds_1.2.0.exe
;   1.2.0.ipa  the original game data (embedded so players need nothing else)

Unicode true
SetCompressor /SOLID lzma
SetCompressorDictSize 64

!include "MUI2.nsh"
!include "FileFunc.nsh"

!define APP_NAME      "Angry Birds 1.2.0"
!define APP_EXE       "AngryBirds_1.2.0.exe"
!define APP_VERSION   "1.2.0"
!define APP_PUBLISHER "Shohail Mahmud"
!define APP_URL       "https://github.com/shohail-mahmud/angry-bird-1.2.0"
!define REG_UNINST    "Software\Microsoft\Windows\CurrentVersion\Uninstall\AngryBirds1_2"
!define REG_KEY       "Software\AngryBirds1_2"

Name "${APP_NAME}"
BrandingText "${APP_NAME} (Rust rebuild)"
OutFile "AngryBirds_1.2.0_Setup.exe"
InstallDir "$LOCALAPPDATA\AngryBirds1_2"
InstallDirRegKey HKCU "${REG_KEY}" "InstallLocation"

; Per-user install: no UAC prompt, no admin rights needed.
RequestExecutionLevel user
ShowInstDetails show
ShowUninstDetails show
XPStyle on

; File properties -> Details tab
VIProductVersion "1.2.0.0"
VIAddVersionKey /LANG=1033 "ProductName"     "${APP_NAME}"
VIAddVersionKey /LANG=1033 "ProductVersion"  "${APP_VERSION}"
VIAddVersionKey /LANG=1033 "FileDescription" "Angry Birds 1.2.0 setup"
VIAddVersionKey /LANG=1033 "FileVersion"     "1.2.0.0"
VIAddVersionKey /LANG=1033 "CompanyName"     "${APP_PUBLISHER}"
VIAddVersionKey /LANG=1033 "LegalCopyright"  "MIT for the code. Original game (c) Rovio Entertainment."
VIAddVersionKey /LANG=1033 "OriginalFilename" "AngryBirds_1.2.0_Setup.exe"

; ---- wizard look ------------------------------------------------------------------
!define MUI_ICON   "icon.ico"
!define MUI_UNICON "icon.ico"
!define MUI_ABORTWARNING
!define MUI_WELCOMEPAGE_TITLE "Angry Birds 1.2.0"
!define MUI_WELCOMEPAGE_TEXT "This will install the Angry Birds 1.2.0 Rust rebuild on your computer.$\r$\n$\r$\nThe game, its uninstaller and the original game data are all included - there is nothing else to download. It works fully offline.$\r$\n$\r$\nClick Next to continue."
!define MUI_DIRECTORYPAGE_TEXT_TOP "Setup will install ${APP_NAME} in the folder below. Click Install to start."
!define MUI_FINISHPAGE_TITLE "Angry Birds 1.2.0 is installed"
!define MUI_FINISHPAGE_TEXT "Click Finish to start playing.$\r$\n$\r$\nYou can remove the game later from Settings > Apps, or with the Uninstall shortcut in the Start Menu."
!define MUI_FINISHPAGE_RUN "$INSTDIR\${APP_EXE}"
!define MUI_FINISHPAGE_RUN_TEXT "Start Angry Birds 1.2.0"
!define MUI_FINISHPAGE_LINK "Project page on GitHub"
!define MUI_FINISHPAGE_LINK_LOCATION "${APP_URL}"

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

; ---- install ----------------------------------------------------------------------
Section "Angry Birds 1.2.0" SecMain
  SectionIn RO
  SetOutPath "$INSTDIR"

  ; Don't trip over a copy that is already running.
  nsExec::Exec 'taskkill /F /IM "${APP_EXE}"'
  nsExec::Exec 'taskkill /F /IM "AngryBirds_1.2.0_Windows_r9.exe"'
  Pop $0

  SetOverwrite on
  File "/oname=${APP_EXE}"   "payload/game.exe"
  File "/oname=1.2.0.ipa"    "payload/1.2.0.ipa"
  File "/oname=README.txt"   "payload/README.txt"

  WriteUninstaller "$INSTDIR\Uninstall.exe"

  ; Shortcuts
  CreateShortcut "$DESKTOP\Angry Birds 1.2.0.lnk" "$INSTDIR\${APP_EXE}" "" "$INSTDIR\${APP_EXE}" 0
  CreateDirectory "$SMPROGRAMS\Angry Birds 1.2.0"
  CreateShortcut "$SMPROGRAMS\Angry Birds 1.2.0\Angry Birds 1.2.0.lnk" "$INSTDIR\${APP_EXE}" "" "$INSTDIR\${APP_EXE}" 0
  CreateShortcut "$SMPROGRAMS\Angry Birds 1.2.0\Uninstall Angry Birds 1.2.0.lnk" "$INSTDIR\Uninstall.exe" "" "$INSTDIR\Uninstall.exe" 0

  ; Settings -> Apps entry
  ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
  WriteRegStr   HKCU "${REG_KEY}"    "InstallLocation" "$INSTDIR"
  WriteRegStr   HKCU "${REG_KEY}"    "Version"         "${APP_VERSION}"
  WriteRegStr   HKCU "${REG_UNINST}" "DisplayName"     "${APP_NAME} (Rust rebuild)"
  WriteRegStr   HKCU "${REG_UNINST}" "DisplayVersion"  "${APP_VERSION}"
  WriteRegStr   HKCU "${REG_UNINST}" "Publisher"       "${APP_PUBLISHER}"
  WriteRegStr   HKCU "${REG_UNINST}" "URLInfoAbout"    "${APP_URL}"
  WriteRegStr   HKCU "${REG_UNINST}" "InstallLocation" "$INSTDIR"
  WriteRegStr   HKCU "${REG_UNINST}" "DisplayIcon"     "$INSTDIR\${APP_EXE}"
  WriteRegStr   HKCU "${REG_UNINST}" "UninstallString" '"$INSTDIR\Uninstall.exe"'
  WriteRegStr   HKCU "${REG_UNINST}" "QuietUninstallString" '"$INSTDIR\Uninstall.exe" /S'
  WriteRegDWORD HKCU "${REG_UNINST}" "NoModify" 1
  WriteRegDWORD HKCU "${REG_UNINST}" "NoRepair" 1
  WriteRegDWORD HKCU "${REG_UNINST}" "EstimatedSize" $0
SectionEnd

; ---- uninstall --------------------------------------------------------------------
Section "Uninstall"
  nsExec::Exec 'taskkill /F /IM "${APP_EXE}"'
  Pop $0

  ; Installed files
  Delete "$INSTDIR\${APP_EXE}"
  Delete "$INSTDIR\1.2.0.ipa"
  Delete "$INSTDIR\README.txt"
  Delete "$INSTDIR\Uninstall.exe"
  ; anything the player dropped in there
  RMDir /r "$INSTDIR"

  ; Shortcuts
  Delete "$DESKTOP\Angry Birds 1.2.0.lnk"
  Delete "$SMPROGRAMS\Angry Birds 1.2.0\Angry Birds 1.2.0.lnk"
  Delete "$SMPROGRAMS\Angry Birds 1.2.0\Uninstall Angry Birds 1.2.0.lnk"
  RMDir  "$SMPROGRAMS\Angry Birds 1.2.0"

  ; Save data: progress, stars, highscores, settings
  RMDir /r "$APPDATA\AngryBirds1_2_Rust"

  ; Registry
  DeleteRegKey HKCU "${REG_UNINST}"
  DeleteRegKey HKCU "${REG_KEY}"
SectionEnd
