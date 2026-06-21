export const DEFAULT_SOURCE = `app "hello-menu" target "xteink-x4"

state {
  selected: int = 0,
  view: string = "menu"
}

event.on("app.start") {
  state.load()
  state.view = "menu"
  screen.open("menu")
}

event.on("key.DOWN") {
  if (state.view == "menu") {
    if (state.selected < 2) {
      state.selected = state.selected + 1
      state.save()
      screen.refresh()
    }
  }
}

event.on("key.UP") {
  if (state.view == "menu") {
    if (state.selected > 0) {
      state.selected = state.selected - 1
      state.save()
      screen.refresh()
    }
  }
}

event.on("key.SELECT") {
  if (state.selected == 0) {
    state.view = "hello"
    screen.open("hello")
  } else {
    if (state.selected == 1) {
      state.view = "about"
      screen.open("about")
    } else {
      app.exit()
    }
  }
}

event.on("key.BACK") {
  if (state.view != "menu") {
    state.view = "menu"
    state.save()
    screen.open("menu")
  } else {
    state.save()
    app.exit()
  }
}

function drawMenuRow(index, label, y) {
  if (state.selected == index) {
    service.display.text(label, {
      x: 32,
      y: y,
      w: 416,
      h: 48,
      fontHeight: 22,
      align: "center",
      valign: "middle",
      textColor: color.GRAY0,
      backgroundColor: color.GRAY15
    })
  } else {
    service.display.text(label, {
      x: 32,
      y: y,
      w: 416,
      h: 48,
      fontHeight: 22,
      align: "center",
      valign: "middle",
      textColor: color.GRAY15,
      backgroundColor: color.GRAY0
    })
  }
}

screen("menu", { render: "compose" }) {
  service.display.clear(color.GRAY0)

  service.display.text("Hello Menu", {
    x: 20,
    y: 60,
    w: 440,
    h: 48,
    fontHeight: 32,
    align: "center",
    valign: "middle"
  })

  drawMenuRow(0, "Say Hello", 160)
  drawMenuRow(1, "About", 216)
  drawMenuRow(2, "Exit", 272)

  service.display.text("UP/DOWN select  SELECT open", {
    x: 20,
    y: 720,
    w: 440,
    h: 32,
    fontHeight: 18,
    align: "center",
    valign: "middle",
    textColor: color.GRAY8
  })
}

screen("hello") {
  service.display.clear(color.GRAY0)
  service.display.text("Hello, Squid!", {
    x: 20,
    y: 120,
    w: 440,
    h: 64,
    fontHeight: 32,
    align: "center",
    valign: "middle"
  })
  service.display.text("BACK returns to menu", {
    x: 20,
    y: 720,
    w: 440,
    h: 32,
    fontHeight: 18,
    align: "center",
    valign: "middle",
    textColor: color.GRAY8
  })
}

screen("about") {
  service.display.clear(color.GRAY0)
  service.display.text("Selection is state.", {
    x: 32,
    y: 120,
    w: 416,
    h: 48,
    fontHeight: 24,
    align: "center",
    valign: "middle"
  })
  service.display.text("Changing selected then calling screen.refresh redraws the menu from state. The old highlight is not manually erased.", {
    x: 32,
    y: 200,
    w: 416,
    h: 160,
    fontHeight: 18,
    wrap: true
  })
}
`;
