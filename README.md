![Neothesia Baner](https://github.com/user-attachments/assets/383438e5-80cd-49d2-af30-85afe5d79c6b)


# Neothesia

Neothesia is a cross-platform MIDI visualizer build in Rust.
It helps people to quickly learn how to play piano.
It takes music notes from a MIDI file as an input and displays them as colorful falling blocks on a virtual piano.

MusicXML scores (`.musicxml`, `.xml`, and the compressed `.mxl`) can be opened as well, they get converted to midi on the fly.
Repeats, voltas and `D.C.`/`D.S.` jumps are unrolled, ties are merged, and every staff of a part becomes its own track, named after the part and its clef (`Piano · treble`, `Piano · bass`, `Voice`), so you can pick the staves you want to play and mute the rest.

Muted tracks are kept out of the performance entirely by default: no sound, no falling notes, and no say in the chord analysis. Turn **Hide Muted Tracks** off in the player's display settings to get them back on screen while staying silent.

## Practice mode

Set a track's player to **Human** in the track picker and the song waits for you: it stops advancing until you play the exact note(s) due at the playhead on a MIDI keyboard (or the PC-keyboard fallback), then continues. Mute the hand you don't want to practice - it disappears from the waterfall entirely and never blocks anything - and set the hand you do want to Human. While frozen, the theory panel names exactly what it's waiting for.

## Theory panel

While a song plays, a panel above the keyboard names what is sounding: the chord under each hand, the harmony both hands make together, its roman numeral in the key, the scale that implies, the meter with the way its beats group (`7/8 (2+2+3)`), the bar and beat, and the note value being played.

The song can also be stepped through like a debugger, one note event at a time, either hand on its own, which sounds the notes under the playhead without the song running away from you. See the [shortcuts](docs/pages/shortcuts.md).

Opensource Synthesia was abandoned in favour of [closed source commercial project](https://www.synthesiagame.com/)  
The goal of this project is to bring Opensource Synthesia back to life, and make it look and work as good (or even better) than commercial Synthesia.

If you have any questions, feel free to join my Discord

[<img alt="Discord" src="https://img.shields.io/discord/273176778946641920?logo=discord&style=for-the-badge&color=%23a051ee">](https://discord.gg/sgeZuVA)

## Screenshots

![image](https://github.com/PolyMeilex/Neothesia/assets/20758186/65483bab-0b74-4fd4-90b1-fdd00508b676)

[![Video](https://github.com/PolyMeilex/Neothesia/assets/20758186/dc564433-aade-4430-b137-5f90000ae9e0)](https://youtu.be/ReE9nVuMCSE)

|![settings](https://github.com/PolyMeilex/Neothesia/assets/20758186/e38642e2-6118-4931-9964-a1df27a36db9)|![track selection](https://github.com/PolyMeilex/Neothesia/assets/20758186/2309d970-0234-45ff-a9f4-105ff08514af)|
|--|--|

[Video](https://youtu.be/ReE9nVuMCSE)

## Download

<a href="https://flathub.org/apps/details/com.github.polymeilex.neothesia"><img width="240" alt="Download on Flathub" src="https://flathub.org/assets/badges/flathub-badge-en.png"/></a>

Arch Linux (**Unofficial AUR** built from source, maintained by @zayn7lie): <https://aur.archlinux.org/packages/neothesia>

All binary releases:
[https://github.com/PolyMeilex/Neothesia/releases](https://github.com/PolyMeilex/Neothesia/releases)

## FAQ

- [FAQ](https://polymeilex.github.io/Neothesia/pages/installation.html)
- [Video encoding](https://polymeilex.github.io/Neothesia/pages/video-encoding.html)

## Thanks to

- [WGPU](https://wgpu.rs/)
- [Linthesia](https://github.com/linthesia/linthesia)
- [Synthesia](https://github.com/johndpope/pianogame)
