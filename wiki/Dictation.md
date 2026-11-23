# Dictation

Hold a key and talk, and what you said is typed where your cursor is. The bundled Dictation tool (`tools/dictation`, id `dictation`) sends your speech to Groq's Whisper with your own Groq key, tidies what comes back, and learns the names and terms you use.

## Using it

| | macOS | Windows |
| --- | --- | --- |
| Hold to talk, let go to type | **fn** (🌐) | **Ctrl + Win** |
| Tap to keep listening, tap again to type | the same key, pressed for less than a third of a second | the same chord |
| Throw it away | **Esc** while listening | **Esc** while listening |
| Other keys | right ⌥ or right ⌘ | right Ctrl or right Alt |

The key is set under **Settings › Tools › Dictation**. The **Start or finish dictation** shortcut, the tile's page and the live activity's ✓ do the same as a tap. Laptops handle fn themselves on Windows and never pass it on, which is why the PC uses a chord; Ctrl + Win does nothing in Windows on its own, and the Start menu is kept from opening when you let go.

A held key that turns out to be part of a shortcut (fn with an arrow, Ctrl + Win + →) is not dictation: the recording is thrown away. Hands-free listening that hears no speech for 10 seconds stops by itself, and a recording stops after 20 minutes.

While listening, the closed notch shows a red microphone and a meter (with a lock when hands-free), and pointing at it shows ✗ and ✓. "Writing" follows until the words are typed.

On a Mac, set **System Settings › Keyboard › Press 🌐 key to** to **Do Nothing**, or every press of fn also opens the emoji picker or switches the input source. The tool says so, with a button to that page, until it's set.

## Setting up

Dictation needs a free Groq key: open [console.groq.com/keys](https://console.groq.com/keys), sign in, press **Create API Key** and paste it into the tool's page or its settings. It stays in your `config.json` and is sent only to Groq. Pressing the key without one shows a notice that opens the page.

On a Mac it needs Accessibility (to hear fn and to paste) and the microphone, and asks for the microphone the first time.

## Settings

| Setting | Key | Default |
| --- | --- | --- |
| Groq API key | `api_key` | empty |
| Dictation key | `hotkey` | `fn` on a Mac, `ctrl_win` on Windows |
| Whisper | `model` | `accurate` (whisper-large-v3); `fast` is whisper-large-v3-turbo |
| Language | `language` | `en`; `auto` lets Whisper detect it, which is slower and can mistake short clips for another language |
| Tidy with AI | `polish` | on |
| Learn my words | `learn` | on |
| Use words on screen | `screen` | on |
| Sounds | `sounds` | on (macOS) |

## How it's fast

- The microphone starts on the key press, and the connection to Groq is opened while you talk, so no TLS handshake waits for the words.
- Silence before and after the speech is cut, quiet speech is lifted, and it's sent as 16 kHz FLAC, half the size of WAV.
- A long dictation is cut at a pause every 24 seconds or so, and each part is written down while you keep talking. Only the last part is left when you let go.
- Whisper usually answers in under a second. The tidy-up is skipped if it would take more than 2.5 seconds.

## How it's reliable

- Failed requests are tried again with back-off. When one Whisper model is over Groq's limit, the other takes over.
- A failure says what went wrong (no key, a key Groq refuses, the free limit, no network) and keeps the recording, with **Retry** when trying again could work.
- Whisper's well-known inventions in silence ("Thank you.", "Thanks for watching!") and segments it doubts itself are dropped, and nothing is sent at all when no speech was heard.
- The tidy-up model's answer is checked before it's used: if it answered the text instead of cleaning it, rewrote it, or swapped in a word that sounds nothing like what was said, Whisper's words are typed instead.
- Typing waits for the dictation key to be let go, so letters never become shortcuts.
- On a Mac the words are pasted, and the clipboard is put back as it was half a second later (marked so clipboard managers skip it). With no text field focused they're left on the clipboard and the notch says so. Windows types them as Unicode keystrokes and leaves the clipboard alone. The last few dictations are on the tool's page, with **Copy**.

## How it learns your words

The glossary lives in `dictation/glossary.json` in notchbase's data folder. Terms come from:

- **You.** Add a word on the tool's page, or a fix written `heard => meant` (`brock => Groq`), which is applied every time.
- **Your corrections** (macOS). After typing, the tool reads the field again over the next 40 seconds. Words you replace (`Brock` with `Groq`, `notch base` with `notchbase`) teach the new spelling; a correction made twice is applied by itself from then on. Everyday words and rewrites teach nothing.
- **What you say.** A name, product or piece of code you say four times is learned.
- **What's on screen.** The window title and the text around the cursor are scanned for names and terms when you start, and those lead the prompt for that dictation. One that you then say counts double.

Every transcript is then put through three steps: Whisper is prompted with the glossary, the fixer respells words heard apart or a letter off (`Github` to `GitHub`, `Teraform` to `Terraform`) without touching everyday words, and the tidy-up model (Groq's `openai/gpt-oss-120b`) fixes misheard terms, punctuation, ums and self-corrections with the glossary and the text before the cursor in mind.

Words are also fitted to the cursor: a space after a word, no capital mid-sentence, no full stop before text that carries on.

## The code

| Piece | Where |
| --- | --- |
| The tool: settings, tile, page, live activity, notices | `tools/dictation/src/lib.rs` |
| Hold, tap, shortcut and Escape | `trigger.rs` |
| The engine thread: sessions, parts, delivery, learning | `engine.rs` |
| Speech detection, trimming, cutting at pauses, resampling | `sound.rs`, `flac.rs` |
| Groq: Whisper, retries, the tidy-up model | `groq.rs` |
| Dropping inventions, checking the tidy-up, fitting to the cursor | `clean.rs` |
| The glossary, the fixer, learning | `glossary.rs`, `diff.rs`, `words.rs` |
| Keys, microphone, text around the cursor, typing, sounds | `platform/macos.rs`, `platform/windows.rs` |

`cargo test -p dictation` runs everything offline. Two tests talk to Groq and are ignored: give them a key in `GROQ_API_KEY` (and a 16 kHz mono WAV in `NOTCHBASE_DICTATION_WAV` for `real_speech_is_written`) and run them with `--ignored`.
