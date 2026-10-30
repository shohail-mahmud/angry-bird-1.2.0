ANGRY BIRDS 1.2.0 - RUST REBUILD
================================

FILES
  AngryBirds_1.2.0.exe  the game
  uninstall.exe         removes the game and all its data
  INSTALL.bat           installs and creates shortcuts
  README.txt            this file

YOU NEED 1.2.0.ipa
  The game reads the original Angry Birds 1.2.0 archive for its artwork,
  sounds and levels. It is NOT included (it belongs to Rovio).
  Copy your 1.2.0.ipa next to AngryBirds_1.2.0.exe.

INSTALL
  1. Run INSTALL.bat   (or just start AngryBirds_1.2.0.exe from this folder)
  2. Make sure 1.2.0.ipa is in the same folder as the game

UNINSTALL - NO TRACE LEFT
  Settings > Apps > Angry Birds 1.2.0 > Uninstall, or run uninstall.exe.
  Removes: the game folder, %APPDATA%\AngryBirds1_2_Rust (saves, stars,
  highscores, settings), the registry entries and the shortcuts.
  Nothing outside those paths is touched.

IF WINDOWS BLOCKS IT
  'Windows protected your PC' -> More info -> Run anyway.
  Nothing happens on double-click -> right-click the exe > Properties >
  tick Unblock > OK.

CONTROLS
  Drag + release  shoot        Space  ability      Wheel  zoom
  Arrows / WASD / drag  camera  [ ]  UI size       R  restart
  Esc / P  pause               F11  fullscreen     M  mute

CREDITS
  Rebuilt in Rust by Shohail Mahmud - @shohailmahmud09 - nirjonpc@gmail.com
  Original Angry Birds (2010) by Rovio Entertainment. Fan project.
