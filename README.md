# Angry Birds 1.2.0 (Rust rebuild)

A from scratch reimplementation of **Angry Birds 1.2.0 (February 2010)** for Windows, written in
Rust. All 105 levels with the original artwork, sounds and bitmap fonts.

![Rust](https://img.shields.io/badge/Rust-1.75%2B-orange)
![Platform](https://img.shields.io/badge/platform-Windows%20x64-0078D6)
![License](https://img.shields.io/badge/license-MIT-green)

> Unofficial fan project, made for preservation. *Angry Birds* and *Rovio* are trademarks of
> Rovio Entertainment - all original artwork, audio, fonts and level design belong to them.
> No affiliation and no endorsement is claimed, and everything here is credited to Rovio.
> The installer is self-contained: **download one `.exe`, install, play.**

## Features

* **105 levels** across 5 chapters: Poached Eggs, Mighty Hoax, Danger Above, The Big Setup,
  Ham 'Em High
* **Physics** on Rapier2D with a fixed 60 Hz timestep, so slow frames never make towers collapse
* **All bird types** with their special abilities
* **Original rendering**: sprite sheets, PVR and PNG textures, bitmap fonts, parallax backgrounds
  and ground cross sections
* **Audio**: WAV sound effects and MP3 music
* **Save data**: progress, stars, highscores, seen tutorials, mute and interface scale
* **Camera**: height fit framing like the original, free zoom out to 0.25x, panning by mouse or
  keyboard, automatic follow of a shot bird

## Install

Download **AngryBirds_1.2.0_Setup.exe** from [Releases](../../releases/latest) and run it.
It needs no admin rights.

The game is registered in *Settings > Apps*, so it can be removed from there. The Start Menu
carries an **Uninstall Angry Birds 1.2.0** shortcut as well. Uninstalling removes the game, your
saves and settings, the shortcuts and the registry entries.

> Unsigned binaries: Windows SmartScreen will warn once. Click **More info → Run anyway**, or
> right-click the file → *Properties* → *Unblock*.

## Controls

| Input | Action |
|---|---|
| Drag the bird, release | aim and shoot |
| Space / click | bird's special ability in flight |
| Arrow keys, WASD, left drag | pan the camera |
| Mouse wheel | zoom in and out (the whole level fits on screen when zoomed out) |
| `[` / `]` | shrink or grow the interface |
| `R` | restart level |
| `Esc` / `P` | pause |
| `Home` | recentre the camera |
| `F11` | fullscreen |
| `M` | mute |

## Assets

The game is built using the original **1.2.0.ipa** archive, which supplies every sprite, sound,
font and level layout. **1.2.0.ipa is copyrighted by Rovio Entertainment.**

## Credits

Rebuilt in Rust by **Shohail Mahmud**
Instagram [@shohailmahmud09](https://instagram.com/shohailmahmud09) and nirjonpc@gmail.com

Original game by Rovio Entertainment. This is an unofficial fan project for study and
preservation, with no affiliation to Rovio.

## License

MIT for this project's own code, see [LICENSE](LICENSE). It grants no rights to the original game,
its assets, its names or its trademarks, which belong to Rovio Entertainment.v
