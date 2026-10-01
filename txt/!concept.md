# Drifter

The living spec. Rust and Smithay, Wayland, bare DRM. Built and running.

## What it is

A desktop environment where the window manager is a camera, not a window arranger.

There is one large workspace. It persists. Applications sit in places on it. You normally look at it through a viewport the size of one screen, so day to day it feels like a normal desktop.

The point is geography. You learn that the browser lives up-left and the build terminal sits under it, then you navigate by muscle memory instead of window lists.

That only works if one rule is absolute: Drifter never moves your windows. No tiling. No auto-arrange. No sending a window to another workspace. Where you put it is where it stays.

## The hard constraint

Drifter is a compositor. It cannot sit on top of Openbox or any other WM.

To zoom and pan real windows, Drifter must own how every window is drawn and where every click goes. The camera is the scene graph.

## Workspace

Size is in screens, not pixels. `width = 4` and `height = 6` means 4 screens wide and 6 screens tall. On a 4K screen that is 15360 x 12960 px.

Size is fixed. It is never grown by window sprawl, because the overview has to look the same every time. If the world resizes when a window moves, the overview rescales and the geography is lost.

## Config

One file, set once. No CLI flags. Keep the file small and hold everything else as defaults in code. The settings panel, from a right click on the desktop or the cog in the dock, writes the same file. It makes the file when there is none, and a change takes effect at once, with no restart.

TOML.

    # ~/.config/drifter/drifter.toml

    [workspace]
    width = 4
    height = 6
    wallpaper = "/path/to/huge_wallpaper.png"
    wallpaper_mode = "pan"

    [input]
    modifier = "Super"

    [dock]
    pinned = []

    [tray]
    format = "%a %b %d %I:%M:%S %p"

    [drawers]
    top = 36
    bottom = 64

    [assets]
    cursor = "../img/cursor.png"
    font = "../fonts/WorkSans.ttf"
    window = "../img/window"

Defaults in code, not in the file: drawer dwell and hide timings, hot corner dwell, pan speed, animation curve and duration, and every size inside a bar. Minimum zoom is derived. It is whatever fits the whole world in the viewport.

Art lives in the repo, in `img/` and `fonts/`. The defaults live in the repo too, at `conf/default.toml`, with a comment on every key. Your own settings live beside them in `conf/drifter.toml`, which git ignores, so a pull never touches what you set. When `conf/drifter.toml` is missing, Drifter and the settings window make it by copying `conf/default.toml`. A key added to the defaults later still works in an older copy, because every key has a default in the code.

Drifter looks for `conf/drifter.toml` or `conf/default.toml` beside its own binary, walking up from the executable, and falls back to `$XDG_CONFIG_HOME/drifter/drifter.toml` when there is neither. So a build from the repo always reads the repo's config.

Paths in the config are relative to the config file. `../img/cursor.png` works on any machine that has the repo. An absolute path is left alone.

The font is Work Sans, under the SIL Open Font License, kept at `fonts/` with its license.

## Settings

The settings window is `drifter-settings`, a small Wayland program built from Drifter's own drawing code, with no toolkit. Drifter frames it like any window. The dock cog, a right click on empty desktop and the power menu open it, and a second open brings the running one back.

A sidebar picks the page: Desktop, Wallpaper, Windows, Dock, Top Bar, Keyboard & Mouse, Lock Screen, Login Items and Default Apps. Each page is grouped rounded cards with switches, sliders, segmented buttons and small row buttons.

- Desktop: world size, one slider from 2 to 10 screens a side, over a grid that shows the one screen seen at zoom 1.
- Wallpaper: behavior, a folder chosen with a folder browser, a grid of thumbnails from it to click, and rotation every so many seconds, minutes or hours, at random or by name.
- Windows: buttons left or right, their order, and whether the title shows, over a live title bar preview.
- Dock: size, and the dock items. Add one from the installed apps or as a typed command line. Move it up or down, remove it, or give it a PNG or JPEG icon of its own. Below them, five switches show or hide the file manager, terminal and browser the system names as defaults, the settings cog and the trash.
- Top Bar: bar height, text size, the clock and the system info.
- Lock Screen: minutes with no input before the screen locks, or never.
- Login Items: apps and command lines that start each time Drifter starts, added the same way as dock items.
- Default Apps: the file manager, terminal and web browser, each chosen with the same picker or reset to the system's choice. Choosing an installed app also writes it to `~/.config/mimeapps.list`, so other apps agree. The dock switches name the app each one shows.

