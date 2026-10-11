
  Easiest with the biggest visual change at the top.

  Tier 1: small changes, big visual effect
  1. Fade every panel in and out. 14 panels appear at full opacity and vanish in one frame:
     - menus, tooltips, Spotlight, Launchpad, Control Center
     - Notes, Charges, Glance, HUD, notifications
     - the window switcher, the power alert, and the password prompt with its dim.
     - Fix it in one place, render.rs:820-900. Keep each panel's last image and draw it at an eased opacity, scaling from 0.96 to 1. When a panel closes, keep drawing it for about 120 ms while it fades out. The 71 places that close panels need no changes.
  2. Other menus copy the desktop menu. The dock, tray, title bar, Launchpad and screenshot menus should grow from the click point and get its shadow. Reuse deskmenu.rs:333.
  3. Add the missing shadows. Menus, tooltips, Control Center, Notes, Charges, Glance and the HUD have none. Search and Launchpad have one.
  4. Bug: long window titles are squashed sideways, not cut short (windows.rs:342). Crop the title and fade its edge instead.
  5. Notifications should slide in from the right. The cards below should glide up when one closes.
  6. HUD should fade out instead of vanishing, and its level bar should ease.
  7. The snap preview is 3 px white bars over a 12% white fill (render.rs:840). You cannot see it on light wallpapers. Use a rounded, frosted, accent-colored shape that eases from the window to the zone.
  8. Focus change happens in one frame. The title bar art, shadow (0.105 to 0.2) and title dim (0.55 to 1) all jump. Ease them over 150 ms.
  9. Title bar buttons and titles vanish at one fixed zoom level. Fade them out over a zoom range instead.
  10. Dock icons do not react to hover. Only the Apps logo does. Add an eased 3-4 px lift. Running dots should fade in and out.
  11. The Restart and Shut Down alert pops in, and its countdown changes once a second as text. Make it grow in and show a smooth bar that drains.

  Tier 2: about a day each

  12. Launchpad, Spotlight, Control Center, Glance and notifications have no hover highlight. Draw one rounded highlight on the GPU over the existing image. It needs no repaint, so it does not paint on the input path. Let it slide between rows.
  13. Spotlight's height jumps on every key press. Ease it.
  14. Launchpad:
      - icons zoom in when it opens
      - pages slide when they change
      - a touchpad swipe changes the page.
  15. Switcher: slide the selection ring between tiles instead of jumping.
  16. Bug: a window that an app closes itself vanishes in one frame (windows.rs:779). This happens when you type exit or use File > Quit. Only the close button and the close shortcut animate.
  17. Fullscreen jumps in and out (windows.rs:610-670). Reuse the resize animation.
  18. The dock does not glide. Icons and the dock width jump when apps open, close or minimize.
  19. Settings and Activity have no animation. Move Explorer's Fade and Glide into drifter-core and use them there too. That gives toggle and page crossfades and smooth wheel scrolling.
  20. Two menu styles. The panel menus use corner radius 8 and 24 px rows. The desktop menu uses radius 10 and 26 px rows. Use one style.
  21. Hardcoded colors and labels:
      - The fallback tray icon is a fixed gray circle (tray.rs:173).
      - The hot-corner glow is a fixed peach color, not the accent color (start.rs:347).
      - A tray tooltip can show the raw D-Bus name (tray.rs:163).

  Tier 3: shaders, the biggest effect, the most work

  22. Move the right-click ripple to the GPU. Today a CPU water simulation runs on a thread every 16 ms and copies the screen back from the GPU on each right-click (deskmenu.rs:136-222). A GLSL pass would be smoother on the Pixelbook and could cover the whole screen.
  23. Dark/Light switch as a circle that grows from the click point. Today it is a plain fade (follow.rs:557). Use the same effect for a wallpaper change.
  24. Genie minimize. Today the window shrinks in a straight line to the dock. A shader can bend it into the dock slot.
  25. Wallpaper shaders that follow the pointer. Add a pointer position input so caustics bend and the aurora glows near the cursor. Run it only while the pointer moves on a bare desktop, to keep the low-redraw plan.

  Searched for 1 pattern, ran 2 shell commands

Most of these should not get their own switch. They should follow one shared animation setting. Only a few are matters of taste and need their own control.

First, fix the settings side
- Today, window animation speed comes from the wallpaper motion slider (render.rs:1638). That is the wrong place for it.
- Add an "Animation" group to the Motion page:
  - Animation speed: Off / Calm / Full. Windows, panels, menus, the dock and notifications all follow it.
  - Reduce motion (Accessibility) keeps fades and turns off movement, scale and bounce.
- Every item below follows these two settings unless it says otherwise.

Tier 1: small changes, big visual effect
1. Fade every panel in and out: the shared setting only.
2. Other menus copy the desktop menu: the shared setting only.
3. Add the missing shadows: no setting. This only makes the panels look the same.
4. Bug: long titles are squashed: no setting.
5. Notification slide-in and glide: the shared setting. Add Position: top right / bottom right next to "Notification seconds".
6. HUD fade and eased bar: the shared setting only.
7. Snap preview: no setting. It uses the accent color.
8. Focus change crossfade: the shared setting. Add an "Inactive windows: dim slightly / dim more / don't dim" choice under Appearance. Today the 0.55 dim is fixed.
9. Title bar fade by zoom: no setting.
10. Dock hover: new setting, Dock hover: Off / Lift / Magnify, on the Dock page. People feel strongly about magnify. Default: Lift. Magnify needs its own size slider.
11. Power alert grow and drain bar: the shared setting. The countdown length (power::COUNTDOWN) is fixed. It can become a Power page slider (10-60 s).

Tier 2: about a day each

12. Hover highlight that slides between rows: the shared setting. Reduce motion makes it fade instead of slide.
13. Spotlight height ease: the shared setting only.
14. Launchpad:
    - The zoom-in and page slide follow the shared setting.
    - Touchpad swipe has no setting.
15. Switcher ring slide: the shared setting only.
16. Bug: app-closed windows vanish: no setting.
17. Fullscreen animation: the shared setting only.
18. Dock glide: the shared setting only.
19. Settings and Activity get Explorer's Fade and Glide:
    - The fades follow the shared setting.
    - Add Smooth scrolling: On / Off under Mouse. Some people dislike wheel glide.
    - Explorer reads the same key.
20. One menu style: no setting.
21. Hardcoded colors:
    - The tray fallback icon and the hot-corner glow follow the accent color. No setting.
    - Optional: Hot corner glow: On / Off in the Hot corner group.

Tier 3: shaders, the biggest effect, the most work

22. GPU right-click ripple: new setting, Desktop click ripple: On / Off. Add a strength slider if wanted. It sits under Wallpaper. Reduce motion turns it off.
23. Circular reveal for Dark/Light and wallpaper changes: new setting, Theme change: Fade / Reveal. Reduce motion forces Fade.
24. Genie minimize: new setting, Minimize effect: Scale / Genie on the Motion page, like macOS. Reduce motion forces a fade.
25. Wallpaper shaders that follow the pointer: new setting, Wallpaper reacts to pointer: On / Off under Wallpaper.
    - Default Off on weak GPUs like the Pixelbook. Default On on the RTX box.
    - Off when "Wallpaper motion" is Still.

New settings in total: one Animation group and 9 controls.
- Notification position
- Inactive window dim
- Dock hover, with a magnify size
- Alert countdown
- Smooth scrolling
- Hot corner glow
- Click ripple
- Theme change
- Minimize effect
- Pointer-reactive wallpaper
