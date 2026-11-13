//! Synthetic nested exit contract: no mounted game text or artwork.
use nekopack_script::{runtime::Variables, system::{ResourceProvider, SystemVm}};

struct Resources;
impl ResourceProvider for Resources {
    fn resource(&mut self, kind: &str, name: &str) -> Result<String, String> {
        assert_eq!((kind, name), ("Assets", "dialog"));
        Ok(r#"{
            BASE=["system/synthetic.mng:0",0,0,100,60],
            QUIT=["system/synthetic.mng:1",0,0,100,60],
            ["= OK"]=["system/synthetic.mng:2",10,20,30,20],
            ["= CANCEL"]=["system/synthetic.mng:3",60,20,30,20]
        }"#.into())
    }
}

fn host() -> SystemVm {
    let mut vm = SystemVm::new();
    for (name, source) in [
        ("headless", include_str!("../../nekopack-script/host/headless.nut")),
        ("command", include_str!("../host/system_command.nut")),
        ("gui", include_str!("../host/system_gui.nut")),
        ("exit", include_str!("../host/system_exit.nut")),
        ("dialog", include_str!("fixtures/system_gui_dialog.nut")),
    ] {
        vm.eval(name, source).unwrap();
    }
    vm.eval("synthetic original quit caller", r#"
        Config <- {Dialog={QUIT=true},AutoSelect=false,Effect=1}
        function NativeAudioConfigSnapshot(){return "synthetic audio metadata"}
        InQuit <- false
        ID_QUIT <- 91
        AfterQuit <- false
        function QuitGame(){throw "unbound synthetic quit"}
        SavedQuit <- QuitGame
        function InvokeCommand(code){
            if(InQuit)return false
            wait(0)
            function Invoke_(code){
                switch(code){
                case ID_QUIT:
                    InQuit=true
                    if(ViewDialog("QUIT")){QuitGame();throw null}
                    else{InQuit=false;return false}
                }
                return false
            }
            if(Invoke_(code)){
                wait(0)
                return true
            }
            return false
        }
        com["quit"] = function(args){InvokeCommand(ID_QUIT);AfterQuit=true}
        SystemCommandInstall("quit",[],null)
    "#).unwrap();
    vm
}

fn step(vm: &mut SystemVm, event: i32, pointer: Option<[i32; 2]>) -> Result<String, String> {
    let point = pointer.map_or("null".into(), |[x,y]| format!("[{x},{y}]"));
    vm.eval_with_resources("step", &format!("return SystemCommandStep({event},false,0,{point})"),
        &mut Variables::default(), &mut Resources)?;
    Ok(vm.result())
}

fn open_dialog(vm: &mut SystemVm) {
    // InvokeCommand's initial wait, then ViewDialog's initial wait.
    for _ in 0..2 {
        let out = step(vm, 0, None).unwrap();
        assert!(out.contains("W\t0\n"), "{out}");
        assert!(!out.contains("U\tquit\n"), "{out}");
    }
    let frame = step(vm, 0, Some([0,0])).unwrap();
    assert!(frame.contains("B\t0:OK:2000000001\t10\t20\t30\t20\n"), "{frame}");
    assert!(frame.contains("W\t-1\n"), "{frame}");
}

#[test]
fn nested_quit_cancel_resumes_original_caller_without_terminal_action() {
    for (event, pointer) in [(2_000_000_002, None), (2_000_000_001, Some([65,25]))] {
        let mut vm = host();
        open_dialog(&mut vm);
        let closing = step(&mut vm, event, pointer).unwrap();
        assert!(closing.contains("W\t0\n"), "{closing}");
        assert!(!closing.contains("U\tquit\n"), "{closing}");
        let done = step(&mut vm, 0, None).unwrap();
        assert_eq!(done, "R\tnull\n");
        vm.eval("cancel state", r#"return format("%s %s %s", InQuit.tostring(),AfterQuit.tostring(),(SystemCommandHost.terminal_action==null && QuitGame==SavedQuit).tostring())"#).unwrap();
        assert_eq!(vm.result(), "false true true");
    }
}

#[test]
fn nested_quit_confirm_emits_terminal_action_only_after_dialog_final_wait() {
    let mut vm = host();
    open_dialog(&mut vm);
    let closing = step(&mut vm, 2_000_000_001, Some([15,25])).unwrap();
    assert!(closing.contains("W\t0\n"), "{closing}");
    assert!(!closing.contains("U\tquit\n"), "{closing}");
    vm.eval("before final wait", "return (SystemCommandHost.terminal_action==null && !AfterQuit).tostring()").unwrap();
    assert_eq!(vm.result(), "true");
    let done = step(&mut vm, 0, None).unwrap();
    assert_eq!(done, "U\tquit\nR\tnull\n");
    vm.eval("terminal state", r#"return format("%s %s %s",SystemCommandHost.terminal_action,AfterQuit.tostring(),(QuitGame==SavedQuit).tostring())"#).unwrap();
    assert_eq!(vm.result(), "quit false true");
}

#[test]
fn unrelated_null_throw_is_an_error_instead_of_normal_completion() {
    let mut vm = host();
    vm.eval("unexpected null", r#"
        com["bad"] = function(args){throw null}
        SystemCommandInstall("bad",[],null)
    "#).unwrap();
    assert!(step(&mut vm, 0, None).is_err());
}

#[test]
fn explicit_quit_does_not_suppress_non_null_failure() {
    let mut vm = host();
    vm.eval("unexpected failure", r#"
        com["bad"] = function(args){QuitGame();throw "synthetic failure"}
        SystemCommandInstall("bad",[],null)
    "#).unwrap();
    let error = step(&mut vm, 0, None).unwrap_err();
    assert!(error.contains("synthetic failure"), "{error}");
}