Every change is written to `conf/drifter.toml` the moment it is made, sliders included while they move, and Drifter applies it within half a second. There is no save button and no restart.

`cargo run -p drifter-core --example settings -- <folder>` draws every page and both dialogs to PNG files, so the design can be checked on any machine.

## Wallpaper modes

One key, `wallpaper_mode`. Three names for the user. One factor in the code.

- `fixed`. Viewport sized. Never moves, never zooms. Factor 0. One screen-sized texture, no tiles, no mipmaps, so it is the cheap mode.
- `pan`. World sized. Moves and zooms with the world. Factor 1. This is the default, because the wallpaper then works as a map: zoom out and parts of the picture become landmarks.
- `parallax`. Any size between the two. Moves slower than the world.

Parallax takes no tuning number. Derive the factor:

    factor = (wallpaper_size - viewport_size) / (world_size - viewport_size)

Derived that way the wallpaper edges land exactly on the world edges, so you never pan off the picture and never see a seam. A 2K wallpaper, a 1080p viewport and a 4K world give exactly one third.

In parallax the picture is its own size, grown until it covers the screen and cut down to the world where it is larger. Zooming out shows more of the picture, until the whole picture fills the screen. It never shows past its own edges, and it never changes shape.

Every mode keeps the picture's shape. Drifter cuts a centered piece of the picture with the shape the mode needs and scales that piece. It never stretches.

The supplied image does not have to be an exact size. Drifter scales it to what the mode needs, keeps the aspect ratio, and crops the overflow.

A world-sized wallpaper in `pan` mode is large. A 4x6 world at 4K is about 199 megapixels, roughly 800 MB raw. So it is stored as mipmapped tiles, and only the tiles near the camera are resident at full detail. Zoomed out, the whole picture is needed but only at a low mip level, which is cheap. The full-resolution image is never resident all at once.

## Rules

Zoom

- 1.0 is the zoom you work at.
- Free zoom with modifier + scroll is still allowed. Zoom centers under the cursor, like Figma or Google Maps.

Camera

- Push the pointer into the top-left corner to zoom out until every window is in view. The camera only zooms and pans. It never moves a window to make them fit.
- Zoomed out, clicks never reach the app. A click means "put this window in the center of my viewport". The camera then elastically zooms back in to it.
- Focus does not follow the camera. Camera and focus are separate.
- Modifier + push the pointer against the viewport edge pans the viewport that way.
- A window can demand the camera. Games and video need exact 1:1 with no zoom.

Windows

- Windows overlap, like a normal stacking desktop.
- A new window opens in the middle of the current viewport.
- The user click-drags a window where it goes.
- While dragging a window, modifier + push against the viewport edge pans the viewport. Without this a window can only move one screen at a time.
- Every window is told the screen is one screen. It never learns the world size. Otherwise maximize fills the whole world and dialogs center in empty space.

Drawers

