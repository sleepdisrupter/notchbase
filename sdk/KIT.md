# notchbase tools kit

Everything you need to build your own notchbase tools: the SDK, an example tool, the docs and an install script. Tools are small Rust plugins that notchbase loads by itself, with no restart.

```
notchbase-tools/
  notchbase-sdk/   the SDK your tools compile against
  notes/           a complete example tool with tests
  docs/            how everything works
  install.ps1      build a tool and install it (Windows)
  install.sh       build a tool and install it (macOS)
```

## The easy way: let an AI build it

1. Unzip this into your home folder, so you have `notchbase-tools` there (for example `C:\Users\you\notchbase-tools` or `/Users/you/notchbase-tools`).
2. Add the **notchbase** skill (`notchbase-skill.zip`) to Claude or ChatGPT. It builds tools, and answers questions about notchbase itself too.
3. Describe the tool you want: "a tile that shows my next meeting", "a timer that chimes every 25 minutes".
4. With Claude Code, it writes, tests and installs the tool for you. In a chat without a terminal, save the files it gives you inside `notchbase-tools` and run the commands it shows you.
5. Look for "Reloaded <your tool>" in the notch. Your tool is on the **Tools** tab.

## Doing it yourself

You need Rust 1.88 or newer: [rustup.rs](https://rustup.rs) (`rustup update` brings an older one up to date). On Windows, Rust also needs the Visual Studio C++ build tools.

Each tool is a folder next to `notchbase-sdk`, for example `notchbase-tools/my-tool/`, whose `Cargo.toml` has:

```toml
[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
notchbase-sdk = { path = "../notchbase-sdk" }

[workspace]
```

Try the example first. From the `notchbase-tools` folder:

```sh
cargo test --manifest-path notes/Cargo.toml
```

Then install it:

- Windows: `powershell -ExecutionPolicy Bypass -File install.ps1 notes -Native`
- macOS: `sh install.sh --native notes`

The script builds a portable tool, one `.wasm` file that runs on any computer with notchbase, and copies it into your tools folder (`%APPDATA%\notchbase\tools` or `~/Library/Application Support/notchbase/tools`). notchbase picks it up about a second later. Run it again after every change; to remove a tool, delete its folder there.

If a tool doesn't show up, or its page says "This tool stopped working", open **Settings › Tools** in the notch: the reason is under the tool's name, and for a portable tool it names the line of your code that failed. `docs/Plugins-and-Distribution.md` lists what each reason means.

Start with `docs/Getting-Started.md`, then `docs/Surfaces.md` and `docs/Elements.md`.

## Sharing a tool

Send the one built file, `my-tool/target/wasm32-unknown-unknown/release/my_tool.wasm`. It runs on Windows and macOS alike: the other person puts it in a folder named after the tool inside their own tools folder.

A tool that needs more than a portable one can do (see `docs/Plugins-and-Distribution.md`) is built natively instead, with `sh install.sh --native my-tool` or `install.ps1 my-tool -Native`, and then only runs on the kind of computer it was built on. The `notes` example is one of these, because it writes files itself.
