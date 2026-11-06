//! Actual Squirrel 2.2.5 module execution. Host symbols must be explicitly supplied.
#[cfg(panic = "abort")]
compile_error!("nekopack-script requires panic=unwind to contain Rust callback panics");
use crate::runtime::Variables;
use std::{
    ffi::{c_char, c_void, CStr, CString},
    marker::PhantomData,
    rc::Rc,
};
type VariableCallback = unsafe extern "C" fn(
    *mut c_void,
    i32,
    *const u8,
    usize,
    *const u8,
    usize,
    *mut i32,
) -> *const c_char;
/// Synchronous, bounded resource serialization supplied by the runtime adapter.
pub trait ResourceProvider {
    fn resource(&mut self, kind: &str, name: &str) -> Result<String, String>;
}
type ResourceCallback = unsafe extern "C" fn(
    *mut c_void,
    *const u8,
    usize,
    *const u8,
    usize,
    *mut *const u8,
    *mut usize,
) -> *const c_char;
struct ResourceBinding<'a> {
    provider: &'a mut dyn ResourceProvider,
    output: String,
    error: [u8; 256],
    panicked: bool,
}
unsafe extern "C" fn resource_callback(
    state: *mut c_void,
    kind: *const u8,
    kind_len: usize,
    name: *const u8,
    name_len: usize,
    data: *mut *const u8,
    length: *mut usize,
) -> *const c_char {
    let binding = unsafe { &mut *state.cast::<ResourceBinding<'_>>() };
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let kind = std::str::from_utf8(unsafe { std::slice::from_raw_parts(kind, kind_len) })
            .map_err(|e| e.to_string())?;
        let name = std::str::from_utf8(unsafe { std::slice::from_raw_parts(name, name_len) })
            .map_err(|e| e.to_string())?;
        let output = binding.provider.resource(kind, name)?;
        if output.len() > 1024 * 1024 {
            return Err("resource literal exceeds 1 MiB".into());
        }
        binding.output = output;
        unsafe {
            *data = binding.output.as_ptr();
            *length = binding.output.len();
        }
        Ok::<(), String>(())
    }));
    match result {
        Ok(Ok(())) => std::ptr::null(),
        Ok(Err(error)) => {
            binding.error.fill(0);
            for (out, byte) in binding.error.iter_mut().take(255).zip(error.bytes()) {
                *out = if byte == 0 { b'?' } else { byte };
            }
            binding.error.as_ptr().cast()
        }
        Err(payload) => {
            std::mem::forget(payload);
            binding.panicked = true;
            c"Rust resource callback panicked".as_ptr()
        }
    }
}
unsafe extern "C" {
    fn neko_sq_bind_resources(
        vm: *mut c_void,
        state: *mut c_void,
        callback: Option<ResourceCallback>,
    );
    fn neko_sq_bind(vm: *mut c_void, state: *mut c_void, callback: Option<VariableCallback>);
    fn neko_sq_open() -> *mut c_void;
    fn neko_sq_close(vm: *mut c_void);
    fn neko_sq_result(vm: *mut c_void) -> *const c_char;
    fn neko_sq_error(vm: *mut c_void) -> *const c_char;
    fn neko_sq_memory(vm: *mut c_void) -> usize;
    fn neko_sq_eval(
        vm: *mut c_void,
        source: *const c_char,
        length: usize,
        name: *const c_char,
        budget: usize,
        memory: usize,
        work: usize,
    ) -> i32;
}
struct Binding<'a> {
    variables: &'a mut Variables,
    panicked: bool,
    error: [u8; 256],
}
#[cfg(any(test, feature = "validation"))]
thread_local! { static INJECT_CALLBACK_PANIC: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }
unsafe extern "C" fn variable_callback(
    state: *mut c_void,
    operation: i32,
    name: *const u8,
    length: usize,
    alias: *const u8,
    alias_length: usize,
    value: *mut i32,
) -> *const c_char {
    // Pointers are borrowed only during synchronous eval; C++ validates argument types.
    let binding = unsafe { &mut *state.cast::<Binding<'_>>() };
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        #[cfg(any(test, feature = "validation"))]
        if INJECT_CALLBACK_PANIC.with(|flag| flag.replace(false)) {
            panic!("injected callback fault");
        }
        let name = unsafe { std::slice::from_raw_parts(name, length) };
        match operation {
            0 => binding.variables.get(name).map(|v| unsafe { *value = v }),
            1 => binding.variables.set(name, unsafe { *value }),
            2 => binding.variables.define_system(name, unsafe {
                std::slice::from_raw_parts(alias, alias_length)
            }),
            _ => Err("invalid variable operation".into()),
        }
    }));
    match result {
        Ok(Ok(())) => std::ptr::null(),
        Ok(Err(error)) => {
            binding.error.fill(0);
            for (out, byte) in binding.error.iter_mut().take(255).zip(error.bytes()) {
                *out = if byte == 0 { b'?' } else { byte };
            }
            binding.error.as_ptr().cast()
        }
        Err(payload) => {
            // Do not unwind through C++; a panic payload may itself panic on Drop.
            std::mem::forget(payload);
            binding.panicked = true;
            c"Rust variable callback panicked".as_ptr()
        }
    }
}
pub struct SystemVm {
    raw: *mut c_void,
    instruction_budget: usize,
    memory_budget: usize,
    native_work_budget: usize,
    callback_failed: bool,
    _thread: PhantomData<Rc<()>>,
}
impl Default for SystemVm {
    fn default() -> Self {
        Self::new()
    }
}
impl SystemVm {
    pub fn eval_with_resources(
        &mut self,
        name: &str,
        source: &str,
        variables: &mut Variables,
        provider: &mut dyn ResourceProvider,
    ) -> Result<(), String> {
        let mut binding = ResourceBinding {
            provider,
            output: String::new(),
            error: [0; 256],
            panicked: false,
        };
        unsafe {
            neko_sq_bind_resources(
                self.raw,
                (&mut binding as *mut ResourceBinding<'_>).cast(),
                Some(resource_callback),
            );
        }
        let result = self.eval_with_variables(name, source, variables);
        unsafe {
            neko_sq_bind_resources(self.raw, std::ptr::null_mut(), None);
        }
        if binding.panicked {
            self.callback_failed = true;
            Err("Rust resource callback panicked; VM stopped".into())
        } else {
            result
        }
    }
    pub fn new() -> Self {
        Self::try_new().expect("Squirrel VM initialization failed")
    }
    pub fn try_new() -> Result<Self, String> {
        let raw = unsafe { neko_sq_open() };
        if raw.is_null() {
            return Err("Squirrel VM allocation failed".into());
        }
        let mut vm = Self {
            raw,
            instruction_budget: 1_000_000,
            memory_budget: 64 * 1024 * 1024,
            native_work_budget: 4_000_000,
            callback_failed: false,
            _thread: PhantomData,
        };
        vm.eval(
            "variable-access",
            r#"
            class NekoVariableAccess {
                function _get(name) { return ::NekoVariableGet(name) }
                function _set(name,value) { ::NekoVariableSet(name,value) }
            }
            var <- NekoVariableAccess()
        "#,
        )?;
        Ok(vm)
    }
    pub fn result(&self) -> String {
        unsafe { CStr::from_ptr(neko_sq_result(self.raw)) }
            .to_string_lossy()
            .into_owned()
    }
    /// Bytecodes per eval, shared by child VMs. Exhaustion stops this VM permanently.
    /// Compilation/native work and allocation use separate native limits.
    pub fn set_instruction_budget(&mut self, budget: usize) {
        self.instruction_budget = budget;
    }
    pub fn set_native_limits(&mut self, memory_bytes: usize, work_units: usize) {
        self.memory_budget = memory_bytes;
        self.native_work_budget = work_units;
    }
    pub fn memory_used(&self) -> usize {
        unsafe { neko_sq_memory(self.raw) }
    }
    /// Bind the authoritative Rust state for this call only, including error paths.
    pub fn eval_with_variables(
        &mut self,
        name: &str,
        source: &str,
        variables: &mut Variables,
    ) -> Result<(), String> {
        let mut binding = Binding {
            variables,
            panicked: false,
            error: [0; 256],
        };
        unsafe {
            neko_sq_bind(
                self.raw,
                (&mut binding as *mut Binding<'_>).cast(),
                Some(variable_callback),
            );
        }
        let result = self.eval(name, source);
        unsafe {
            neko_sq_bind(self.raw, std::ptr::null_mut(), None);
        }
        if binding.panicked {
            self.callback_failed = true;
            Err("Rust variable callback panicked; state may be partially changed".into())
        } else {
            result
        }
    }
    pub fn eval(&mut self, name: &str, source: &str) -> Result<(), String> {
        if self.callback_failed {
            return Err("VM stopped after Rust callback panic".into());
        }
        if source.len() > 1024 * 1024 {
            return Err("module source exceeds 1 MiB limit".into());
        }
        let name = CString::new(name).map_err(|_| "module name contains NUL")?;
        if source.contains('\0') {
            return Err("module source contains NUL".into());
        }
        let result = unsafe {
            neko_sq_eval(
                self.raw,
                source.as_ptr().cast(),
                source.len(),
                name.as_ptr(),
                self.instruction_budget,
                self.memory_budget,
                self.native_work_budget,
            )
        };
        if result == 0 {
            Ok(())
        } else {
            Err(unsafe { CStr::from_ptr(neko_sq_error(self.raw)) }
                .to_string_lossy()
                .into_owned())
        }
    }
}
impl Drop for SystemVm {
    fn drop(&mut self) {
        unsafe { neko_sq_close(self.raw) }
    }
}

#[cfg(feature = "validation")]
impl SystemVm {
    pub fn validation_live_bytes() -> usize {
        unsafe extern "C" {
            fn neko_sq_live_bytes() -> usize;
        }
        unsafe { neko_sq_live_bytes() }
    }
    pub fn validation_fail_after(&mut self, count: usize) {
        unsafe extern "C" {
            fn neko_sq_fail_after(vm: *mut c_void, count: usize);
        }
        unsafe {
            neko_sq_fail_after(self.raw, count);
        }
    }
    pub fn validation_panic_next_callback(&mut self) {
        INJECT_CALLBACK_PANIC.with(|flag| flag.set(true));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    unsafe extern "C" {
        fn neko_sq_live_bytes() -> usize;
        fn neko_sq_fail_after(vm: *mut c_void, count: usize);
    }
    #[test]
    fn allocation_faults_are_terminal_and_all_arena_blocks_are_reclaimed() {
        let baseline = unsafe { neko_sq_live_bytes() };
        for source in [
            "local a=[]; for(local i=0;i<80;i++)a.append({value=i}); return a.len().tostring()",
            "local re=regexp(\"a+\"); return re.search(\"aaaa\")!=null ? \"ok\" : \"bad\"",
            "class Node {value=null; constructor(v){value=v}} local a=Node(array(32)); return format(\"%s\",\"ok\")",
        ] {
        let mut failures = 0;
        let mut reached_success = false;
        for count in 0..2000 {
            {
                let mut vm = SystemVm::try_new().unwrap();
                unsafe { neko_sq_fail_after(vm.raw, count) };
                let result=vm.eval("allocation-fault", source);
                if let Err(error) = result {
                    assert!(error.contains("native allocation failed"), "{error}");
                    assert!(vm
                        .eval("again", "return 1")
                        .unwrap_err()
                        .contains("VM stopped"));
                    failures += 1;
                } else {
                    reached_success = true;
                }
            }
            assert_eq!(
                unsafe { neko_sq_live_bytes() },
                baseline,
                "allocation point {count}"
            );
            if reached_success {
                break;
            }
        }
        assert!(failures > 0);
        eprintln!("covered {failures} allocation failures before successful execution");
        assert!(
            reached_success,
            "fault sweep must cover every allocation through successful execution"
        );
        }
    }
    #[test]
    fn native_limits_are_per_vm_and_failure_cleanup_does_not_touch_another_vm() {
        let baseline = unsafe { neko_sq_live_bytes() };
        {
            let mut healthy = SystemVm::new();
            healthy.eval("state", "value <- 42").unwrap();
            let memory = healthy.memory_used();
            let mut limited = SystemVm::new();
            limited.set_native_limits(0, 100);
            assert!(limited
                .eval("limit", "return 1")
                .unwrap_err()
                .contains("memory budget"));
            drop(limited);
            assert_eq!(healthy.memory_used(), memory);
            healthy.eval("intact", "return value.tostring()").unwrap();
            assert_eq!(healthy.result(), "42");
        }
        assert_eq!(unsafe { neko_sq_live_bytes() }, baseline);
    }
    #[test]
    fn callback_panic_is_contained_even_if_script_catches_it() {
        let mut vars = Variables::default();
        vars.define_system(b"G1", b"score").unwrap();
        let mut vm = SystemVm::new();
        INJECT_CALLBACK_PANIC.with(|flag| flag.set(true));
        let error = vm
            .eval_with_variables(
                "panic",
                "try {NekoVariableGet(\"score\")} catch(e) {}",
                &mut vars,
            )
            .unwrap_err();
        assert!(error.contains("callback panicked"));
        assert!(vm
            .eval("again", "return 1")
            .unwrap_err()
            .contains("callback panic"));
        assert_eq!(vars.get(b"score").unwrap(), 0);
    }
    #[test]
    fn variable_errors_preserve_diagnostics_and_unbind() {
        let mut vm = SystemVm::new();
        let mut vars = Variables::default();
        let error = vm
            .eval_with_variables("bad", "def(\"G-1\",\"bad\")", &mut vars)
            .unwrap_err();
        assert!(error.contains("invalid system variable address"));
        assert!(vm
            .eval("unbound", "NekoVariableGet(\"bad\")")
            .unwrap_err()
            .contains("not bound"));
        vm.eval_with_variables("good", "def(\"G1\",\"ok\"); var.ok=12", &mut vars)
            .unwrap();
        assert_eq!(vars.get(b"ok").unwrap(), 12);
    }
    #[test]
    fn variables_are_live_aliases_across_both_languages() {
        let mut state = Variables::default();
        state.define_system(b"G200", b"first").unwrap();
        state.set(b"first", 7).unwrap();
        state.define_constant(b"constant", 9).unwrap();
        let mut vm = SystemVm::new();
        vm.eval("host", include_str!("../host/headless.nut"))
            .unwrap();
        vm.eval_with_variables(
            "aliases",
            r#"
            def("G200","second")
            if(var.second!=7)throw "initial value"
            var.second=12
            if(var.first!=12)throw "alias did not share storage"
            def("GF200","seen")
            var["@seen"]=1
            if(var.first!=12)throw "flag collided with integer slot"
            def("GF200","seen")
            if(var["@seen"]!=1)throw "redeclaration cleared state"
        "#,
            &mut state,
        )
        .unwrap();
        assert_eq!(state.get(b"first").unwrap(), 12);
        state.set(b"first", 23).unwrap();
        vm.eval_with_variables(
            "later",
            "if(var.second!=23)throw \"stale state\"",
            &mut state,
        )
        .unwrap();
        for code in [
            "var.constant=1",
            "var.unknown=1",
            "var.first=1.5",
            "def(\"G201\",\"first\")",
            "def(\"G-1\",\"bad\")",
        ] {
            assert!(
                vm.eval_with_variables("invalid", code, &mut state).is_err(),
                "{code}"
            );
            assert!(vm.eval("unbound", "return var.first").is_err());
        }
        vm.eval_with_variables("after-errors", "var.second=31", &mut state)
            .unwrap();
        assert_eq!(state.get(b"first").unwrap(), 31);
        assert_eq!(state.get(b"constant").unwrap(), 9);
    }
    #[test]
    fn system_callback_writes_resume_in_scenario_state() {
        use crate::runtime::{Machine, Request, Scripts, Source, Step};
        struct NoScripts;
        impl Scripts for NoScripts {
            fn load(&mut self, _: &[u8]) -> Result<Source, String> {
                Err("unexpected load".into())
            }
        }
        let mut machine = Machine::new(
            Source {
                name: b"fixture".to_vec(),
                bytes: b".var G1 score\n? score = 4\n.system fixture\n? score = 19\n.observe"
                    .to_vec(),
            },
            100,
            4,
        );
        let mut scripts = NoScripts;
        assert!(matches!(
            machine.advance(&mut scripts).unwrap(),
            Step::Request(Request::System(_))
        ));
        let mut vm = SystemVm::new();
        vm.eval("host", include_str!("../host/headless.nut"))
            .unwrap();
        vm.eval_with_variables(
            "callback",
            "def(\"G1\",\"alias\"); var.alias=var.score+8",
            &mut machine.variables,
        )
        .unwrap();
        assert_eq!(machine.variables.get(b"score").unwrap(), 12);
        machine.complete(None).unwrap();
        assert!(matches!(
            machine.advance(&mut scripts).unwrap(),
            Step::Request(Request::Command { .. })
        ));
        vm.eval_with_variables(
            "observe",
            "if(var.alias!=19)throw \"scenario write missing\"",
            &mut machine.variables,
        )
        .unwrap();
    }
    #[test]
    fn executes_registration_not_comment_text() {
        let mut vm = SystemVm::new();
        vm.eval(
            "fixture",
            r#"
            class Registry {
                entries = null
                constructor() { entries = {} }
                function _set(k,v) { entries.rawset(k,v) }
                function _get(k) { return entries[k] }
            }
            com <- Registry()

            // com["fake"] = function(){};
            com["answer"] = function(v){return v+1;};
            if ("fake" in com || com.answer(41)!=42) throw "bad registration";
        "#,
        )
        .unwrap();
        assert!(vm.eval("unknown", "MissingHostApi()").is_err());
        vm.eval(
            "after_error",
            "if(com.answer(1)!=2) throw \"stack corruption\";",
        )
        .unwrap();
    }
}

#[cfg(test)]
mod resource_tests {
    use super::*;
    struct Provider {
        panic: bool,
    }
    impl ResourceProvider for Provider {
        fn resource(&mut self, _kind: &str, name: &str) -> Result<String, String> {
            if self.panic {
                panic!("resource fault")
            }
            if name == "missing" {
                Err("missing resource".into())
            } else {
                Ok("resource-value".into())
            }
        }
    }
    #[test]
    fn resource_binding_errors_children_and_panic_are_contained() {
        let mut vm = SystemVm::new();
        let mut vars = Variables::default();
        let mut provider = Provider { panic: false };
        vm.eval_with_resources(
            "child",
            "return newthread(function(){return NekoResource(\"Data\",\"ok\")}).call()",
            &mut vars,
            &mut provider,
        )
        .unwrap();
        assert_eq!(vm.result(), "resource-value");
        assert!(vm
            .eval_with_resources(
                "missing",
                "NekoResource(\"Data\",\"missing\")",
                &mut vars,
                &mut provider
            )
            .unwrap_err()
            .contains("missing resource"));
        assert!(vm
            .eval("unbound", "NekoResource(\"Data\",\"ok\")")
            .unwrap_err()
            .contains("not bound"));
        provider.panic = true;
        assert!(vm
            .eval_with_resources(
                "panic",
                "try {NekoResource(\"Data\",\"ok\")} catch(e) {}",
                &mut vars,
                &mut provider
            )
            .unwrap_err()
            .contains("resource callback panicked"));
        assert!(vm
            .eval("terminal", "return 1")
            .unwrap_err()
            .contains("VM stopped"));
    }
}
