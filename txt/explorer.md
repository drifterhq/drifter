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
    - Built: a tab bar shows under the toolbar when there are two or more tabs. Each tab keeps its own folder, Back and Forward history, picks and scroll. Ctrl+T opens a new tab, Ctrl+W closes the tab (the window when it is the last), Ctrl+Q closes the window, Ctrl+Tab and Ctrl+Page Down/Up switch, and Alt+1 to 9 jump. "Open in New Tab" is in the folder and sidebar menus. A middle-click on a folder opens it in a tab behind, and a middle-click on a tab closes it. Click + or double-click the empty bar for a new tab. Right-click a tab for Close Other Tabs. Drop files on a tab to move them into its folder. If a tab's folder is gone, the tab opens the nearest folder above it. Checked on Blue. Saved since: the tabs are kept in explorer.toml (`tabs`, `tab`) and come back when Explorer starts in the last folder with no folder asked for; tabs whose folder is gone are left out (`tabs_come_back_next_time`).
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
    - Built: drag from empty space in List or Icons view to draw a box. It picks every item it touches, live. Ctrl or Shift adds to what is picked. Dragging past the top or bottom scrolls. A click without a drag still clears the picks. Checked on Blue: a box over five folders picked those five. Left: in List view the box starts only below the last row, since a press on a row drags files. Later: in List view a box can also start from the blank part of a row, right of the name (`row_blank`); a click there still picks the row, and a drag picks what the box touches. Checked on the trixie VM: a drag from right of file003 down to file007 picked those five.
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
    - Built: a file drag becomes a real Wayland drag as soon as it moves (`lift` in `window.rs`), offering the same kinds as Copy. Drops come back through the data device for every window, Explorer's own too: our own drag moves (Ctrl copies), a drag from another app copies, and the drag card and the target ring follow the drop motion. Files from other apps are read from a gnome list or a uri list. Empty space or a file row now means "this folder" for a drop, unless the items already live here (`drop_at`). Fixed on the way: sctk's `ReadPipe::into_raw_fd` closes the fd it returns, so pipes go through `OwnedFd`. Test: `files_from_elsewhere_drop_into_the_open_folder`. Checked on the trixie VM with two Explorer processes: note2.txt dragged from /tmp/xp into the /tmp/far window was copied there, and note3.txt dragged onto Docs in the same window moved, with the card and the ring shown. Later: Drifter now draws the icon surface a client gives when it starts a drag (`ClientDndGrabHandler::started`, `drag_icon`), and Explorer sends its drag card as that icon, so the card follows the pointer across other windows and the desktop (checked on the trixie VM). Later: the screenshot thumbnail drag now drops into Explorer. Two Drifter fixes made it work: the release of the press that started the drag was eaten, so the drag never ended (it now goes on to the drag while `drag_shot` is set), and its uri list was not percent-encoded, so names with spaces broke (it now uses `clip::uri_list`). Checked on the trixie VM: "Screenshot 2026-10-10 at 06.08.25.png" landed in /tmp/shots-in.
30. Open With: there is no "Other app…" picker and no "Always use this app".
    Waits on: nothing.
    - Built: Open With ends with "Other App…". It opens a sheet that lists the apps for this kind first, a line, then every installed app by name, with icons. Click picks, a double-click or Open opens, arrows, Page keys, Home and End move, the wheel scrolls, and Space ticks "Always use this app for … files". With the tick, Explorer writes the kind into [Default Applications] of the user's mimeapps.list (`desktop::set_defaults`, which `set_default` now uses too), forgets its cached app list, and says so in the status bar. Apps marked Terminal=true now run inside the chosen terminal (`prefs::terminal_run`), for Open, Open With and the chooser. Fixed on the way: the screen reader's sheet buttons used Dismiss, which only clears alerts; they now use CloseSheet. Test: `another_app_can_open_a_file_and_become_the_default`. Checked on the trixie VM: the chooser listed Text Editor and Vim first, Vim with the tick wrote text/plain=vim.desktop; and the status said "TXT text files now open with Vim"; Open With Vim started `foot vim /tmp/xp/note1.txt`.
