# Calendar

The bundled Calendar tool ([`tools/calendar`](../tools/calendar)) puts your next meeting in the notch. It shows up a few minutes before it starts with a ring counting down and a **Join** button, nudges once when it begins, and counts down the time left while it runs. It has its own **Calendar** tab at the end of the header: a month calendar with the selected day's events under it.

Where the events come from:

| Platform | Source | New events go to |
| --- | --- | --- |
| macOS | **Apple Calendar** (the default): every account added in System Settings, Internet Accounts | Apple Calendar, and from there iCloud, Google or whichever account the chosen calendar belongs to. Its events can be moved, renamed and deleted too |
| macOS | **iCal link**, chosen under Settings, Calendar, Source | notchbase on this computer |
| Windows | **iCal link**, or none at all | notchbase on this computer |

An iCal link is a read-only feed by design, so its events are never offered for editing. Events you add while using a link, or with no calendar at all, are kept by notchbase itself: they show on the grid and the day list in purple ("On this computer"), become live activities like any meeting, and can be edited and deleted.

## Settings

| Setting | Key | Default | Meaning |
| --- | --- | --- | --- |
| Source | `source` | `apple` on macOS, `ical` elsewhere | `apple` or `ical`. Only shown on a Mac |
| iCal link | `link` | empty | One or more secret iCal addresses, separated by spaces. A secret setting, shown as dots. Only shown while the source is `ical` |
| Heads-up | `heads_up` | 10 | Minutes before a meeting that it appears in the notch, 0 to 30 in steps of 5 |
| Nudge when it starts | `nudge` | on | A shaking notice as a meeting begins |
| All-day events | `all_day` | on | Show all-day events on the page (they never become live activities) |
| (hidden) | `setup_skipped` | `false` | Set when the "Add a calendar to see your events" row is dismissed with its × |
| (hidden) | `view` | `week` | `week` (the Days view) or `month`, the last view you chose |
| Week starts on | `week_start` | `auto` | `auto`, `monday` or `sunday`. Auto starts on Sunday with a 12-hour clock and on Monday with a 24-hour one |

`webcal://` addresses are read as `https://`. In Google Calendar the secret address is under the calendar's settings, in Integrate calendar, as "Secret address in iCal format". Outlook and iCloud publish the same kind of link.

## The live activity

`upcoming.rs` decides what the notch shows:

- **Coming up**: a timed meeting that starts within the heads-up time. A blue ring drains from the start of the heads-up to the meeting's start, next to the title and a countdown (on a hardware notch, just the countdown, for room).
- **Happening now**: once it starts, a green ring drains toward the end with the time left. A meeting coming up beats one already running, so the next call wins over one that's overrunning.
- **Left out**: all-day events, declined meetings, and anything longer than four hours (focus blocks, out-of-office).

Hovering the live activity, in the middle of the notch or in an ear, shows its controls: a join button when the meeting has a link, tinted with its service's color (Zoom blue, Meet green, Teams purple, FaceTime green and so on) and showing a video glyph, or a link glyph for any other link; and × to hide that meeting's timer, for a meeting that was canceled or ended early. Hidden meetings stay on the page.

Within the first minute of a meeting, a notice with the title and "Starting now" shakes the notch once, unless **Nudge when it starts** is off. Opening notchbase late never nudges. While a meeting is close the tool ticks every 5 seconds, otherwise every 30.


## Join links

`join.rs` looks through, in order, Google's conference field, the event's URL, its location and its description, and takes the first link to a meeting: Google Meet, Zoom (`/j/`, `/my/`, `/w/`, `/wc/`, `/s/` paths), Microsoft Teams, Webex, Whereby, Around, Jitsi, Amazon Chime or FaceTime. Other links, such as a Zoom download page or a shared document, are skipped. `&amp;` in HTML descriptions is decoded.

An event's own link counts too, even when it isn't a meeting: the **Link** you add in the form, and with Apple Calendar the event's URL field. A link typed without `https://` gets it; app links such as `zoommtg:`, `msteams:`, `facetime:` and `facetime-audio:` are kept as they are.

