# Explorer punch list

Work left on Explorer (`drifter-explorer`). Do the steps in number order. Done steps leave this file. The numbers do not change.

## Phase 1: See where it stands

1. Run it under Drifter itself: the window frame, scale 1.5 and 2, theme changes. Fix what breaks.
   Waits on: nothing.
   - Ran on 114 under Drifter (4K, scale 1): the frame, title, menus, list and sidebar look right. On Blue's headless sway at scale 2 and 1.5 Explorer is sharp and lays out right, and a light/dark change in the config switches it live both ways within 1.5 s. Left: Drifter's own frame at scale 1.5 and 2.

## Phase 2: Groundwork

Later steps build on these.

2. Drawing cost: every hover redraws the whole window on the CPU, on the main thread. On the Pi it is slow. At 4K and scale 2 it will cost too much. Paint on a worker and redraw only what changed.
   Waits on: 1.
   - Built: a hover repaints only the old and new hover areas (rows, cells, tree lines, buttons, menus) inside a clip, and sends only that area to the compositor. A scroll moves the pixels already drawn and paints only the strip that comes into view. Tests check every case pixel for pixel against a full paint, at 1x and 2x. Hover cost on 114: 1.0 ms at 1x and 5.1 ms at 2x (was 9 and 38 ms); on Blue: 3.9 ms at 1x (was 41 ms). Full repaints (opening a folder, typing) still run on the main thread: 9 ms at 1x and 38 ms at 2x on 114. Left: move those to a worker if they feel slow on the Pixelbook.
3. Folders are read on the main thread, so a huge folder or a slow network drive freezes the window. Read them on a worker.
   Waits on: nothing.
   - Built: folders, the tree, columns, text previews and change checks read on worker threads. A folder that takes more than 50 ms shows "Loading…" and fills in later. A 100,000-file folder now blocks 60 ms, not 0.9 s. The last-folder check at start, the sidebar and drag targets no longer touch the disk. Still on the main thread: single-path checks when you act (a typed path, paste name checks, rename, new folder, Get Info). Left: a test on the Pixelbook.
4. Folder watching: Explorer checks only the open folder, every half second. The tree and the other columns do not refresh when a folder changes. Watch every shown folder with inotify.
   Waits on: 3.
   - Built: one inotify thread watches the open folder, every open folder in the tree and every column. Changes come in 120 ms after they settle, and refresh the list, tree and columns without blocking. The thread sleeps until a change or a new folder set. The half-second check stays as a backup for network drives. Left: run the Linux watcher test and try it on the Pixelbook.
5. Errors: an error shows in the status bar for 6 seconds and then disappears. It is easy to miss. Keep it until the user closes it.
   Waits on: nothing.
   - Built: errors show in a red banner under the toolbar, even with the status bar off. The banner stays until you click × or press Escape. It counts errors behind it ("1 more"). Left: a test on the Pixelbook.

## Phase 3: File jobs

6. Big copies show only "Copying…", with no progress bar and no cancel.
   Waits on: 5.
   - Built: copies, moves, Trash, delete and empty Trash show a row under the toolbar after 0.4 s. Each row names the item and target, has a progress bar, "1.3 GB of 4.7 GB, about 53 s left", and a × that stops the job. A stopped copy removes its half-done file. Closing the window with work running asks first. Copies keep the file date and use copy_file_range on Linux, falling back to plain reads when it stops short. Named pipes inside folders are made again, not read; sockets and devices are left out. Undo runs to the end and shows no stop button. Linux tests pass on Blue. Left: a test on the Pixelbook, with a USB stick for a cross-disk move.
7. Name clashes: when a pasted file has the same name as one already there, Explorer adds "2" to the name without asking. Ask Replace, Skip or Keep Both.
   Waits on: 6.
   - Built: a paste or drop onto names already there opens a sheet: Cancel, Skip, Replace, Keep Both (the default, Enter). One choice covers all clashes. Replace puts the old item in the Trash, and Undo puts it back. Left: a test on the Pixelbook.
8. Trash: on a USB stick, the file moves to the home Trash. The spec wants a `.Trash-UID` folder on that disk.
   Waits on: 6.
   - Built: a file on another disk goes to that disk's `.Trash/UID` (if it is a sticky folder) or `.Trash-UID`, with its path relative to the disk. It falls back to the home Trash only if neither can be made. The Trash view lists every mounted disk's Trash; Put Back, Delete and Empty Trash work on all of them. The disk top comes from the mount table, through symlinks. Trash folders must be real folders you own (and a sticky .Trash) before Explorer lists or empties them, and Put Back refuses paths that leave the disk. Empty Trash keeps going past an item it cannot remove. Trashing to a disk image was tested on the Mac; the merged view and Empty Trash across disks need Linux. Left: a test with a USB stick on the Pixelbook.
