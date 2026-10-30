@echo off
title Angry Birds 1.2.0 - Setup
cd /d "%~dp0"
setlocal

echo ============================================
echo   ANGRY BIRDS 1.2.0 (Rust rebuild) - INSTALL
echo ============================================
echo.

if not exist "AngryBirds_1.2.0.exe" (
  echo ERROR: AngryBirds_1.2.0.exe is missing from this folder.
  echo Extract the whole zip first, then run INSTALL.bat again.
  pause
  exit /b 1
)

if not exist "1.2.0.ipa" (
  echo Note: 1.2.0.ipa is not in this folder.
  echo The game needs it to start. Copy your 1.2.0.ipa here, or into
  echo the install folder afterwards.
  echo.
)

set "TARGET=%LOCALAPPDATA%\AngryBirds1_2"
if not exist "%TARGET%" mkdir "%TARGET%"

echo Installing to %TARGET% ...
copy /y "AngryBirds_1.2.0.exe" "%TARGET%\" >nul
copy /y "uninstall.exe"        "%TARGET%\" >nul
if exist "1.2.0.ipa" copy /y "1.2.0.ipa" "%TARGET%\" >nul

echo   files copied.

rem ---- Shortcuts -----------------------------------------------------------
powershell -NoProfile -Command ^
  "$t=[Environment]::GetFolderPath('Desktop')+'\Angry Birds 1.2.0.lnk'; $s=(New-Object -ComObject WScript.Shell).CreateShortcut($t); $s.TargetPath='%TARGET%\AngryBirds_1.2.0.exe'; $s.WorkingDirectory='%TARGET%'; $s.IconLocation='%TARGET%\AngryBirds_1.2.0.exe,0'; $s.Description='Angry Birds 1.2.0 (Rust rebuild)'; $s.Save()" >nul 2>&1
echo   desktop shortcut created.

powershell -NoProfile -Command ^
  "$d=[Environment]::GetFolderPath('Programs')+'\Angry Birds 1.2.0'; New-Item -ItemType Directory -Force -Path $d | Out-Null; $s=(New-Object -ComObject WScript.Shell).CreateShortcut($d+'\Angry Birds 1.2.0.lnk'); $s.TargetPath='%TARGET%\AngryBirds_1.2.0.exe'; $s.WorkingDirectory='%TARGET%'; $s.IconLocation='%TARGET%\AngryBirds_1.2.0.exe,0'; $s.Save(); $u=(New-Object -ComObject WScript.Shell).CreateShortcut($d+'\Uninstall Angry Birds 1.2.0.lnk'); $u.TargetPath='%TARGET%\uninstall.exe'; $u.WorkingDirectory='%TARGET%'; $u.Save()" >nul 2>&1
echo   start menu entries created.

rem ---- Apps and features entry ---------------------------------------------
set "K=HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\AngryBirds1_2"
reg add "%K%" /v DisplayName     /t REG_SZ    /d "Angry Birds 1.2.0 (Rust rebuild)" /f >nul
reg add "%K%" /v DisplayVersion  /t REG_SZ    /d "1.2.0" /f >nul
reg add "%K%" /v Publisher       /t REG_SZ    /d "Shohail Mahmud" /f >nul
reg add "%K%" /v InstallLocation /t REG_SZ    /d "%TARGET%" /f >nul
reg add "%K%" /v DisplayIcon     /t REG_SZ    /d "%TARGET%\AngryBirds_1.2.0.exe" /f >nul
reg add "%K%" /v UninstallString /t REG_SZ    /d "\"%TARGET%\uninstall.exe\"" /f >nul
reg add "%K%" /v NoModify        /t REG_DWORD /d 1 /f >nul
reg add "%K%" /v NoRepair        /t REG_DWORD /d 1 /f >nul
reg add "%K%" /v EstimatedSize   /t REG_DWORD /d 20000 /f >nul
echo   registered in Apps ^& features.

echo.
echo ============================================
echo   INSTALL COMPLETE
echo ============================================
echo.
echo  Start it from the Desktop shortcut, or:
echo    "%TARGET%\AngryBirds_1.2.0.exe"
echo.
if not exist "%TARGET%\1.2.0.ipa" (
  echo  REMINDER: copy 1.2.0.ipa into
  echo    %TARGET%
  echo  or the game will not start.
  echo.
)
echo  To remove everything later:
echo    Settings ^> Apps ^> Angry Birds 1.2.0 ^> Uninstall
echo    or "%TARGET%\uninstall.exe"
echo.
pause
if exist "%TARGET%\1.2.0.ipa" start "" "%TARGET%\AngryBirds_1.2.0.exe"