31. Get Info: it shows no total folder size and has no way to change anything.
    Waits on: 3.
    - Built: Get Info on a folder counts its whole tree in the background (`ops::folder_size`, Effect::Measure); the Size row says "N items, counting…" and then "12 MB in 340 items". Get Info can now change things: a Locked switch (takes away every write bit, and unlocking gives back the ones it took), a "Can run as a program" switch for files (adds x where r is set, or removes all x), and for files an "Opens with" row with a Change… button that opens the app chooser with "Always use this app" ticked. Errors show as an alert. The screen reader sees the total, the Change… button and both switches as check boxes. Test: `get_info_counts_folders_and_changes_access`. Checked on the trixie VM: note1.txt showed Opens with Text Editor, Change…, Locked and Can run as a program; Docs showed "3 bytes in 1 item", and Locked turned 775 into 555 and back into 775. Later: a click on the name in Get Info turns it into a field with the name part picked; Enter renames (with undo, through the same `ops::rename` as F2) and Get Info follows the new name; Escape stops editing; Done or a click elsewhere saves; a bad name keeps the field open and says why. The screen reader gets a Rename button and a text field. Test: `get_info_can_rename`. Checked on the trixie VM: note5.txt became five.txt.

## Phase 8: Looks

32. Flat: nothing has depth or a 3D feel, and Drifter's identity is 3D and skeuomorphic.
    Waits on: 2.
    - Built: depth in the style of Drifter's Welcome keys (`raised`, `pressed`, `sunk`, `glossy` in look.rs). The toolbar has a soft top-to-bottom shade. Back, Forward, Up and the settings button are raised keys: a soft shadow under them, a shaded face, a dark rim and a thin top highlight; hover lightens the face. The four views are one raised segmented control with thin dividers, and the chosen view is pressed in with a darker accent at the top. The path and search fields are sunk: a dark inner line at the top and a light line under them. Every accent fill (picked rows, icons, columns, gallery tiles, the sidebar line you are in, menus, the menu bar, the Settings tabs and the app chooser) is a glossy accent gradient with a thin highlight. Partial repaints still match full paints (the repaint tests pass). Checked on the trixie VM in the dark theme. Later, in the light theme: white page icons nearly vanished on the white list, so pale theme icons (`pale`) now get a faint dark edge from their own shape (`silhouette`) in light mode only. Checked on the trixie VM in light: the toolbar, the picked row and Get Info all read well.
33. Animation: menus, sheets and view changes snap in.
    Waits on: 2, 32.
    - Built: a menu or sheet that opens fades in over 140 ms with an ease-out; a menu drops 4 px into place and a sheet rises 10 px (`layered` paints it into its own layer and stamps it at the eased alpha, `model.pop`). A change of view or folder crossfades from the last frame over 160 ms (the window keeps the old image in `swap` and stamps it over the new full paint). The window notices the openings in `animate` and keeps drawing every 16 ms only while something moves. Look code only reads `model.pop`, which stays 1.0 in tests, so every repaint test stays exact. Test: `menus_and_sheets_fade_in`. Checked on the trixie VM: switching to Icons and back and opening the space menu all end on a clean frame.
34. Animation: hover and selection do not fade.
    Waits on: 2, 32.
    - Built: when the hover moves, the window keeps the old and the new pixels of the repainted area and blends them over 120 ms (`Fade` in window.rs), so a hover glows in and out without painting again. A tooltip fades in the same way. Menus and drags stay instant. Any full repaint or scroll first puts the final pixels back, so nothing is left half faded. A click that changes the picked items crossfades the frame over 120 ms; picks from the keyboard stay instant so fast arrow keys do not smear. Checked on the trixie VM: after sweeping the pointer over the rows only the row under it was lit, and a click picked Music cleanly.
35. Animation: scrolling does not glide.
    Waits on: 2.
    - Built: a mouse wheel click (a discrete axis step) now moves 48 px and glides there over 150 ms with an ease-out (`Glide` in window.rs). Each frame moves the view by whole pixels through the same blit shifts as plain scrolling, so it costs no full repaint. More clicks during a glide add to its target. Touchpad scrolling stays direct. Any other change to the scroll (keys, the rail, the other pane) ends the glide at once. Checked on the trixie VM in a folder of 200 files: three clicks down showed file007 at the top, one click up file005, with clean frames.
36. Gallery: the strip has no names.
    Waits on: nothing.
    - Built: the strip is 136 px tall; each tile shows its picture at the top and its name under it in the small font, cut to fit, white on a picked tile. Checked on the trixie VM: blueman.p…, copyq.png and debian-log… under their pictures.