- Push the pointer at the top edge and an inset drawer slides down, `drawers.top` tall, full width. It holds the system tray.
- The top-left hot corner is NOT part of that edge. A square of `hot_corner_size` in the corner belongs to the camera, so reaching for the overview never opens the tray. The bottom-left corner has no such hole, because there is no gesture there.
- Push the pointer at the bottom edge and the dock slides up. It reads pinned apps, then minimized windows, then the trash, with a thin divider between each pair of groups and none where a group is empty. The whole row is centered.
- With no `[dock] pinned` list, the dock starts with the XDG defaults: the default browser, the default file manager and the default terminal, read from mimeapps.list.
- Clicking a pinned icon starts that app. Clicking a minimized icon brings that window back and raises it. Clicking the trash opens `~/.local/share/Trash/files` with the default file manager, or `xdg-open`. With neither installed Drifter says so on stderr.
- Right clicking a minimized icon gives Show and Exit. Hovering any icon shows its name.
- The trash can is painted by Drifter, never taken from an icon theme. A machine with no theme installed still has a trash can.
- Minimize hides a window and puts its icon in the dock. That is the only way a window ever leaves the screen, and the dock is always the way back.
- The drawer opens after a short dwell at the edge, so a fast pass never pops it open. It stays open while the pointer rests on it, and hides a short delay after the pointer leaves.
- Each bar is exactly its configured height everywhere. The corner curve reaches 10 px further in, but only within 10 px of the left and right ends. Drifter prints the measured band at startup, so this is never a guess.
- The look: the corners of the DESKTOP OPENING are rounded, not the drawer. The panel runs the full width and curves deeper at the left and right ends, so the viewport reads as a rounded window and the drawer as the frame around it.
- An inset shadow runs along the whole inner boundary, inside the panel, following the curve of the corners. The drawer reads as a recess, not as a floating bar.
- The slide is elastic. It eases out past its resting place and settles back. The panel texture carries slack past the screen edge, so the spring can never open a gap and show wallpaper behind the drawer.

Pointer

- The pointer is trapped in the viewport. Its world position is derived from the viewport position. Edge push does not work otherwise.
- Drifter draws its own cursor, last, over everything. A bare DRM compositor has no system cursor, so without this the screen has no pointer at all.
- Over an app, the app picks the cursor. A cursor the app draws itself is shown as it is. A cursor the app names, such as a text beam or a resize arrow, comes from the cursor theme in `XCURSOR_THEME`, then Adwaita. A name Drifter cannot find, and the plain arrow, are Drifter's own cursor.
- An app can lock or confine the pointer, for games. Holding the modifier breaks the hold, so the camera always works.

Multi-monitor

- One shared world. One camera per monitor.
- Every monitor has its own scale, rotation and place. Settings > Displays sets them: click a monitor to choose it, drag it to move it, and its edges snap to the others. They are saved as `[[monitor]]` tables in the config, by connector name.
- The pointer crosses to the next monitor at an edge they share. Holding the modifier stops it at the edge, so edge pan works on every monitor.
- Each monitor has its own drawers and hot corner. The cursor, menus, cards, the switcher and the volume display show on the monitor with the pointer.
- The world size comes from the first monitor.
- Every window is told the largest scale, so it is sharp on every monitor.
- Plugging a monitor in or out is seen at once.

Session

- No app restore on login. Windows are not reopened.
- Camera position is saved across logout.

## What is not built yet

The MVP list is finished: config, the compositor, both drawers, the hot corner, click to fly, free zoom, and edge pan.

What is left:
- Lock on lid close and on a logind Lock signal.
- A `/etc/pam.d/drifter` stack of its own, so the lock does not depend on `login`.
- Submenus. A dbusmenu item with `children-display` is drawn but does not open.
- A world-sized wallpaper as mipmapped tiles. Today the whole image is one texture.
- The headless backend and frame capture, which is testing target 2.

## Tray

The top drawer is the tray. It reads right to left, because the right side is where people look for the clock.

