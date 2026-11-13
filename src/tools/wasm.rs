//! Portable tools: WebAssembly run in wasmi, with only the clock imported. Each call gets a fuel budget, and once a
//! call traps the tool answers nothing more.

use std::path::Path;

use notchbase_sdk::ffi::ABI_VERSION;
use notchbase_sdk::ffi::portable::unpack;
use notchbase_sdk::protocol::{Answer, Call, Effects, Event, Manifest, PROTOCOL_VERSION, Start, Views};
use notchbase_sdk::{Driver, now_ms};
use wasmi::{Config, Engine, Linker, Memory, Module, Store, TypedFunc};

pub const EXTENSION: &str = "wasm";
const HOST: &str = "notchbase";
/// Each call's budget, so a tool stuck in a loop is cut off instead of hanging notchbase.
const FUEL: u64 = 500_000_000;
const TOO_LONG: &str = "it took too long to answer and was stopped";
const STOPPED: &str = "it stopped after an earlier failure";
const SAID_AT_MOST: u32 = 2048;

pub struct WasmDriver {
    store: Store<()>,
    memory: Memory,
    alloc: TypedFunc<u32, u32>,
    free: TypedFunc<(u32, u32), ()>,
    call: TypedFunc<(u32, u32), u64>,
    panicked: Option<TypedFunc<(), u64>>,
    manifest: Manifest,
    fuel: u64,
    broken: bool,
}

impl WasmDriver {
    pub fn load(path: &Path) -> Result<Self, String> {
        let bytes = std::fs::read(path).map_err(|err| err.to_string())?;
        Self::from_bytes(&bytes, path.file_stem().and_then(|stem| stem.to_str()).unwrap_or_default())
    }

    pub fn from_bytes(bytes: &[u8], stem: &str) -> Result<Self, String> {
        Self::with_fuel(bytes, stem, FUEL)
    }

    fn with_fuel(bytes: &[u8], stem: &str, fuel: u64) -> Result<Self, String> {
        let mut config = Config::default();
        config.consume_fuel(true);
        let engine = Engine::new(&config);
        let module = Module::new(&engine, bytes).map_err(|err| format!("it isn't a WebAssembly tool: {err}"))?;
        let mut linker = Linker::<()>::new(&engine);
        linker.func_wrap(HOST, "now_ms", now_ms).map_err(|err| err.to_string())?;
        let mut store = Store::new(&engine, ());
        store.set_fuel(fuel).map_err(|err| err.to_string())?;
        let instance =
            linker.instantiate_and_start(&mut store, &module).map_err(|err| format!("it couldn't start: {err}"))?;
        let missing = |name: &str| format!("it doesn't export {name}; end the tool with notchbase_sdk::export!");
        let memory = instance.get_memory(&store, "memory").ok_or_else(|| missing("memory"))?;
        let abi = instance.get_typed_func::<(), u32>(&store, "nb_abi").map_err(|_| missing("nb_abi"))?;
        let alloc = instance.get_typed_func(&store, "nb_alloc").map_err(|_| missing("nb_alloc"))?;
        let free = instance.get_typed_func(&store, "nb_free").map_err(|_| missing("nb_free"))?;
        let call = instance.get_typed_func(&store, "nb_call").map_err(|_| missing("nb_call"))?;
        let panicked = instance.get_typed_func(&store, "nb_panic").ok();
        let abi = abi.call(&mut store, ()).map_err(|err| err.to_string())?;
        if abi != ABI_VERSION {
            return Err(format!("it was built for tool ABI {abi} but notchbase speaks {ABI_VERSION}"));
        }
        let manifest = Manifest::new(stem, stem);
        let mut driver = Self { store, memory, alloc, free, call, panicked, manifest, fuel, broken: false };
        match driver.ask(&Call::Manifest) {
            Answer::Manifest { manifest } if manifest.protocol > PROTOCOL_VERSION => {
                return Err(format!("it needs a newer notchbase (tool protocol {})", manifest.protocol));
            }
            Answer::Manifest { manifest } => driver.manifest = manifest,
            Answer::Failed { error } => return Err(error),
            _ => return Err("it answered the manifest call with something else".into()),
        }
        Ok(driver)
    }

    fn ask(&mut self, call: &Call) -> Answer {
        if self.broken {
            return Answer::Failed { error: STOPPED.to_owned() };
        }
        let answer =
            serde_json::to_vec(call).map_err(|err| err.to_string()).and_then(|request| self.exchange(&request));
        answer.unwrap_or_else(|error| {
            self.broken = true;
            Answer::Failed { error: self.said().map_or(error, |said| format!("it crashed: {said}")) }
        })
    }

