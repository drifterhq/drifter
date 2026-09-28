# Drifter

Concept notes. No tech stack chosen yet.

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

One file, set once. No CLI flags. Keep the file small and hold everything else as defaults in code, with an in-app settings panel later.

TOML.

    # ~/.config/drifter/drifter.toml

    [workspace]
    width = 4
    height = 6
    wallpaper = "/path/to/huge_wallpaper.png"
    wallpaper_mode = "pan"

    [input]
    modifier = "Super"

    [tray]
    format = "%a %b %d %I:%M:%S %p"

Defaults in code, not in the file: drawer height 64 px, drawer dwell and hide timings, hot corner dwell, pan speed, animation curve and duration. Minimum zoom is derived. It is whatever fits the whole world in the viewport.

Images live at a convention path, not a config key: `~/.local/share/drifter/`, or `$XDG_DATA_HOME/drifter/`. The splash is `splash.png` there.

## Wallpaper modes

One key, `wallpaper_mode`. Three names for the user. One factor in the code.

- `fixed`. Viewport sized. Never moves, never zooms. Factor 0. One screen-sized texture, no tiles, no mipmaps, so it is the cheap mode.
- `pan`. World sized. Moves and zooms with the world. Factor 1. This is the default, because the wallpaper then works as a map: zoom out and parts of the picture become landmarks.
- `parallax`. Any size between the two. Moves slower than the world.

Parallax takes no tuning number. Derive the factor:

    factor = (wallpaper_size - viewport_size) / (world_size - viewport_size)

Derived that way the wallpaper edges land exactly on the world edges, so you never pan off the picture and never see a seam. A 2K wallpaper, a 1080p viewport and a 4K world give exactly one third.

The supplied image does not have to be an exact size. Drifter scales it to what the mode needs, keeps the aspect ratio, and crops the overflow.

A world-sized wallpaper in `pan` mode is large. A 4x6 world at 4K is about 199 megapixels, roughly 800 MB raw. So it is stored as mipmapped tiles, and only the tiles near the camera are resident at full detail. Zoomed out, the whole picture is needed but only at a low mip level, which is cheap. The full-resolution image is never resident all at once.

## Rules

Zoom

- 1.0 is the zoom you work at.
- Free zoom with modifier + scroll is still allowed. Zoom centers under the cursor, like Figma or Google Maps.

Camera

- Push the pointer into the top-left corner to zoom out to the whole world.
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

- Push the pointer at the top edge and an inset drawer slides down, 64 px, full width. It holds the system tray.
- Push the pointer at the bottom edge and an inset drawer holds the launcher icons and a trash can.
- The drawer opens after a short dwell at the edge, so a fast pass never pops it open. It stays open while the pointer rests on it, and hides a short delay after the pointer leaves.
- The bar is 64 px everywhere. The corner curve reaches 10 px further down, but only within 10 px of the left and right ends. Drifter prints the measured band at startup, so this is never a guess.
- The look: the corners of the DESKTOP OPENING are rounded, not the drawer. The panel runs the full width and curves deeper at the left and right ends, so the viewport reads as a rounded window and the drawer as the frame around it.
- An inset shadow runs along the whole inner boundary, inside the panel, following the curve of the corners. The drawer reads as a recess, not as a floating bar.
- The slide is elastic. It eases out past its resting place and settles back. The panel texture carries slack past the screen edge, so the spring can never open a gap and show wallpaper behind the drawer.

Pointer

- The pointer is trapped in the viewport. Its world position is derived from the viewport position. Edge push does not work otherwise.
- Drifter draws its own cursor, last, over everything. A bare DRM compositor has no system cursor, so without this the screen has no pointer at all.

Multi-monitor

- One shared world. One camera per monitor.

Session

- No app restore on login. Windows are not reopened.
- Camera position is saved across logout.

## MVP order

