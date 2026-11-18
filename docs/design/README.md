# Mockups

The record of how each view should look, as plain HTML: open `index.html` in a
browser and zoom with Ctrl and scroll. No build step and no script. An agent
reads a mockup's source like any file, so it costs nothing until opened.

A shot of the view as built goes in only when it shows something the mockup
can't, and then as one final PNG, so the repo does not fill with images.

To see a board as an image:

    chromium --headless --window-size=1360,1040 \
      --screenshot=/tmp/board.png "file://$PWD/docs/design/<board>.html"

## From the canvas

The 20 boards of the "Envio" design canvas were exported here on 2026-11-07,
one file per board, `carbon-*.html` and `airmail-*.html`. They keep the
canvas's layout, so they have inline styles and their own size (1360 × 1040,
the icon boards 1360 × 900). `docs/10-design.md` names them by file.

Fonts load from Google Fonts (Inter, IBM Plex Mono, Lora), so offline they fall
back to system fonts unless installed.

The canvas notes that changed the first boards are kept in
`docs/10-design.md` under "Changes to the first boards"; where a board and that
section disagree, the section wins.

Added after the export: the meeting invite card (0.2.0), at the foot of
`carbon-reading` and `airmail-reading`, in light and dark. It shows every state:
open, with a note, answered, updated, cancelled, reply, sending and send failed.
