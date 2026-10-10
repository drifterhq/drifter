# Explorer punch list

Work left on Explorer (`drifter-explorer`). Do the steps in number order. Done steps leave this file. The numbers do not change.

## Phase 1: See where it stands

1. Run it under Drifter itself: the window frame, scale 1.5 and 2, theme changes. Fix what breaks.
   Waits on: nothing.

## Phase 2: Groundwork

Later steps build on these.

2. Drawing cost: every hover redraws the whole window on the CPU, on the main thread. On the Pi it is slow. At 4K and scale 2 it will cost too much. Paint on a worker and redraw only what changed.
   Waits on: 1.
3. Folders are read on the main thread, so a huge folder or a slow network drive freezes the window. Read them on a worker.
   Waits on: nothing.
   - Built: folders, the tree, columns, text previews and change checks read on worker threads. A folder that takes more than 50 ms shows "Loading…" and fills in later. A 100,000-file folder now blocks 60 ms, not 0.9 s. Left: a test on the Pixelbook.
4. Folder watching: Explorer checks only the open folder, every half second. The tree and the other columns do not refresh when a folder changes. Watch every shown folder with inotify.
   Waits on: 3.
5. Errors: an error shows in the status bar for 6 seconds and then disappears. It is easy to miss. Keep it until the user closes it.
   Waits on: nothing.
   - Built: errors show in a red banner under the toolbar, even with the status bar off. The banner stays until you click × or press Escape. It counts errors behind it ("1 more"). Left: a test on the Pixelbook.

## Phase 3: File jobs

6. Big copies show only "Copying…", with no progress bar and no cancel.
   Waits on: 5.
7. Name clashes: when a pasted file has the same name as one already there, Explorer adds "2" to the name without asking. Ask Replace, Skip or Keep Both.
   Waits on: 6.
8. Trash: on a USB stick, the file moves to the home Trash. The spec wants a `.Trash-UID` folder on that disk.
   Waits on: 6.
9. Trash: the Trash view has no "original location" column and no "date deleted" column.
   Waits on: 8.
10. Compress and extract.
    Waits on: 6, 7.

## Phase 4: Disks and previews

11. Drives: the list is read once at start. It has no mount, unmount or eject, no space bars, and it does not show disks that are not mounted.
    Waits on: 4.
12. Previews are not saved to disk, so each launch makes them again. Big photos are fully decoded on every launch.
    Waits on: nothing.

## Phase 5: Getting around

13. Path bar: you cannot click a part of the path to go there. A click starts edit mode.
    Waits on: nothing.
14. Path bar: it has no folder dropdown.
    Waits on: 13.
15. A search box. There is only type-to-find.
    Waits on: 3.
16. Tabs.
    Waits on: 3, 4.
17. Remembering the window size.
    Waits on: nothing.
18. A different view for each folder.
    Waits on: nothing.

## Phase 6: Mouse and keyboard

19. Tooltips: none of the icon buttons have a tooltip.
    Waits on: nothing.
20. Scrollbars: they only show your position. You cannot drag them.
    Waits on: nothing.
21. Lasso selection with the mouse.
    Waits on: nothing.
22. Changing list column widths.
    Waits on: nothing.
23. Click-and-drag text selection in the rename and path fields.
    Waits on: 13.
24. Keyboard: there is no visible focus ring.
    Waits on: nothing.
25. Keyboard: you cannot use Tab to move between the sidebar and the files.
    Waits on: 24.
26. Keyboard: the tree has no keyboard control.
    Waits on: 24.
27. Screen readers: there is no Orca support. Activity and Settings have it.
    Waits on: 24, 25, 26.

## Phase 7: Other apps

28. Clipboard: copy to other apps goes through `wl-copy`. Paste from other apps does not work. Files you copy in another file manager cannot be pasted here. Use the Wayland clipboard directly.
    Waits on: 7.
29. Drag and drop: it works only inside Explorer. You cannot drag to or from other apps.
    Waits on: 28 (same Wayland data device code).
30. Open With: there is no "Other app…" picker and no "Always use this app".
    Waits on: nothing.
31. Get Info: it shows no total folder size and has no way to change anything.
    Waits on: 3.

## Phase 8: Looks

32. Flat: nothing has depth or a 3D feel, and Drifter's identity is 3D and skeuomorphic.
    Waits on: 2.
33. Animation: menus, sheets and view changes snap in.
    Waits on: 2, 32.
34. Animation: hover and selection do not fade.
    Waits on: 2, 32.
35. Animation: scrolling does not glide.
    Waits on: 2.
36. Gallery: the strip has no names.
    Waits on: nothing.
37. Gallery: small pictures look tiny in the big preview.
    Waits on: nothing.
38. Empty folders show gray text and nothing else.
    Waits on: 32.

## Phase 9: Make it the default

39. Offer Explorer for Files in Welcome and Default Apps.
    Waits on: all steps above.