37. Gallery: small pictures look tiny in the big preview.
    Waits on: nothing.
    - Built: the big preview now grows a small picture to fill the space, up to 4 times its size (`grow_fit`); big pictures still shrink to fit. Pictures with see-through pixels no longer get a box shadow, in the preview or in tiles (`solid`), so logos and icons do not sit on a dark square. Test: `small_pictures_grow_in_the_gallery_but_not_without_end`. Checked on the trixie VM: a 48 px PNG showed at 192 px with no box behind it.
38. Empty folders show gray text and nothing else.
    Waits on: 32.
    - Built: an empty folder now shows its own folder icon, large and faded, then "This folder is empty" in normal ink and a hint: "Drop files here, or press Shift+Ctrl+N for a new folder". The empty Trash shows the Trash icon and "Things you move to the Trash wait here until you empty it". A search with no results shows a large magnifier and "Check the spelling or try fewer letters". Searching and loading keep plain text. In Gallery the empty state sits in the picture area. Checked on the trixie VM for all three.

## Phase 9: Make it the default

39. Offer Explorer for Files in Welcome and Default Apps.
    Waits on: all steps above.
    - Built: Drifter's own apps that sit next to the running program are now one list (`desktop::own_apps`: Activity and Explorer). `installed()` adds them, so Settings' app picker and Spotlight list Explorer even when no .desktop file is installed. `serving(Role::Files)` puts Explorer first (`ours_first`), so Welcome's Files choices start with it. Drifter's launcher finds own apps by id too, so a Files default of drifter-explorer opens it. Test: `drifter_apps_join_the_lists_and_explorer_leads_the_file_managers`. Checked on the trixie VM: Welcome's Files choices were Explorer, PCMan, PCManFM-Qt, Thunar (Get), Files (Get); picking Explorer saved files = "drifter-explorer" and inode/directory=drifter-explorer.desktop; Spotlight found Explorer and opened it.

## Phase 10: Polish found after the list

40. Quick Look: there is no quick way to see a file without opening it.
    Waits on: nothing.
    - Built: Space on a picked file opens a large preview sheet (`Sheet::Look`): the picture grown to fit (up to 4 times), or the first lines of a text file in the body font, or the big icon; the name sits on top and kind, size and date under it. Left/Up and Right/Down step to the next file while it stays open, Enter opens the file, Space or Escape closes, a click outside closes. A space typed while finding a file by name stays part of the name. Also in the item menu and the Keys sheet. Test: `space_looks_at_a_file_and_arrows_step_through`. Checked on the trixie VM: list.txt showed its lines, and Down showed shot.png.
41. Big folders: with the screen reader bus on, a folder of 20,000 files kept one core at 100% even when idle.
    Waits on: 27.
    - Built: profiling (perf on the trixie VM) showed AccessKit's AT-SPI layer walking siblings for every list item on each update. The screen reader tree now holds only the 200 files around the focus (`near`); each file carries its place in the whole list and the list its full size, so Orca can still say "5 of 20000". Also fixed: Home and End with nothing picked went the wrong way (End picked the first file). Tests: `big_folders_send_only_the_rows_near_the_focus`, `home_and_end_go_the_right_way_with_nothing_picked`. Checked on the trixie VM with 20,000 files: idle CPU 0, End then showed rows 19800–19999 to AT-SPI, Ctrl+A, view switch and a search for "1999" (12 found) were quick.
42. Scaling: at 150% a saved 1100×700 window did not fit the 1280×720 screen, so the status bar was cut off.
    Waits on: 17.
    - Built: Drifter now sends real window bounds (`window_room`: the work zone at the current zoom, minus the frame) in the first configure instead of 0×0, so every app can fit itself. Explorer keeps the last bounds it got and shrinks its start size to them; a size squeezed this way is not saved, so the size the user chose comes back on a bigger screen. Pictures and text were already sharp at 150% (Explorer draws at 2× and Drifter scales down). Checked on the trixie VM at 150%: the window fit above the dock, and explorer.toml kept window_size = [1100, 700].
43. Copy wording: after Ctrl+C the status bar said "Copied 1 item" before anything was pasted, which reads like a finished file copy.
    Waits on: 28.
    - Built: Copy now says "1 item ready to paste" and Cut "1 item ready to move" (de, es, fr too). Checked on the trixie VM with a 1.6 GB file: the progress bar showed "Copying movie.bin to into, 1.4 GB of 1.6 GB", the copy took about 2 seconds, and the status bar now read "1 item ready to paste" after Ctrl+C. Stop mid-copy is covered by `a_stopped_copy_leaves_no_half_file`; the copy was too quick to stop by hand.
