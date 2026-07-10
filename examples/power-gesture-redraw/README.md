# Power Gesture Redraw

This two-app example keeps POWER gesture policy in SquidScript. The root app
arms `redraw-helper` for `key.POWER.doubleTap`. A double tap launches the helper,
which exits immediately; returning starts the root app fresh and redraws its
screen through the normal lifecycle path.

Install both `.squid` files, then launch `main`. The displayed redraw count
increments after each recognized POWER double tap.
