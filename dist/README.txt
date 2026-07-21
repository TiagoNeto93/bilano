Bilano - game vs voice audio balance for Windows
=================================================

One slider balances your game audio against your voice-chat apps. No
installer, no audio driver, no runtime - it's a single .exe. Same job as
the game/chat dial on a SteelSeries Arctis, minus the driver stack.

The name is from Latin "bilanx", two scale-pans - the root of "balance".


HOW TO RUN
----------
1. Double-click  bilano.exe
2. Windows SmartScreen may say "Windows protected your PC" because the app
   isn't code-signed. That's expected for a small indie build.
   Click  "More info"  ->  "Run anyway".
3. A window opens and a blue/green dial icon appears in the system tray
   (bottom-right, possibly under the ^ arrow).


HOW TO USE
----------
- Drag the slider toward GAME to fade voice chat down, toward CHAT to fade
  the game down. Center = both at full volume.
- In the "Apps" list, tick the apps that are voice chat (Discord, etc.).
  Everything else is treated as "game". You can also add one by name.
- Right-click the tray icon for a quick tag checklist without opening the
  window.
- Global hotkeys (work even while in-game):
     Ctrl+Alt+Left   -> toward Chat
     Ctrl+Alt+Right  -> toward Game
     Ctrl+Alt+Down   -> re-center
- Closing the window hides it to the tray; use tray -> Quit to exit.
- "Start with Windows" makes it launch on login.


HOW IT WORKS
------------
It balances by adjusting each app's per-application volume through the
Windows audio API (the same volumes you'd see in the Volume Mixer), using a
smooth decibel fade. It restores every app to full volume when you quit.

Settings are saved to:  %APPDATA%\bilano\config.json


UPGRADING FROM CHATMIX 1.x
--------------------------
Bilano is the same app, renamed. On first launch it moves your old
%APPDATA%\chatmix settings across and repoints "Start with Windows" at the
new exe. You can delete the old chatmix.exe afterwards.

Requirements: 64-bit Windows 10/11. Nothing else.