9. Trash: the Trash view has no "original location" column and no "date deleted" column.
   Waits on: 8.
   - Built: the Trash list shows Original Location (as ~/Music) and Date Deleted in place of Modified. Click either head to sort. Left: a test on the Pixelbook.
10. Compress and extract.
    Waits on: 6, 7.
    - Built: Compress makes "name.zip" (or "Archive.zip" for many items) next to the items. Extract Here opens zip, tar (gz, xz, bz2, zst), 7z and rar, plus single .gz/.xz/.bz2/.zst files. An archive with one item at the top lands as that item; others land in a folder named after the archive. Both run as jobs you can stop, and Undo removes the result. Uses bsdtar, with zip, unzip and tar as fallbacks; the package recommends libarchive-tools. Left: a test on the Pixelbook.

## Phase 4: Disks and previews

11. Drives: the list is read once at start. It has no mount, unmount or eject, no space bars, and it does not show disks that are not mounted.
    Waits on: 4.
    - Built: the list comes from lsblk plus network mounts, leaves out system disks, and shows disks that are not mounted (faded). It refreshes when a disk is plugged in, pulled out, mounted or unmounted. Each drive has a space bar (red past 90%). Click a disk that is not mounted to mount and open it; the eject button and the right-click menu (Open, Mount, Unmount, Eject) use udisksctl. Leaving a drive you are in goes Home first. Checked on 114: Computer with its bar, no stray gvfs entry. Left: plug a USB stick into 114 or the Pixelbook to test mount and eject.
12. Previews are not saved to disk, so each launch makes them again. Big photos are fully decoded on every launch.
    Waits on: nothing.
    - Built: previews go to the shared freedesktop cache (~/.cache/thumbnails, normal to xx-large), named by the md5 of the file URI and stamped with the file time, so other file managers share them. A changed file makes a new one. Files no tool can read are marked in fail/drifter-explorer and not tried again; a missing tool does not count. Checked on 114: the preview was saved once and read back on the next launch.

## Phase 5: Getting around

13. Path bar: you cannot click a part of the path to go there. A click starts edit mode.
    Waits on: nothing.
    - Built: each part of the path is a button with a hover look. A click opens that folder and selects the folder you came from. Clicking the current folder's name or the empty part of the bar starts edit mode, and right-click on a part does too. "…" stands for the hidden parts when the path is long. You can drop files onto a part. Checked on Blue: clicking "demo" opened it with Pictures selected.
14. Path bar: it has no folder dropdown.
    Waits on: 13.
    - Built: an arrow after each part of the path opens a menu of the folders inside it (up to 40, then "N more"), with the folder on your path checked. Pick one to open it. Checked on Blue.
15. A search box. There is only type-to-find.
    Waits on: 3.
    - Built: a search box in the toolbar (click it or press Ctrl+F). It finds names in the open folder and every folder below, on a worker thread, and does not follow folder links. Results come in as they are found, with a Folder column, up to 2000. Hidden files are searched only when they are shown. The status bar counts what was found. × or Escape ends the search, and opening another folder ends it too. Checked on Blue: "png" in ~/demo found the 4 pictures, and Escape brought the folder back.
16. Tabs.
    Waits on: 3, 4.
    - Built: a tab bar shows under the toolbar when there are two or more tabs. Each tab keeps its own folder, Back and Forward history, picks and scroll. Ctrl+T opens a new tab, Ctrl+W closes the tab (the window when it is the last), Ctrl+Q closes the window, Ctrl+Tab and Ctrl+Page Down/Up switch, and Alt+1 to 9 jump. "Open in New Tab" is in the folder and sidebar menus. A middle-click on a folder opens it in a tab behind, and a middle-click on a tab closes it. Click + or double-click the empty bar for a new tab. Right-click a tab for Close Other Tabs. Drop files on a tab to move them into its folder. If a tab's folder is gone, the tab opens the nearest folder above it. Checked on Blue. Left: tabs are not saved between launches.