44. Other languages: key names stayed English (Ctrl, Shift, Delete, Space), and German read "15 Byte in 8 Objekte".
    Waits on: nothing.
    - Built: `key_words` turns key labels into the reader's words in menus, tooltips and the Keys sheet: Strg, Umschalt, Entf, Leertaste, Eingabe, Rücktaste in German; Mayús, Supr, Espacio in Spanish; Maj, Suppr, Espace in French. Arrow keys show as ↑ ↓ ← →, and "Ctrl+1 to 4" became "Ctrl+1–4". The Delete key has its own word ("Del") so it does not clash with the Delete action. German folder sizes now read "15 Byte (8 Objekte)". Menus widen to fit. Test: `key_names_read_well`. Checked on the trixie VM in German: the file menu, the View menu and Get Info all fit.
45. Other scripts: Japanese, Arabic and emoji names showed as empty boxes, because text used one font file with no fallback.
    Waits on: nothing.
    - Built: Drifter's core text code (`text.rs`, used by every Drifter app) now splits text into runs by font and script. A letter the main font lacks gets a fallback font from fontconfig (`fc-match :charset=…`), loaded once per process and shared by all sizes; the answer is kept per letter, and fonts already loaded are tried first so new letters rarely start a lookup. Each run is shaped with its own script, and Arabic, Hebrew, Syriac, Thaana and N'Ko runs right to left. fontconfig's index carries a variable-font instance in its high bits, which is masked off. Test: `missing_letters_come_from_another_font`. Checked on the trixie VM: "日本語のファイル.txt" and "مرحبا.txt" now draw (Droid Sans Fallback, DejaVu). Later (step 46): color emoji.
46. Color emoji: after step 45, emoji were boxes or gray shapes, and later emoji in a name stayed boxes.
    Waits on: 45.
    - Built: glyphs come from color outlines (COLR) or color bitmaps (CBDT, scaled by swash) when the font has them, are kept as premultiplied RGBA and drawn in their own colors; plain fonts still use outlines and take the text color. Runs got two fixes: a neutral character (an emoji, a dot) joins a run only when that run's font has it and the run is not right to left, and a space stays in the main font except inside right-to-left text (an emoji font's space is very wide). Test: `missing_letters_come_from_another_font` grew emoji cases. Checked on the trixie VM after installing fonts-noto-color-emoji there: "🎉 party 🎈.txt", "notes ✅ done.txt" and "日本語 🍣.txt" draw in color with normal spacing, and "مرحبا.txt" keeps its dot next to "txt". Without an emoji font, emoji still fall back to whatever fontconfig finds (Symbola draws them in one color).
47. Speed and memory check (no change needed).
    Waits on: nothing.
    - Checked on the trixie VM: the first frame shows 81 ms after start in a small folder and in the home folder, 107 ms in /usr/share/icons. Memory starts at 23 MB, grows only while new folders are visited (34 MB after 45 different folders), and stays flat at 34 MB over 45 more trips into and out of the same folder. Drifter's own title bar draws "日本語 🍣" with the fallback fonts from steps 45 and 46.
48. Recent files: there was no quick way back to a file you just opened.
    Waits on: nothing.
    - Built: a Recent section in the sidebar lists the 6 newest files from the shared recently-used.xbel (the list GTK apps use), leaving out missing, hidden and trashed files. A click shows the file in its folder, a double-click or Enter opens it, and right-click offers Open, Show in Folder and Copy Path. Explorer now adds the files it opens (Open, Open With, the app chooser) to that list in the standard format, updating the time of a file already there and keeping at most 500 entries; it re-reads the list when it changes. A Settings switch turns the section off. Tests never write the real list (`recent_list` is empty in tests). Tests: `the_newest_files_come_first_and_odd_entries_are_skipped`, `opening_a_file_adds_it_or_moves_it_to_the_top`, `recent_files_show_in_the_sidebar_and_lead_to_their_folder`. Checked on the trixie VM: opening list.txt added it under Recent, GTK's own RecentManager read the entry back (text/plain, app "Explorer"), and a click on it from Home went to /tmp/look with list.txt picked.
