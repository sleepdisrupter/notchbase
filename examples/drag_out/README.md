# drag_out

A small window for trying file drags by hand. It builds the app's own drag code from `src/filedrag`, so what works here works in notchbase.

```sh
cargo run --example drag_out                  # drag two temp files out of the window, and see every drag event
cargo run --example drag_out -- --reveal      # show the first temp file in Finder or Explorer, then quit
cargo run --example drag_out -- --self-test   # Windows: style the window like the notch and script a drag onto it
```

The self-test also checks that drops skip a click-through window. See [Building and Testing](../../wiki/Building-and-Testing.md#the-drag-out-example).