17. Remembering the window size.
    Waits on: nothing.
    - Built: the window size and the maximized state are saved (window_size, maximized in explorer.toml) and used for the next window. Sizes from tiled, maximized or full-screen windows are not saved, so the free size stays. Bad sizes in the file are ignored. Checked on Blue: a floating window resized to 896×591 opened again at 896×591. Checked under Drifter on the trixie VM: the green button maximizes, the state is saved, and the next window opens maximized. Fixed there: a window that asked to be maximized before it showed had no size to go back to, so Restore sent 0×0 and Explorer kept the full size. Drifter now restores such a window to two thirds of the screen, centered (`window::restored`), and Explorer uses its saved free size when a configure has no size after a maximize. Checked: Restore gave 1274×698, centered, and saved maximized = false. Seen: on a maximized window the old button order put Restore inside the hot corner; the current order does not.
18. A different view for each folder.
    Waits on: nothing.
    - Built: each folder remembers its own view and sort (in explorer-views.toml, up to 1000 folders). Folders with none use the default view from Settings, and a change there moves them too. Columns view stays on while you browse, as in Finder. Each tab keeps its own view. Two windows saving at once merge, not overwrite. A hover on the tab bar repaints only that area, checked pixel for pixel. Checked on Blue: demo in Icons, Pictures in List, and Back brings Icons again.

## Phase 6: Mouse and keyboard

19. Tooltips: none of the icon buttons have a tooltip.
    Waits on: nothing.
    - Built: Back, Forward, Up, the view buttons, Settings, the search ×, path arrows, tab buttons, the banner ×, job stop and eject show a tooltip after 0.7 s, with the key in gray ("Enclosing Folder  Alt+Up"). Once one has shown, the next shows at once. A tab's tooltip shows its full path. Tips stay inside the window, flip above near the bottom, and hide on click, key, scroll or leave. Showing and hiding repaint only the tip area, checked pixel for pixel. Checked on Blue.
20. Scrollbars: they only show your position. You cannot drag them.
    Waits on: nothing.
    - Built: the scrollbars of the list, icons, sidebar and each column grow wider under the pointer. Drag the thumb to scroll. Click above or below it to move one page. A drag moves drawn pixels like the wheel does, and a hover repaints only the bar, checked pixel for pixel. Checked on Blue with 200 files.
21. Lasso selection with the mouse.
    Waits on: nothing.
    - Built: drag from empty space in List or Icons view to draw a box. It picks every item it touches, live. Ctrl or Shift adds to what is picked. Dragging past the top or bottom scrolls. A click without a drag still clears the picks. Checked on Blue: a box over five folders picked those five. Left: in List view the box starts only below the last row, since a press on a row drags files.
22. Changing list column widths.
    Waits on: nothing.
    - Built: drag the line at the left of a list column (Modified, Size, Kind, Folder, Date Deleted) to set its width. The line lights up under the pointer, and Name takes the rest but keeps 200 px. A double-click on the line goes back to the normal width. Widths are saved in explorer.toml. Checked on Blue.
23. Click-and-drag text selection in the rename and path fields.
    Waits on: 13.
    - Built: in the path, search and rename fields (List, Icons and Columns), a click puts the caret where you click, even when the text scrolls. A drag picks text, a double-click picks a word and a triple-click picks all. Shift+click extends. A click inside the rename box no longer ends the rename. Tests drive the mouse through rename in List and Icons and through the path field.
24. Keyboard: there is no visible focus ring.
    Waits on: nothing.
    - Built: after a key moves the focus, a ring shows on the focused item in List, Icons, Columns and Gallery, and on the focused sidebar line. With no focused item the whole file area gets the ring. A mouse click hides it. Checked on Blue.
25. Keyboard: you cannot use Tab to move between the sidebar and the files.
    Waits on: 24.
    - Built: Tab and Shift+Tab move the focus between the sidebar and the files. In the sidebar the focus starts on the open folder. Escape goes back to the files. A click in either pane moves the focus there. Checked on Blue.
26. Keyboard: the tree has no keyboard control.
    Waits on: 24.
    - Built: in the sidebar, Up, Down, Home, End and Page Up/Down move. Right opens a folder, then steps into it. Left closes it, then goes to its parent. Enter or Space opens the line (or mounts a drive). The Menu key opens the line's menu. The sidebar scrolls to keep the focus in view. Checked on Blue.
