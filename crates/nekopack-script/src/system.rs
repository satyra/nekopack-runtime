//! Actual Squirrel 2.2.5 module execution. Host symbols must be explicitly supplied.
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
unsafe extern "C" {
    fn neko_sq_bind(vm: *mut c_void, state: *mut c_void, callback: Option<VariableCallback>);
    fn neko_sq_open() -> *mut c_void;
    fn neko_sq_close(vm: *mut c_void);
    fn neko_sq_result(vm: *mut c_void) -> *const c_char;
    fn neko_sq_error(vm: *mut c_void) -> *const c_char;
    fn neko_sq_eval(
        vm: *mut c_void,
        source: *const c_char,
        length: usize,
        name: *const c_char,
    ) -> i32;
}
struct Binding<'a> {
    variables: &'a mut Variables,
    error: CString,
}
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
    let name = unsafe { std::slice::from_raw_parts(name, length) };
    let result = match operation {
        0 => binding.variables.get(name).map(|v| unsafe { *value = v }),
        1 => binding.variables.set(name, unsafe { *value }),
        2 => binding.variables.define_system(name, unsafe {
            std::slice::from_raw_parts(alias, alias_length)
        }),
        _ => unreachable!("internal variable operation"),
    };
    match result {
        Ok(()) => std::ptr::null(),
        Err(e) => {
            binding.error = CString::new(e).expect("variable diagnostics escape raw names");
            binding.error.as_ptr()
        }
    }
}
pub struct SystemVm {
    raw: *mut c_void,
    _thread: PhantomData<Rc<()>>,
}
impl Default for SystemVm {
    fn default() -> Self {
        Self::new()
    }
}
impl SystemVm {
    pub fn new() -> Self {
        let mut vm = Self {
            raw: unsafe { neko_sq_open() },
            _thread: PhantomData,
        };
        vm.eval(
            "variable-access",
            r#"
            class NekoVariableAccess {
                function _get(name) { return NekoVariableGet(name) }
                function _set(name,value) { NekoVariableSet(name,value) }
            }
            var <- NekoVariableAccess()
        "#,
        )
        .expect("built-in variable proxy must compile");
        vm
    }
    pub fn result(&self) -> String {
        unsafe { CStr::from_ptr(neko_sq_result(self.raw)) }
            .to_string_lossy()
            .into_owned()
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
            error: CString::default(),
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
        result
    }
    pub fn eval(&mut self, name: &str, source: &str) -> Result<(), String> {
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn variables_are_live_aliases_across_both_languages() {
        let mut state = Variables::default();
        state.define_system(b"G200", b"first").unwrap();
        state.set(b"first", 7).unwrap();
        state
            .names
            .insert(b"constant".to_vec(), crate::runtime::Value::Constant(9));
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
