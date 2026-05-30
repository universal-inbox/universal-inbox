# Universal Inbox — App UI Kit

High-fidelity recreation of the Universal Inbox web app: a three-pane, keyboard-driven notifications triage tool. Built from the design proposal in `uploads/`.

## Files
- `index.html` — interactive click-through demo of the Inbox page
- `Sidebar.jsx` — dark, always-on nav rail (logo, groups, profile footer)
- `ListPanel.jsx` — filter bar, search, time-grouped notification rows
- `DetailPanel.jsx` — selected notification preview + action dock
- `Footer.jsx` — integration status strip
- `data.js` — seed notifications + integration metadata

## What it shows
- Three-pane layout, sidebar + list + detail
- Click any notification row to preview it
- Filter chips (All / Unread / Mentions) affect the list
- Dark sidebar, light content — intentional asymmetric frame
- Action dock: snooze, done, delete, create task, open in browser

## What it skips
- Keyboard shortcuts (full product has 15+)
- Task-plan modal, link-to-existing-task modal
- Dark-mode theme on content panes
- Real OAuth/sync — integration statuses are static
