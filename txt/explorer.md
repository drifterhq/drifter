# Explorer punch list

Work left on Explorer (`drifter-explorer`). Done items leave this file.

## Check

- [ ] Run it under Drifter itself: the window frame, scale 1.5 and 2, theme changes.
- [ ] Offer Explorer for Files in Welcome and Default Apps.

## Stubbed or half done

- [ ] Clipboard: copy to other apps goes through `wl-copy`. Paste from other apps does not work. Files you copy in another file manager cannot be pasted here.
- [ ] Drag and drop: it works only inside Explorer. You cannot drag to or from other apps.
- [ ] Name clashes: when a pasted file has the same name as one already there, Explorer adds "2" to the name without asking. There is no Replace, Skip or Keep Both choice.
- [ ] Big copies show only "Copying…", with no progress bar and no cancel.
- [ ] Folders are read on the main thread, so a huge folder or a slow network drive freezes the window.
- [ ] Drives: the list is read once at start. It has no mount, unmount or eject, no space bars, and it does not show disks that are not mounted.
- [ ] Trash: on a USB stick, the file moves to the home Trash. The spec wants a `.Trash-UID` folder on that disk.
- [ ] Trash: the Trash view has no "original location" column and no "date deleted" column.
- [ ] Previews are not saved to disk, so each launch makes them again.
- [ ] Big photos are fully decoded on every launch.
- [ ] Folder watching: Explorer checks only the open folder, every half second. The tree and the other columns do not refresh when a folder changes.
- [ ] Open With: there is no "Other app…" picker and no "Always use this app".
- [ ] Get Info: it shows no total folder size and has no way to change anything.

## Missing basics

- [ ] A search box. There is only type-to-find.
- [ ] Lasso selection with the mouse.
- [ ] Tabs.
- [ ] Changing list column widths.
- [ ] Remembering the window size.
- [ ] A different view for each folder.
- [ ] Click-and-drag text selection in the rename and path fields.
- [ ] Compress and extract.

## Respect for the user

- [ ] Errors: an error shows in the status bar for 6 seconds and then disappears. It is easy to miss.
- [ ] Path bar: you cannot click a part of the path to go there. A click starts edit mode.
- [ ] Path bar: it has no folder dropdown.
- [ ] Keyboard: there is no visible focus ring.
- [ ] Keyboard: you cannot use Tab to move between the sidebar and the files.
- [ ] Keyboard: the tree has no keyboard control.
- [ ] Screen readers: there is no Orca support. Activity and Settings have it.
- [ ] Tooltips: none of the icon buttons have a tooltip.
- [ ] Scrollbars: they only show your position. You cannot drag them.

## Visual gusto

- [ ] Animation: menus, sheets and view changes snap in.
- [ ] Animation: hover and selection do not fade.
- [ ] Animation: scrolling does not glide.
- [ ] Flat: nothing has depth or a 3D feel, and Drifter's identity is 3D and skeuomorphic.
- [ ] Gallery: the strip has no names.
- [ ] Gallery: small pictures look tiny in the big preview.
- [ ] Empty folders show gray text and nothing else.
- [ ] Drawing cost: every hover redraws the whole window on the CPU. On the Pi it is slow. At 4K and scale 2 it will cost too much.