    /// What the tool's panic hook wrote down, read with fresh fuel after a call failed.
    fn said(&mut self) -> Option<String> {
        let panicked = self.panicked?;
        self.store.set_fuel(self.fuel).ok()?;
        let (at, len) = unpack(panicked.call(&mut self.store, ()).ok()?);
        let mut said = vec![0; len.min(SAID_AT_MOST) as usize];
        self.memory.read(&self.store, at as usize, &mut said).ok()?;
        let said = String::from_utf8_lossy(&said).trim().to_owned();
        (!said.is_empty()).then_some(said)
    }

    fn exchange(&mut self, request: &[u8]) -> Result<Answer, String> {
        let trapped = |err: wasmi::Error| match err.as_trap_code() {
            Some(wasmi::TrapCode::OutOfFuel) => TOO_LONG.to_owned(),
            _ => format!("it crashed: {err}"),
        };
        let out_of_bounds = |_| "it handed back memory it doesn't have".to_owned();
        self.store.set_fuel(self.fuel).map_err(|err| err.to_string())?;
        let len = u32::try_from(request.len()).map_err(|_| "the call is too large".to_owned())?;
        let at = self.alloc.call(&mut self.store, len).map_err(trapped)?;
        self.memory.write(&mut self.store, at as usize, request).map_err(out_of_bounds)?;
        let packed = self.call.call(&mut self.store, (at, len)).map_err(trapped)?;
        self.free.call(&mut self.store, (at, len)).map_err(trapped)?;
        let (answer_at, answer_len) = unpack(packed);
        let mut answer = vec![0; answer_len as usize];
        self.memory.read(&self.store, answer_at as usize, &mut answer).map_err(out_of_bounds)?;
        self.free.call(&mut self.store, (answer_at, answer_len)).map_err(trapped)?;
        serde_json::from_slice(&answer).map_err(|err| format!("unreadable answer: {err}"))
    }

    fn expect_effects(&mut self, call: &Call) -> Effects {
        match self.ask(call) {
            Answer::Effects { effects } => effects,
            Answer::Failed { error } => panic!("{error}"),
            _ => panic!("the tool answered with something other than effects"),
        }
    }
}

impl Driver for WasmDriver {
    fn manifest(&self) -> Manifest {
        self.manifest.clone()
    }

    fn start(&mut self, start: Start) -> Effects {
        self.expect_effects(&Call::Start { start: Box::new(start) })
    }

    fn handle(&mut self, events: Vec<Event>) -> Effects {
        self.expect_effects(&Call::Handle { events })
    }