- The power button is hard against the right edge, 12 px in, 20 px square, centered in the band. A left click opens the power menu under it. Hovering it shows "Power".
- The clock sits 14 px left of the power button, centered in the band, at `tray.size` text. It ticks every second while the drawer is open.
- `format` in the config is a strftime string. The default is `%a %b %d %I:%M:%S %p`, which reads `Mon Sep 28 12:35:26 AM`. A format Drifter cannot parse is reported on stderr and the default is used instead.
- Status icons live on the LEFT end, 18 px, 14 px apart, starting 12 px in, growing rightward. They never reach the clock, so the clock never moves when an icon appears.
- Left click is Activate and middle click is the secondary action. Both go straight to the program.
- RIGHT CLICK IS DRIFTER'S. Drifter reads the item's menu over `com.canonical.dbusmenu` and paints it itself at the pointer. It never calls ContextMenu, because an app asked for its menu draws its own window, and then there are two menus.
- Hovering an icon shows its tooltip, taken from the item's ToolTip property.
- Icons come from the StatusNotifierItem D-Bus protocol, which is how modern tray apps publish themselves. Drifter owns `org.kde.StatusNotifierWatcher`, holds the registered item list, and reads each item's `IconPixmap`, or its `IconName` from the icon folders. This needs no Wayland client, so the tray works before windows do. An `IconName` is looked up in the item's own `IconThemePath`, then the icon theme named in the GTK settings and the themes it inherits, then hicolor, then pixmaps, at the size nearest 24 px. Only PNG icons can be drawn.
- Every tray call to an app waits at most 1 second. Clicks and menus each run on their own thread, and icons are read side by side, so an app that stops answering never blocks the others.
- Icons that do not fit before the clock are not drawn.
- When `DBUS_SESSION_BUS_ADDRESS` is unset, Drifter falls back to `unix:path=/run/user/<uid>/bus`. Without that a compositor started outside a login shell never gets a bus and the tray never appears.
- System info sits one gap right of the icons: `CPU 7%   RAM 42%   VDA1 63%`, in the tray ink. `stats = false` in `[tray]` hides it.
- `metrics` in `[tray]` picks what it shows, left to right. `cpu` is the busy share of all cores since the last reading, from `/proc/stat`. `ram` is total minus available, from `/proc/meminfo`. Anything else is a disk: a mount point like `/home`, or a device like `sda3`. A disk is counted the way `df` counts it, and its label is its kernel device name in capitals, like `SDA3`. A disk that is not mounted is left out, and Drifter says so once on stderr.
- The values are read once a second, and only while the drawer is open.
- Every number is right-aligned in a slot as wide as `100%`. The labels and the `%` signs never move. Only the digits change.
- Icons give up their room to the system info. The system info is not drawn if it would reach the clock.

The clock is only drawn while the drawer is open, so a closed tray costs nothing.

Text needs a font, and Drifter has no toolkit to ask. It uses `assets.font` when the config names one, and otherwise the first of a short list of normal system paths. With no font at all the clock is skipped and Drifter says so on stderr. It never fails to start over a font.

## Windows

Drifter draws every window frame itself. Client side decorations cannot work here: the compositor owns zoom, and a client drawing its own frame has no idea what zoom it is being seen at, so its shadows, corners and hit areas would all be wrong.

Drifter decorates a window by default and offers server side mode through `xdg-decoration`. A client that explicitly asks for client side mode is believed, and Drifter draws no frame around it, because two frames is worse than the wrong one.

GTK never implemented `xdg-decoration` and always draws its own title bar. GTK 3 does obey `GTK_CSD=0`, so every app Drifter launches gets that in its environment, along with `QT_WAYLAND_DISABLE_WINDOWDECORATION=1`. An app started by hand from a terminal misses both and will carry its own chrome.

The frame is a nine piece skin, so it can be themed by replacing PNGs and never by editing code. Pieces live in the folder named by `assets.window`:

- `title-left.png`, `title-mid.png`, `title-right.png`. The middle tiles across. The caps carry the rounded top corners.
- `edge-left.png`, `edge-right.png`, `edge-bottom.png`. All tile.
- `corner-bottom-left.png`, `corner-bottom-right.png`.
- The same eight again with `-idle` for an unfocused window.
- `button-close.png`, `button-minimize.png`, `button-maximize.png`, `button-restore.png`, each with `-hover` and `-press`.

Below a zoom of 0.75 the buttons and the title are not drawn. They would be a few pixels across, they cannot be clicked because a click at that zoom flies the camera instead, and scaling the art that far down turns a round button into a smear. The frame itself keeps scaling, so a window still reads as a window.

The pieces decide the sizes. Title bar height is the height of `title-mid.png`, border width is the width of `edge-left.png`. So a theme with a thicker frame needs no config change.

Resize edges are wider than the art. The grab area reaches 7 px inside and outside each border, and the corners win over the sides where they overlap, because a corner is harder to hit. Drag the title bar to move. Double click the title bar to maximize, and again to put it back.