`join::service` names the link for its button: **Zoom**, **Meet**, **Teams**, **Webex**, **Whereby**, **Around**, **Jitsi**, **Chime** or **FaceTime**, each with its own accent color and a video glyph, or **Open** in gray with a link glyph for anything else. notchbase can't make a FaceTime link itself (macOS offers no way to), so for a FaceTime call create the link in FaceTime (Create Link) and paste it.

## The page

The Calendar tab (the tool asks for it with `.header_tab()`, so it has no tile on the Tools tab) shows, top to bottom:

- **The controls**: a **Days | Month** switch (Days by default; the choice is remembered in the hidden `view` setting), **Today** once you've moved away, in Days also **+** (a new event on the selected day), and chevrons that move four days or a month at a time. Scrolling over the days or the month does the same: a swipe on a trackpad (sideways or up and down) or a notch of a mouse wheel turns one page.
- **The title**: the days on screen ("Sep 30 – Oct 3") or the month ("September 2026").
- **Days view**: four days side by side, since a whole week wouldn't leave room to read anything. They start today and page four at a time, so today is always the first column of its page. It's a [week](tools/Elements.md#week): one column per day with the weekday and date on top, today in red, and that day's events stacked below as tinted blocks with a short time ("9:30a", "2p", or "14:00" on a 24-hour clock) over the title; a day with more than four shows three and "+N". Everything is in the columns, so there's no list under them. Tap a column to select the day (the highlight flows there like liquid, and **+** adds to it); tap a block to edit the event when it's yours, to join it when it's a meeting that hasn't ended, or else to select its day.
- **Month view**: a [month](tools/Elements.md#month) of six weeks with today in red, the selected day highlighted and up to three dots under each day, one per calendar color with an event that day. Click a day to select it, including the dimmed days of the months before and after, which turn the calendar to that month. Below it:
  - **The day**: "Today", "Tomorrow" or the full date, with **+** (a new event on the selected day), and with Apple Calendar also **Accounts** (opens System Settings, Internet Accounts, the only place macOS lets accounts be added).
  - **Its events**, or "No events".

Events are read for the whole grid, not just today, and at least a week either side of the selected day: iCal links are expanded for that span, and Apple Calendar is asked for them with [`calendar_between`](tools/Subscriptions.md#calendar), so turning to another month fills it in straight away.

Each row has the calendar's color, the title, the times (in the system's 12 or 24 hour style), "Now" while it runs, and the location unless the location is just the meeting link. Past and declined meetings are dimmed. A button named for the meeting's service (**Zoom**, **Meet**, **FaceTime**…, or **Open** for another link) appears until it ends, in that service's color, and **Edit** appears for events you can change: your own local events, and with Apple Calendar timed events on calendars that allow changes.

When there's nothing to show yet the page explains why: no link added yet (with **Open Settings**, and a × for people who don't want to add one: the row then stays away for good and the page is simply an empty calendar, while the link can still be added in Settings), still loading, waiting for macOS to ask for access, access refused (with **Open Privacy Settings**), or Apple Calendar chosen on a machine that doesn't have it. A link that fails to load shows its reason in orange; the last good copy stays on screen.

## Adding and editing

`editor.rs` is a short form on the page, top to bottom: **New event** (or **Edit event**) with a × to close it; the title; a **Link** (such as a Zoom, Meet or FaceTime invite); a time picker of narrow number wheels, like a phone's (hour, minute in steps of 5 and AM/PM, or hour and minute on a 24-hour clock); a card with **Date** (the chosen day, such as "Today" or "Fri, Oct 2"; tapping it swaps the time wheels for a month calendar with arrows to any month, and tapping a day picks it and brings the wheels back), **Duration** (15 minutes to 3 hours, plus the event's own length if it's something else), **Color** and, only when there's a choice, **Calendar**; and a bottom line saying where it will be saved, with **Save** (and **Delete** when editing).

The line at the bottom makes the destination plain. With Apple Calendar it has a blue cloud: "Saves to iCloud", "Saves to" the account (such as a Google address), or "Saves to Apple Calendar" for a calendar on the Mac itself. Otherwise it has a computer glyph and reads "Only on this computer". The calendars offered are the Apple calendars that take new events, or when there are none (an iCal link, no calendar, or access refused), just "On this computer". Editing a local event while using Apple Calendar offers both, so it can be moved into Apple Calendar; saving it there removes the local copy.

**Color** is a [palette](tools/Elements.md#palette): first the calendar's own color (the default), then red, orange, yellow, green, teal, blue, purple and pink, leaving out the one that matches the calendar's. Apple Calendar only colors whole calendars, so the choice is kept by notchbase, in `colors.json` in the tool's data folder, for your local events and your Apple Calendar events alike. A repeating event's color covers every occurrence. A new event's color waits until the event turns up with its id, and is then saved under it. Events from an iCal link can't be opened in the form, so they keep their calendar's color.

While the form is open on the Calendar page, the tool holds the notch open ([`hold_open`](tools/Commands.md#hold_open-and-release)), so the pointer can wander off the notch without losing a half-written event. Saving, deleting or closing the form with × lets go. Switching to another tab lets go too, but keeps the unfinished event, and the hold returns when you come back (the tool follows the open page through [`Subscription::notch`](tools/Subscriptions.md#notch)).

Local events (`local.rs`) are kept in `events.json` in the tool's data folder, written to a new file and swapped in so a crash can't leave half a file. A new event starts at the next half hour when the selected day is today, and at 9:00 on any other day. For Apple Calendar, **Save** sends [`Command::save_event`](tools/Commands.md#save_event-and-delete_event) and **Delete** sends `delete_event`. The link is saved with the event: in `events.json` for a local one, as the event's URL in Apple Calendar. Editing keeps an event's exact start and length until a wheel is moved, and never touches its location or notes. A change to a repeating event only changes that occurrence.

## Reading iCal feeds

`ics/` is a small reader written for this tool:

- `lines.rs` unfolds wrapped lines and splits properties, parameters (quoted or not) and values, and unescapes text.
- `component.rs` builds the `VCALENDAR`, `VEVENT` and `VTIMEZONE` tree.
- `stamp.rs` reads dates, floating times, UTC times and `TZID` times.
- `zones.rs` turns a `TZID` into a time zone: an IANA name through `jiff`'s time zone database, then the feed's own `VTIMEZONE` rules (which is how Outlook's "Pacific Standard Time" style names work), then Mozilla-style prefixed names, and finally the local zone.
- `recur.rs` expands `RRULE`s: daily, weekly, monthly and yearly, with `INTERVAL`, `COUNT`, `UNTIL`, `BYDAY` (with ordinals like `2TU` or `-1FR`), `BYMONTHDAY` (including negative days), `BYMONTH`, `BYSETPOS` and `WKST`. Impossible dates such as February 30th are skipped, as the standard says.
- `mod.rs` expands each series into today's and tomorrow's meetings, applying `RDATE`, `EXDATE`, moved occurrences (`RECURRENCE-ID`) and cancellations. Repeats keep their wall-clock time across daylight saving changes. A meeting you declined is recognized from your own `ATTENDEE` line, using the calendar's name when Google sets it to your address.

`fetch.rs` downloads each link on a background thread (20 second timeout, 32 MB at most) and keeps a copy in the tool's data folder under `feeds/`, so the agenda appears straight away on the next start and survives being offline. Links are checked again as soon as the notch opens and as soon as the Calendar tab comes up, every 15 seconds while the Calendar is on screen, and every minute otherwise, so a change made in Google Calendar shows up almost at once. Only one check runs at a time. Each check sends the `ETag` and `Last-Modified` the server gave last time, so a server that supports them answers "not modified" without resending the calendar (Google's feeds don't: they're built fresh on every request). Several links are merged, and the same meeting in two of them shows once.

## Apple Calendar in the app

The host side is `src/system/calendar/`. A `SystemCalendar` runs on its own thread while any tool subscribes to [`Subscription::calendar`](tools/Subscriptions.md#calendar). On macOS (`macos/`) it asks for full calendar access once (`NSCalendarsFullAccessUsageDescription` in `Info.plist`), then reads events from 36 hours ago to 60 hours ahead, widened to any range a tool asks for (up to 400 days), with EventKit every 30 seconds and again as soon as that range changes, sending a new feed only when something changed. Saves and deletes queue on the same thread and apply right away with `EKSpan::ThisEvent`, followed by a fresh read; if Apple Calendar refuses, the reason is shown in the notch. On Windows the feed is `Unavailable`.