    fn views(&mut self) -> Views {
        match self.ask(&Call::Views) {
            Answer::Views { views } => *views,
            Answer::Failed { error } => panic!("{error}"),
            _ => panic!("the tool answered with something other than views"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ANSWER_AT: u32 = 1024;

    fn tool(abi: u32, answer: &str, later: &str) -> String {
        let escaped = answer.replace('"', "\\\"");
        format!(
            r#"(module
                (import "notchbase" "now_ms" (func $now (result i64)))
                (memory (export "memory") 1)
                (data (i32.const {ANSWER_AT}) "{escaped}")
                (global $next (mut i32) (i32.const 8192))
                (global $calls (mut i32) (i32.const 0))
                (func (export "nb_abi") (result i32) (i32.const {abi}))
                (func (export "nb_alloc") (param $len i32) (result i32)
                    (global.set $next (i32.add (global.get $next) (local.get $len)))
                    (i32.sub (global.get $next) (local.get $len)))
                (func (export "nb_free") (param i32 i32))
                (func (export "nb_call") (param i32 i32) (result i64)
                    (global.set $calls (i32.add (global.get $calls) (i32.const 1)))
                    (if (i32.gt_u (global.get $calls) (i32.const 1)) (then {later}))
                    (i64.or (i64.shl (i64.const {ANSWER_AT}) (i64.const 32)) (i64.const {len}))))"#,
            len = answer.len(),
        )
    }

    fn manifest(protocol: u32) -> String {
        format!(r#"{{"answer":"manifest","manifest":{{"id":"wat","name":"Wat","protocol":{protocol}}}}}"#)
    }

    fn loaded(later: &str) -> WasmDriver {
        WasmDriver::with_fuel(tool(ABI_VERSION, &manifest(PROTOCOL_VERSION), later).as_bytes(), "wat", 100_000)
            .unwrap_or_else(|err| panic!("{err}"))
    }

    fn refused(source: &str) -> String {
        WasmDriver::from_bytes(source.as_bytes(), "wat").err().expect("it shouldn't load")
    }

    #[test]
    fn portable_tool_answers_manifest() {
        let driver = loaded("");
        assert_eq!((driver.manifest().id, driver.manifest().name), ("wat".to_owned(), "Wat".to_owned()));
    }

    #[test]
    fn it_keeps_answering_call_after_call() {
        let mut driver = loaded("");
        for _ in 0..3 {
            assert!(matches!(driver.ask(&Call::Manifest), Answer::Manifest { .. }));
        }
    }

    #[test]
    fn files_that_are_not_tools_say_why() {
        assert!(refused("not a module").starts_with("it isn't a WebAssembly tool"));
        assert!(refused("(module (memory (export \"memory\") 1))").contains("notchbase_sdk::export!"));
        let old = refused(&tool(ABI_VERSION + 1, &manifest(PROTOCOL_VERSION), ""));
        assert_eq!(old, format!("it was built for tool ABI {} but notchbase speaks {ABI_VERSION}", ABI_VERSION + 1));
        let newer = refused(&tool(ABI_VERSION, &manifest(PROTOCOL_VERSION + 1), ""));
        assert_eq!(newer, format!("it needs a newer notchbase (tool protocol {})", PROTOCOL_VERSION + 1));
        assert!(refused(&tool(ABI_VERSION, "nonsense", "")).starts_with("unreadable answer"));
    }

    #[test]
    fn crash_stops_the_tool_only() {
        let mut driver = loaded("unreachable");
        let Answer::Failed { error } = driver.ask(&Call::Views) else { panic!("it should have failed") };
        assert!(error.starts_with("it crashed"), "{error}");
        let Answer::Failed { error } = driver.ask(&Call::Views) else { panic!("it should stay stopped") };
        assert_eq!(error, STOPPED);
    }

    fn telling(later: &str, at: u32, said: &str) -> WasmDriver {
        let quiet = tool(ABI_VERSION, &manifest(PROTOCOL_VERSION), later);
        let source = format!(
            r#"{} (data (i32.const 4096) "{said}")
                (func (export "nb_panic") (result i64)
                    (i64.or (i64.shl (i64.const {at}) (i64.const 32)) (i64.const {len}))))"#,
            quiet.strip_suffix(')').expect("the module's closing bracket"),
            len = said.len(),
        );
        WasmDriver::with_fuel(source.as_bytes(), "wat", 100_000).unwrap_or_else(|err| panic!("{err}"))
    }

    fn failed(driver: &mut WasmDriver) -> String {
        let Answer::Failed { error } = driver.ask(&Call::Views) else { panic!("it should have failed") };
        error
    }

    #[test]
    fn crash_reported_in_tools_own_words() {
        let mut driver = telling("unreachable", 4096, "index out of bounds (src/lib.rs:44)");
        assert_eq!(failed(&mut driver), "it crashed: index out of bounds (src/lib.rs:44)");
        assert_eq!(failed(&mut driver), STOPPED);
        let mut silent = telling("unreachable", 4096, "");
        assert!(failed(&mut silent).starts_with("it crashed: wasm"), "nothing was said, so the trap is all there is");
        let mut lying = telling("unreachable", 4_000_000, "out of reach");
        assert!(failed(&mut lying).starts_with("it crashed: wasm"), "words it doesn't own are not read");
        let mut slow = telling("(loop (br 0))", 4096, "");
        assert_eq!(failed(&mut slow), TOO_LONG);
        let mut wordy = telling("unreachable", 4096, &"x".repeat(3000));
        assert_eq!(failed(&mut wordy).len(), "it crashed: ".len() + SAID_AT_MOST as usize);
    }

    #[test]
    fn tool_that_never_answers_is_cut_off() {
        let mut driver = loaded("(loop (br 0))");
        let Answer::Failed { error } = driver.ask(&Call::Views) else { panic!("it should have been stopped") };
        assert_eq!(error, TOO_LONG);
    }

    #[test]
    #[ignore = "set NOTCHBASE_WASM_TOOL to a tool built for wasm32-unknown-unknown to start it and time its answers"]
    fn real_portable_tool_starts_and_draws() {
        let path = std::env::var_os("NOTCHBASE_WASM_TOOL").expect("the path of a .wasm tool");
        let mut driver = WasmDriver::load(Path::new(&path)).unwrap_or_else(|err| panic!("{err}"));
        let settings = driver.manifest().defaults();
        let begun = std::time::Instant::now();
        let effects = driver.start(Start { settings, time_zone: Some("Europe/Amsterdam".into()), ..Start::default() });
        let started = begun.elapsed();
        let views = driver.views();
        let drawn = begun.elapsed() - started;
        let spent = FUEL - driver.store.get_fuel().unwrap_or(FUEL);
        println!("{}: started in {started:?}, drew in {drawn:?} for {spent} fuel", driver.manifest().id);
        assert!(effects.wants.settings || effects.requests.is_empty());
        assert!(views.tile.is_some() || views.page.is_some() || views.live.is_some(), "it shows something");
    }

    #[test]
    fn memory_it_does_not_own_is_refused() {
        let far = tool(ABI_VERSION, &manifest(PROTOCOL_VERSION), "").replace("(i64.const 1024)", "(i64.const 4000000)");
        assert_eq!(refused(&far), "it handed back memory it doesn't have");
    }
}