49. Batch rename: with several items picked, F2 renamed only the focused one.
    Waits on: nothing.
    - Built: F2 or "Rename N Items…" with more than one item picked opens a sheet with three rules: Replace text (find and replace in the whole name), Add text (before or after the name, keeping the extension), and Name and number (a name plus a number padded to two or more digits, from a chosen start, keeping each extension). A live list shows the first five old and new names, and the sheet names any problem: an empty name, a slash, two items with one name, a name already taken outside the set, or no change. Tab moves between the fields, Enter renames. The renames run in two steps through hidden spare names (`ops::rename_all`), so a chain like "shot 01" → "shot 02" → "shot 03" works; undo puts every name back the same way (`Undo::RenamedAll`), and the renamed items stay picked. Test: `many_items_rename_at_once_and_undo_together`. Checked on the trixie VM: four IMG_45xx.jpg files became "beach 01.jpg" to "beach 04.jpg", and Ctrl+Z put the old names back.
50. Screen reader on the newer sheets: batch rename and Quick Look gave Orca only a title and a Done button.
    Waits on: 27, 40, 49.
    - Built: the rename sheet now reads out its rules as radio buttons, its fields as text inputs (the one being typed in has the focus), Before/After for Add text, every old → new name in a "New names" list, any problem as a polite live line, and Cancel and Rename (dimmed while nothing would change). Quick Look is titled "Quick Look: name" and reads the kind, size and date, the text of a text file as a document, and offers Open and Done. Test: `the_rename_sheet_reads_out_its_rule_fields_and_preview`. Checked on the trixie VM through AT-SPI: the dialog showed Replace text checked, Find focused, and "a.txt → a.tt" after typing x.
51. Search results: there was no way to see a found file in its folder, and Open from Quick Look left the preview on top.
    Waits on: 40, 48.
    - Built: a single found item's menu offers "Show in Folder" (the same Reveal as Recent). It ends the search and opens the item's folder with the item picked; when that folder is the one being searched, the full listing comes back instead of the result list (found while checking live). Open from Quick Look now closes the preview first. All four Settings tabs were checked with the new Recent switch and still fit. Test: `search_results_can_show_their_folder`. Checked on the trixie VM: a search for "note2" in /tmp/xp, then Show in Folder, showed all seven items with note2.txt picked.
52. Code health: clippy stopped on three errors in the wallpaper shader before it reached Explorer.
    Waits on: nothing.
    - Built: the shader uses the TAU constant instead of 6.28 and 6.2832 (a change too small to see), and the clippy warnings in Explorer, its window and the text code are fixed: fixed-size chunks use `as_chunks`, one identical `if` branch pair is joined, one nested `if let` is collapsed, one redundant rebinding is gone, the drag icon type has a name, and a modulo check uses `is_multiple_of`. `cargo clippy -p drifter-core -p drifter-explorer` now reports nothing for these files; the full suite still passes.
53. Removable drives, end to end; and a lost first click in Drifter.
    Waits on: 13.
    - Checked on the trixie VM with a 64 MB FAT image plugged in as a USB stick: PHOTOS showed in Drives, a click mounted it through udisks at /media/dh/PHOTOS and opened it (note.txt, 67 MB free), Shift+Ctrl+N made a folder that reached the stick, and the eject button unmounted and powered it off, with Explorer going back to Home. Mounting needs the active seat session: started over ssh, polkit refuses with "Not authorized", which is right; the VM test now starts Drifter by typing on tty1 and opens Explorer from Spotlight.
    - Fixed in Drifter: pointer focus was only updated when the pointer moved, so a window that opened or closed under a still pointer missed the next click (Explorer opened from Spotlight ignored the first click on its list). A press now first refreshes the pointer focus when the surface under the pointer changed (`tell_button`). Checked on the trixie VM: the first click on a just-opened Explorer window picked the row under the pointer.
54. Thumbnails of files still being written: a video that appeared while Explorer was open kept a plain icon for good.
    Waits on: 6.
    - Built: Explorer tried the thumbnail while ffmpeg was still writing the file, failed, and saved the failure with the file's final time, so it never tried again; a finished download would do the same. Now a failure is not saved for a file changed in the last 30 seconds, a saved failure that was written within 30 seconds of the file's change is tried again (this also clears old ones), and when a listed file's time or size changes its thumbnail and any failure in memory are dropped and fetched again (`refresh_art`). Test: `a_failure_noted_while_a_file_was_still_new_is_tried_again`. Checked on the trixie VM: test.mp4 (failed before) and fresh.mp4 (written with Explorer open) both show their frames. Also seen: a PDF falls back to its icon when pdftoppm is missing, and an old screen recording (rec2.mp4, one frame, 0 s) cannot be read by any tool.