1. Read the config: world size in screens, wallpaper. Draw the wallpaper fullscreen and hold the screen until killed.
2. Draw the world. Draw client windows. Camera locked at zoom 1.0. This is the whole compositor and it is the big step.
3. Top drawer with tray and system info. Bottom drawer with launcher and trash. Edge push, dwell, elastic slide.
4. Top-left hot corner. Dwell, then zoom out to fit the world.
5. Click a window in overview. Camera flies to it and settles at 1.0.
6. Modifier + scroll zoom at the cursor. Modifier + edge push to pan.
7. Save and restore camera position across logout.

## Tray

The top drawer is the tray. It reads right to left, because the right side is where people look for the clock.

- The clock is hard against the right edge, 18 px in, centered in the 64 px band, at 16 px text. It ticks every second while the drawer is open.
- `format` in the config is a strftime string. The default is `%a %b %d %I:%M:%S %p`, which reads `Mon Sep 28 12:35:26 AM`. A format Drifter cannot parse is reported on stderr and the default is used instead.
- Status icons live on the LEFT end, 24 px, 10 px apart, starting 18 px in, growing rightward. They never reach the clock, so the clock never moves when an icon appears.
- An icon does what its own program says. Left click is Activate, right click opens that program's context menu, middle click is the secondary action. Drifter routes the click and draws the icon. It never invents a menu of its own.
- Icons come from the StatusNotifierItem D-Bus protocol, which is how modern tray apps publish themselves. Drifter owns the org.kde.StatusNotifierWatcher name, holds the list of registered items, draws each item's icon, and calls Activate, ContextMenu or SecondaryActivate on the item when it is clicked. This needs no Wayland client, so the tray can be built and tested before windows work.
- System info comes later, on the same left end, to the right of the icons.

The clock is only drawn while the drawer is open, so a closed tray costs nothing.

Text needs a font, and Drifter has no toolkit to ask. It uses `font.ttf` in the data directory when there is one, and otherwise the first of a short list of normal system paths. With no font at all the clock is skipped and Drifter says so on stderr. It never fails to start over a font.

## Images

Wallpapers and other images are PNG or JPEG. The format is read from the first bytes of the file, never from the name.

The wallpaper path comes from the config and nowhere else. A missing file is reported on stderr and Drifter starts with no wallpaper. Drifter never silently picks a different file.

## Running it

Escape quits. There is nothing else to leave with while Drifter owns the tty, so the key stays until a real menu exists.

SIGUSR1 toggles the top drawer and SIGUSR2 toggles the bottom one. This is how a drawer is driven from another machine over ssh, with no mouse: `pkill -USR1 -x drifter`. It is a trigger for the same code path the pointer uses, not a test hook.

## Frame loop

Drifter renders from the DRM VBlank event, never from a sleep.

After `queue_frame` the compositor holds a pending frame. Until the page flip event is read off the DRM fd and `frame_submitted` is called, every later `queue_frame` returns Ok and is silently dropped. The screen then freezes on frame 1 while the frame counter keeps climbing, so a frame count is never proof that anything moved. Only the screen is.

So the DrmDeviceNotifier runs as a calloop event source. On VBlank: `frame_submitted`, then draw the next frame. When nothing changed, `render_frame` reports empty, no flip is queued, and the loop blocks with no work until something wakes it.

The seat notifier is in the same loop. On PauseSession drawing stops and DRM master is released. On ActivateSession master is taken back, the compositor state and buffers are reset, and the screen is redrawn. Without this a VT switch away and back leaves a dead screen.

## Open items

Direct scanout needs the drm node. GbmFramebufferExporter::new takes an import_node as its second argument. With None, client buffers are never direct-scanned-out, they always go through the renderer. That is fine while nothing is on screen but a clear color. Once real clients land, pass Some(drm_node) or every window costs a composite pass it does not need.

Stack. Wayland, because the compositor tells each surface its size and scale, so a window can be told the screen is one screen through the normal path. On X11 apps read the root window size directly. Recommendation is Rust + Smithay: niri already proves a viewport-with-smooth-animation compositor works there, and a compositor crash loses every open window, so memory safety pays for itself.

A window demands the camera. Proposal: lock the camera to that window at 1.0 and disable zoom, pan and the hot corner until it releases. On multi-monitor, the monitor whose camera holds the window obeys.

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