Close asks the app to close. Minimize hides the window into the dock. Maximize fills the viewport and remembers where the window was, so a second press restores it exactly. A maximized or snapped window shows the restore button in place of maximize.

### Fullscreen

An app that asks for fullscreen gets exactly one screen and no frame. Drifter places that screen where the window was, pulled inside the world, and the camera flies onto it at zoom 1. While the fullscreen window has focus, the bars, the hot corner, zoom and edge pan are all off, so a game can use every edge. An app that asks before its first frame, like `foot --fullscreen`, is fullscreen from the start. Leaving fullscreen puts the window back exactly where it was.

The way out is Alt-Tab. The fullscreen window stays fullscreen in the world, and the lock comes back when it has focus again.

### Buttons and title

`[window]` in the config sets which end of the title bar holds the buttons (`buttons = "left"` or `"right"`), their order left to right (`order`, naming close, minimize and maximize once each), and whether the title shows (`title`). Maximize turns into restore on a maximized window in the same slot.

### Alt-Tab and Alt-F4

Alt-Tab opens the switcher in the middle of the screen: one tile per window, newest first, each with its app icon, and the chosen window's title under the row. An app with no icon gets a plain tile with its first letter. Each Tab moves one tile, Shift-Tab moves back, and the row shrinks its tiles to fit the screen. Letting go of Alt picks the window: it comes back from the dock if it was there, takes focus, and the camera flies to it. Alt-F4 closes the focused window the same way its close button does.

`cargo run -p drifter-core --example window` paints the default skin and writes every piece, so there is always a real file to start from.

## Lock

The lock screen is part of the compositor. It is not a client, so there is no protocol for another program to bypass.

While locked:
- Only the lock panel is drawn. Windows, popups, drawers, the dock, the tray, menus and the cursor are not in the frame list at all. If the panel fails to draw, the screen is the plain background color. It never falls back to the desktop.
- Every input event is read by the lock and goes no further. Keyboard focus and pointer focus are cleared, so no client sees a key or a click. Pointer, scroll, gestures, hot corner, edge push, drawers, modifier + Escape and the tray are all dead.
- A window that opens while locked is not shown and does not get focus. Move and resize requests are refused.
- Drifter has no VT switch key, so another VT cannot be reached from the keyboard.

The password is checked by PAM, service `login`, for the user that owns the session (found from the uid, never from `$USER`). Empty passwords are refused. PAM does the fail delay and any account lockout, so the system policy applies. A password that PAM cannot check is never accepted, and Drifter refuses to lock at all when `/etc/pam.d/login` is missing or `pam_start` fails, so nobody is ever locked out with no way in.

The typed password lives in a buffer that never grows, so it is never copied by a reallocation. Every path that ends with it wipes it: backspace, Escape, submit, unlock and drop. The check runs on its own thread, so the screen stays alive, and the copy handed to PAM is wiped when the check ends.

Typing shows dots. Enter checks the password. Backspace removes a letter and Escape clears the field. A wrong password says so and the field starts again. A check that takes over 30 seconds can be tried again.

Log out ends the session only if Drifter is the last thing the login ran. Start it with `exec drifter` from the login shell or from a display manager. If it runs as a child of a logged in shell, quitting or crashing drops to that shell.

## Images

Wallpapers and other images are PNG or JPEG. The format is read from the first bytes of the file, never from the name.

The wallpaper path comes from the config and nowhere else. A missing file is reported on stderr and Drifter starts with no wallpaper. Drifter never silently picks a different file.

## Screen mode

On a real panel Drifter uses the largest mode the panel offers, which is its native mode. On a virtual screen, such as virtio-gpu in the VM, it asks for 1920 x 1080, because a virtual screen offers modes up to 5120 x 2160 and drawing on the CPU cannot keep up with them.

## Renderer

Drifter draws with GLES when there is a real GPU. When the only GL is a software one (llvmpipe, softpipe or swrast), or there is no GL at all, it draws with pixman. llvmpipe shades every damaged pixel through a JIT shader and costs many times more CPU. The `gl` line of the startup log names the renderer.

Pixman draws into dumb buffers, never GBM buffers. Pixman only draws into linear memory it maps. A dumb buffer is always linear; a GBM buffer can carry an unknown modifier, and pixman refuses it.

