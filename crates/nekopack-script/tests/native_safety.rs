//! Native fault cases run in bounded child processes so a crash cannot kill cargo test.
use nekopack_script::{runtime::Variables, system::SystemVm};
use std::{
    process::Command,
    thread,
    time::{Duration, Instant},
};

fn probe(name: &str) {
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "native_probe", "--ignored", "--nocapture"])
        .env("NEKO_NATIVE_PROBE", name)
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            let status = child.wait().unwrap();
            panic!("{name} timed out: {status}");
        }
        thread::sleep(Duration::from_millis(10));
    }
    let status = child.wait().unwrap();
    assert!(status.success(), "{name}: {status}");
}

#[test]
fn coroutine_variable_context() {
    probe("coroutine");
}
#[test]
fn malformed_formats_are_errors() {
    probe("format-invalid");
}
#[test]
fn format_integer_width_and_types() {
    probe("format-valid");
}
#[test]
fn checked_integer_arithmetic() {
    probe("integer");
}

#[test]
fn root_suspend_is_an_error_and_preserves_pending_request() {
    probe("root-suspend");
}

#[test]
fn bytecode_budget_cannot_be_caught_or_reset_by_coroutines() {
    probe("budget");
}

#[test]
#[ignore = "executed by bounded subprocess tests"]
fn native_probe() {
    let name = std::env::var("NEKO_NATIVE_PROBE").expect("probe scenario");
    match name.as_str() {
        "root-suspend" => {
            use nekopack_script::runtime::{Machine, Scripts, Source};
            struct Empty;
            impl Scripts for Empty {
                fn load(&mut self, _: &[u8]) -> Result<Source, String> {
                    Err("no source".into())
                }
            }
            let mut machine = Machine::new(
                Source {
                    name: b"root".to_vec(),
                    bytes: b".system suspend\n.done".to_vec(),
                },
                100,
                4,
            );
            let pending = machine.advance(&mut Empty).unwrap();
            let mut vm = SystemVm::new();
            vm.eval("previous", "return \"old result\"").unwrap();
            let error = vm
                .eval_with_variables(
                    "suspend",
                    "suspend(); throw \"must not resume\"",
                    &mut machine.variables,
                )
                .unwrap_err();
            assert!(error.contains("root VM suspension"), "{error}");
            assert_eq!(vm.result(), "");
            assert_eq!(machine.advance(&mut Empty).unwrap(), pending);
            assert_eq!(machine.location().unwrap().1, 1);
            vm.eval("recovery", "return \"complete\"").unwrap();
            assert_eq!(vm.result(), "complete");
            machine.complete(None).unwrap();
            assert_eq!(machine.location().unwrap().1, 2);
            vm.eval("caught", "try {suspend()} catch(e) {} return \"finished\"")
                .unwrap();
            assert_eq!(vm.result(), "finished");
        }
        "budget" => {
            for source in [
                "while(true){}",
                "while(true){try {while(true){}} catch(e){}}",
                "local co=newthread(function(){while(true){}}); try {co.call()} catch(e){}",
                "local co=newthread(function(){while(true){suspend()}}); co.call(); while(true){co.wakeup()}",
                "function recurse(){return recurse()} recurse()",
                "seterrorhandler(function(e){while(true){}}); while(true){}",
            ] {
                let mut vm = SystemVm::new();
                vm.set_instruction_budget(1000);
                let mut vars = Variables::default();
                let error = vm.eval_with_variables("budget", source, &mut vars).unwrap_err();
                assert!(error.contains("instruction budget exceeded"), "{source}: {error}");
                // A failed execution is terminal, with no hidden reset or partial replay.
                vm.set_instruction_budget(10_000);
                assert!(vm.eval("after-limit", "return 1").unwrap_err().contains("VM stopped"));
            }
            let mut vm = SystemVm::new();
            vm.set_instruction_budget(0);
            assert!(vm
                .eval("zero", "return 1")
                .unwrap_err()
                .contains("instruction budget"));
            let mut vm = SystemVm::new();
            vm.set_instruction_budget(1000);
            for _ in 0..3 {
                vm.eval(
                    "finite",
                    "local n=0; for(local i=0;i<10;i++)n+=i; return n.tostring()",
                )
                .unwrap();
                assert_eq!(vm.result(), "45");
            }
        }
        "coroutine" => {
            for iteration in 0..16 {
                let mut vm = SystemVm::new();
                let mut vars = Variables::default();
                vm.eval_with_variables(
                    "create",
                    r#"
                    def("G1","score")
                    co <- newthread(function(){
                        def("G1","alias")
                        var.alias=7
                        suspend()
                        var.score+=3
                    })
                    co.call()
                "#,
                    &mut vars,
                )
                .unwrap_or_else(|e| panic!("create iteration {iteration}: {e}; {vars:?}"));
                assert_eq!(vars.get(b"score").unwrap(), 7);
                vars.set(b"score", 20).unwrap();
                vm.eval_with_variables("resume", "co.wakeup()", &mut vars)
                    .unwrap();
                assert_eq!(vars.get(b"alias").unwrap(), 23);
                let error = vm
                    .eval(
                        "unbound",
                        r#"newthread(function(){return NekoVariableGet("score")}).call()"#,
                    )
                    .unwrap_err();
                assert!(error.contains("variable state is not bound"), "{error}");
                assert!(vm
                    .eval_with_variables(
                        "throw",
                        r#"
                    newthread(function(){var.score=30;throw "expected"}).call()
                "#,
                        &mut vars
                    )
                    .is_err());
                vm.eval_with_variables("after-error", "var.alias=31", &mut vars)
                    .unwrap();
                assert_eq!(vars.get(b"score").unwrap(), 31);
            }
        }
        "format-invalid" => {
            let mut vm = SystemVm::new();
            for format in [
                "%",
                "%--",
                "%.",
                "%--------------------d",
                "%00000000000000000000d",
                "%100d",
                "%.100f",
                "%q",
                "%lld",
            ] {
                let code = format!("return format(\"{format}\",1)");
                assert!(
                    vm.eval("invalid-format", &code).is_err(),
                    "accepted {format}"
                );
                vm.eval("recovery", r#"return format("%03d",7)"#).unwrap();
                assert_eq!(vm.result(), "007");
            }
        }
        "format-valid" => {
            let mut vm = SystemVm::new();
            vm.eval(
                "formats",
                r#"
                if(format("%03d",7)!="007")throw "padding"
                if(format("%+d",-17)!="-17")throw "signed"
                if(format("%#x",255)!="0xff")throw "hex"
                if(format("%.2f",1.25)!="1.25")throw "float"
                if(format("%5.2s","abcd")!="   ab")throw "string"
                if(format("%c%%",65)!="A%")throw "char"
                if(_intsize_==8 && format("%d",4294967297)!="4294967297")throw "integer width"
                if(_intsize_==8 && format("%x",4294967297)!="100000001")throw "hex width"
            "#,
            )
            .unwrap();
        }
        "integer" => {
            let mut vm = SystemVm::new();
            vm.eval(
                "limits",
                r#"
                bits <- _intsize_*8
                maximum <- ((1 << (bits-2))-1)*2+1
                minimum <- -maximum-1
                if(!(minimum<maximum) || !(maximum>minimum))throw "comparison"
                if((minimum % -1)!=0)throw "remainder"
                if((-2 >> 1)!=-1)throw "signed shift"
                if((-1 >>> 1)!=maximum)throw "unsigned shift"
                if((1 << (bits-1))!=minimum)throw "sign bit"
                if((-1 << 1)!=-2)throw "bit shift"
                if((maximum-1)+1!=maximum || (minimum+1)-1!=minimum)throw "edge"
            "#,
            )
            .unwrap();
            for expr in [
                "maximum+1",
                "minimum-1",
                "maximum*2",
                "minimum*-1",
                "minimum/-1",
                "-minimum",
                "1 << -1",
                "1 << bits",
                "1 >> bits",
                "1 >>> -1",
            ] {
                assert!(
                    vm.eval("invalid-integer", &format!("return {expr}"))
                        .is_err(),
                    "accepted {expr}"
                );
                vm.eval("recovery", "if(6*7!=42)throw \"bad recovery\"")
                    .unwrap();
            }
        }
        _ => panic!("unknown probe {name}"),
    }
}