27. Screen readers: there is no Orca support. Activity and Settings have it.
    Waits on: 24, 25, 26.
    - Built: `drifter-explorer/src/access.rs` gives Orca the window through AccessKit, like Activity. The tree has the toolbar (Back, Forward, Enclosing Folder, the path, Search, the four views as radio buttons, Settings), the tabs, the sidebar as a tree (places, folders with level and open state, drives; an unmounted drive says "Not mounted" and Click mounts it), the files as a list box (name, then kind, size and date; picked items are selected), the status line, the last message as a polite live region, an open sheet as a dialog with its buttons, and an open menu with check items and shortcuts. Focus follows the rename box, the path or search field, an open menu or sheet, the sidebar line or the focused file. Click on a file opens it and Focus picks it; Click on a sidebar line goes there. The tree root name is shared with the painter (`look::tree_name`). Tests: `a_screen_reader_sees_the_files_and_can_open_and_pick_them`, `menus_sheets_and_renames_take_the_focus`. Checked on the trixie VM: AT-SPI shows the tree, a Click on "Docs" opened it, and Orca said "xp. List with 8 items", then "Docs. Folder, Today 04:19" and "Music." on Down.

## Phase 7: Other apps

28. Clipboard: copy to other apps goes through `wl-copy`. Paste from other apps does not work. Files you copy in another file manager cannot be pasted here. Use the Wayland clipboard directly.
    Waits on: 7.
    - Built: Explorer now owns the Wayland clipboard itself (`explorer/clip.rs`, a data device in `window.rs`); wl-copy is no longer used. Copy and Cut offer x-special/gnome-copied-files (with copy or cut), text/uri-list and plain text. Copy Path and text copied in a field offer plain text. When another app takes the clipboard, Explorer reads it: files from a gnome list or a uri list (file:// only, localhost allowed, comments skipped) become what Paste uses; plain text becomes what Ctrl+V types into the path, search and rename fields (first line only). Something Explorer cannot read clears Paste. Tests: three in `clip.rs`, `the_clipboard_is_shared_with_other_apps`. Checked on the trixie VM: Ctrl+C on note1.txt gave all five kinds to wl-paste; a uri list from wl-copy pasted "far away.txt" into the folder; text from wl-copy went into the search field.
29. Drag and drop: it works only inside Explorer. You cannot drag to or from other apps.
    Waits on: 28 (same Wayland data device code).
    - Built: a file drag becomes a real Wayland drag as soon as it moves (`lift` in `window.rs`), offering the same kinds as Copy. Drops come back through the data device for every window, Explorer's own too: our own drag moves (Ctrl copies), a drag from another app copies, and the drag card and the target ring follow the drop motion. Files from other apps are read from a gnome list or a uri list. Empty space or a file row now means "this folder" for a drop, unless the items already live here (`drop_at`). Fixed on the way: sctk's `ReadPipe::into_raw_fd` closes the fd it returns, so pipes go through `OwnedFd`. Test: `files_from_elsewhere_drop_into_the_open_folder`. Checked on the trixie VM with two Explorer processes: note2.txt dragged from /tmp/xp into the /tmp/far window was copied there, and note3.txt dragged onto Docs in the same window moved, with the card and the ring shown. Left: Drifter draws no drag icon for client drags, so outside Explorer only the pointer shows; Drifter's screenshot drag offers text/uri-list, so it should drop into Explorer too (not checked).
30. Open With: there is no "Other app…" picker and no "Always use this app".
    Waits on: nothing.
    - Built: Open With ends with "Other App…". It opens a sheet that lists the apps for this kind first, a line, then every installed app by name, with icons. Click picks, a double-click or Open opens, arrows, Page keys, Home and End move, the wheel scrolls, and Space ticks "Always use this app for … files". With the tick, Explorer writes the kind into [Default Applications] of the user's mimeapps.list (`desktop::set_defaults`, which `set_default` now uses too), forgets its cached app list, and says so in the status bar. Apps marked Terminal=true now run inside the chosen terminal (`prefs::terminal_run`), for Open, Open With and the chooser. Fixed on the way: the screen reader's sheet buttons used Dismiss, which only clears alerts; they now use CloseSheet. Test: `another_app_can_open_a_file_and_become_the_default`. Checked on the trixie VM: the chooser listed Text Editor and Vim first, Vim with the tick wrote text/plain=vim.desktop; and the status said "TXT text files now open with Vim"; Open With Vim started `foot vim /tmp/xp/note1.txt`.
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
