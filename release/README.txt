FFRS PATCHER  -  for FINAL FANTASY RESONANCE (Steam, v1.0.1.0)
==============================================================

INSTALL
   1. Close the game.
   2. Double-click FFRS-Patcher.exe.
   3. Check the folders it found (or use Browse...), tick what you want, press Install.
   To change features later, or remove the mod, run the setup again
   (Uninstall puts the game back exactly as it was).

   Windows may warn that the setup is from an unknown publisher: choose
   "More info" > "Run anyway".

FEATURES
   Full analog movement
      The game snaps the stick to 8 directions; with this you walk exactly
      where you point. (Characters still face one of their 8 sprite directions.)

   Octopath-style battle screen, menus and fonts
      - party status as a compact column at the top-right; the acting member
        slides out to the left
      - command menu beside the party, above the characters; white hand cursor
      - turn order as a bare line, without the blue band
      - neutral charcoal windows with thin white lines instead of the blue theme
        (HP/MP and other meaningful colours kept)
      - Alegreya Sans typeface, also on the "STAGGERED" battle banners
        (English, German, French and Spanish)
      The patcher builds these from YOUR game's own files when you press
      Install; no game file is included in it. If a game update changes the
      parts it edits, it refuses (nothing is changed) until the patcher is
      updated.

   OCTOPATH TRAVELER II menu sounds   (needs OCTOPATH TRAVELER II installed)
      Cursor, confirm, cancel, menus, pages, equip, shop and map sounds are
      replaced by Octopath Traveler II's own. They are read from YOUR copy of
      Octopath Traveler II each time the game starts: nothing from Octopath is
      included in this mod or copied to disk. Without Octopath Traveler II the
      game keeps its own sounds.

SETTINGS
   FINAL FANTASY RESONANCE\FFRS\Binaries\Win64\FFRSPatcher.ini
   SoundVolume   loudness of the Octopath sounds (1.0 = default, 0.5 = half)
   The setup writes the other entries; it keeps SoundVolume when you rerun it.

TROUBLESHOOTING
   FFRS\Binaries\Win64\FFRSPatcher.log shows what was applied each time the
   game starts. If a game update changes the code, the mod logs "signature not
   found" and leaves that part of the game unchanged instead of misbehaving.
   The setup refuses to replace a dsound.dll that belongs to another mod.
   Compatible with UE4SS (which uses dwmapi.dll).

CREDITS / LICENCES
   FFRS Patcher: MIT licence (LICENSE.txt).
   Alegreya Sans (c) The Alegreya Sans Project Authors, SIL Open Font Licence
   1.1 (OFL-AlegreyaSans.txt); re-metricked for use as the game's UI face.
   Libraries compiled into the setup and the mod: THIRD-PARTY.txt.
   FINAL FANTASY RESONANCE and OCTOPATH TRAVELER II are (c) SQUARE ENIX.
   This is an unofficial fan mod, not affiliated with Square Enix.