Both renderers read an offscreen target back top row first, so screen copies are never flipped.

`paint.rs` holds `Painter`, one renderer over both, and `Scanout`, one DRM compositor over both buffer kinds. The rest of Drifter only sees those two.

## Protocols

Drifter speaks these, beside the core ones:

- linux-dmabuf v4 with feedback for the render node, so GPU apps hand over their buffers directly. The same node is given to the scanout exporter, so a buffer can go to the screen without a composite pass.
- Primary selection, for middle click paste.
- xdg-activation. A request with a token under 10 seconds old raises and focuses the window, and brings it back from the dock.
- viewporter and fractional scale. The scale is always 1.
- xdg-output.
- Relative pointer and pointer constraints.
- Cursor shape.
- Text input and input method, so an input method such as fcitx5 works.
- Idle inhibit. Drifter keeps the list. Nothing blanks the screen yet, so nothing reads it yet.
- wlr layer shell. Overlay layers draw over everything but Drifter's cursor. Top layers draw under Drifter's drawers and menus and over windows. Bottom and background layers draw under windows. A layer that asks for the keyboard gets it, and gives it back to the top window when it goes. Maximize leaves room for the space a bar reserves.

X11 apps run through `xwayland-satellite`, when it is installed. Drifter starts it on the first free display number and gives every app it launches that `DISPLAY`. Without it Drifter says so on stderr and X11 apps cannot run.

## Running it

Escape goes to the focused app like any key. Three things open the power menu: the button at the right end of the top drawer, modifier + Escape (in the middle of the screen), and a right click on empty desktop (under Settings). Modifier + L locks the screen at once.

The power menu lists Lock, Log out, Shut down, Suspend and Restart, in that order. Log out stops Drifter, which ends the session on the tty. Shut down, Suspend and Restart call `org.freedesktop.login1.Manager` on the system bus: `PowerOff`, `Suspend` and `Reboot`. Suspend locks first and waits 400 ms so the lock is on the screen before the machine sleeps. If the lock cannot start, Drifter does not suspend.

Media keys work in any app and never reach it. Volume up, down and mute and the mic mute key run `wpctl` on the default sink or source. Brightness goes through logind's `SetBrightness`, so the session user needs no root. Play, pause, next, previous and stop go to the MPRIS player that is playing, or else to the first one. Every key runs on a thread of its own. A small panel at the bottom of the screen shows the volume or brightness for 1.5 seconds.

SIGUSR1 toggles the top drawer and SIGUSR2 toggles the bottom one. This is how a drawer is driven from another machine over ssh, with no mouse: `pkill -USR1 -x drifter`. It is a trigger for the same code path the pointer uses, not a test hook.

## Camera

Windows live in WORLD coordinates and never move when the camera does. Drawers, the dock, the tray, menus, hover text and the cursor live in SCREEN coordinates and never zoom, because they belong to Drifter rather than to the world.

Every window rect is mapped through the camera on its way to the screen. Client surfaces are drawn with the camera zoom as their render scale, so a zoomed out window is a real scaled window, not a thumbnail.

Clients are told nothing about any of this. Pointer events reach them in world coordinates, so a click lands on the same pixel whatever the zoom.

The camera moves by flights. A flight eases with the same spring as the drawers, and its zoom glides in log space, so half way from 0.25 to 1.0 is 0.5 rather than 0.625. A straight line mix makes the first half of a zoom out look like the whole of it.

