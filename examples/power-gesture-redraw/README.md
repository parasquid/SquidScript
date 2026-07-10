# Power Gesture Redraw

This three-app example keeps POWER gesture policy in SquidScript. The root app
arms `redraw-helper` for `key.POWER.doubleTap` and `sleep-helper` for
`key.POWER.longTap`. A double tap launches the redraw helper, which exits
immediately; returning starts the root app fresh and redraws its screen through
the normal lifecycle path. A long tap launches the sleep helper, which requests
timerless planned sleep and saves its state during `power.sleep` cleanup.

Install all three `.squid` files, then launch `main`. The displayed redraw count
increments after each recognized POWER double tap. Physical POWER wakes the
device from the timerless sleep requested by the long-tap helper.
