# bridge

The types the app and the notch pass each other, and nothing else. Each frame the app fills a `PanelData` (in `mod.rs`) and the notch hands back the `Action`s (in `action.rs`) for whatever was pressed.

The other files are the parts of `PanelData`, one area each: pages and tabs, tools, live activities, the assistant, the app's own rows and the welcome walkthrough.

The notch never owns a service and never changes one directly. If you want the notch to show something new, add it here and fill it in `src/app/panel.rs`. If you want it to do something, add an `Action` and carry it out in `src/app/actions.rs`. [wiki/Architecture.md](../../wiki/Architecture.md#data-flow) has the full picture.