Controls:
- The top-left corner warms up as the pointer nears it. The glow reaches 128 px and gets brighter the closer the pointer is, so the corner is visible before it is reached.
- The glow stays as long as the pointer stays near, also after the corner has flown the camera out to the overview. It only fades as the pointer moves away, or while the modifier is held.
- Push the pointer into the top-left corner and hold. The camera flies out until every window is in view, framing their bounding box with a margin. With no windows open it fits the whole world. It never zooms past 1.0 on the way in, and never past the world on the way out.
- Zoomed out, a click never reaches the app. It flies the camera to that window at zoom 1. A click on empty world centers there instead.
- Modifier + scroll zooms at the cursor, so the world point under the pointer stays put.
- Modifier + push the pointer against a viewport edge pans that way, by the distance pushed rather than at a fixed speed. The modifier also turns the drawers and the hot corner off while it is held, so the edges belong to the camera.
- A three finger swipe on a trackpad pans in any direction. SPICE carries no gestures, so this only works on real hardware, not in the VM.
- Scroll with no modifier goes to the focused app, untouched.
- The camera is clamped so the viewport can never leave the world.
- Drifter always starts with an empty world, so the camera always starts in the dead center of the world at zoom 1. The camera is not saved when Drifter quits.

## Frame loop

Drifter renders from the DRM VBlank event, never from a sleep.

After `queue_frame` the compositor holds a pending frame. Until the page flip event is read off the DRM fd and `frame_submitted` is called, every later `queue_frame` returns Ok and is silently dropped. The screen then freezes on frame 1 while the frame counter keeps climbing, so a frame count is never proof that anything moved. Only the screen is.

So the DrmDeviceNotifier runs as a calloop event source. On VBlank: `frame_submitted`, then draw the next frame. When nothing changed, `render_frame` reports empty, no flip is queued, and the loop blocks with no work until something wakes it.

The seat notifier is in the same loop. On PauseSession drawing stops and DRM master is released. On ActivateSession master is taken back, the compositor state and buffers are reset, and the screen is redrawn. Without this a VT switch away and back leaves a dead screen.

## Crashes

`scripts/drifter-session` runs Drifter and starts it again when it crashes. A clean exit, from Log out or SIGTERM, ends the session. Four crashes inside one minute also end it, so a Drifter that cannot start does not spin forever. Every crash is written to `$XDG_STATE_HOME/drifter/crash.log`, and a panic adds its message and a backtrace. The session sets `XDG_CURRENT_DESKTOP=drifter`.

Apps cannot outlive the compositor: their Wayland connection dies with it. So after a crash they are gone, and the login apps start again.

A crash never unlocks the screen. Locking writes `$XDG_RUNTIME_DIR/drifter-locked` and unlocking removes it. A Drifter that starts and finds the file starts locked. The runtime folder is empty after a reboot, so a normal boot is not locked.

`scripts/run-tty.sh` stops the session and Drifter, waits for the old Drifter to let go of the screen, and starts the session on tty1.

## Open items

On multi-monitor, the monitor whose camera holds a fullscreen window obeys its lock.

Multi-monitor overlap. Two cameras may look at the same region. Proposal: allow it.

## Deferred

- Saved camera presets. A coordinate plus a zoom level, bound to a keyboard shortcut and optionally a launcher button.
- Moving windows directly in the overview.
- A subtle grid or coordinate system shown when zoomed out.

## Testing

Three targets. Drifter must support target 2, so a headless backend is a build requirement, not an afterthought.

1. Debian Trixie VM, Drifter on bare DRM over virtio-gpu, watched over SPICE. This is the main loop. Two things about it are not optional.
   - The VM needs a RELATIVE mouse, virtio-mouse or usb-mouse, not just the default absolute tablet. With an absolute pointer the host sets the guest pointer position, so pointer confinement is impossible and edge push reports no further motion once the pointer reaches the edge. Edge push and hot corners would then look like Drifter bugs.
   - The viewer must be SPICE, not VNC. The VNC protocol carries only absolute pointer coordinates, so it can never deliver relative motion and can never exercise edge push or confinement. SPICE has a server-side mouse mode that grabs the cursor and sends deltas.
2. Headless backend plus frame capture. The automated check, run without a human. Drifter runs headless, starts a known client, drives the camera, then captures frames and writes a PNG. It checks camera math, window placement, world bounds and wallpaper modes by looking at pixels.
3. Bare metal on the host at 192.168.1.114. Fedora 44, RTX 5060 on the proprietary NVIDIA driver, 8 GB VRAM. The only place animation smoothness can be judged. Target 1 runs under llvmpipe, and a remote SPICE session adds latency and dropped frames on top, so neither can be trusted on feel.
