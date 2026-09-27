# Validation symbol fallback

NotoEmoji-Regular.ttf is an unmodified Google Noto Emoji font, distributed under the SIL Open Font License 1.1 in NotoEmoji-OFL.txt. Upstream: https://github.com/googlefonts/noto-emoji . Copied from Fedora google-noto-emoji-fonts at /usr/share/fonts/google-noto-emoji-fonts/NotoEmoji-Regular.ttf.

SHA-256: `b57ed895ae9d09ba7b4b19c343a75cf39aad57156c3450e339d9d11556d7edc1`.

Fontconfig charset inspection confirms U+1F50D (magnifier), U+1F4CB (clipboard), U+2705 (check), U+274C (cross), and U+26A0 (warning). It is appended after existing egui proportional and monospace fonts, preserving the primary text face. Egui renders monochrome outlines; this does not reproduce Avalonia color emoji or establish matching glyph metrics.
