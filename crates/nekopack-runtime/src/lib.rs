//! Shared bounded title execution and raw DAT resources; no platform APIs.
pub mod dat;
mod dialogue;
mod filter;
pub mod flash;
mod motion;
mod scene;
pub mod shake;
pub mod story;
pub mod text_render;
pub mod title;
pub mod transition;
use nekopack_script::{
    runtime::{Machine, Request, Scripts, Step},
    system::{ResourceProvider, SystemVm},
};
use std::{error::Error, io::Write};
pub trait GameResources: Scripts + ResourceProvider {
    fn setup(&self) -> Result<String, String>;
    fn system(&mut self, name: &[u8]) -> Result<Vec<u8>, String>;
}
fn quote(bytes: &[u8]) -> Result<String, String> {
    let text = std::str::from_utf8(bytes).map_err(|_| "scenario input must be decoded UTF-8")?;
    Ok(format!(
        "\"{}\"",
        text.replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\r', "\\r")
            .replace('\n', "\\n")
            .replace('\t', "\\t")
    ))
}
fn array(items: &[Vec<u8>]) -> Result<String, String> {
    Ok(format!(
        "[{}]",
        items
            .iter()
            .map(|v| quote(v))
            .collect::<Result<Vec<_>, _>>()?
            .join(",")
    ))
}
pub fn run_title(
    store: &mut impl GameResources,
    choice: i32,
    output: &mut impl Write,
) -> Result<(), Box<dyn Error>> {
    bootstrap(store, choice, output).map(|_| ())
}
fn bootstrap(
    store: &mut impl GameResources,
    choice: i32,
    output: &mut impl Write,
) -> Result<(Machine, SystemVm), Box<dyn Error>> {
    bootstrap_begin(store, choice, output, false)
}
fn bootstrap_begin(
    store: &mut impl GameResources,
    choice: i32,
    output: &mut impl Write,
    stop_at_menu: bool,
) -> Result<(Machine, SystemVm), Box<dyn Error>> {
    let machine = Machine::new(store.load(b"start")?, 10000, 64);
    let mut system = SystemVm::try_new()?;
    let metadata = store.setup()?;
    system.eval("assets", &metadata)?;
    system.eval("choice", &format!("InputChoice <- {choice}"))?;
    system.eval(
        "host",
        include_str!("../../nekopack-script/host/headless.nut"),
    )?;
    bootstrap_continue(store, machine, system, output, stop_at_menu)
}
fn bootstrap_continue(
    store: &mut impl GameResources,
    mut machine: Machine,
    mut system: SystemVm,
    output: &mut impl Write,
    stop_at_menu: bool,
) -> Result<(Machine, SystemVm), Box<dyn Error>> {
    let mut modules = 0;
    let mut menus = 0;
    let mut entered = false;
    loop {
        let step = machine
            .advance(store)
            .map_err(|e| format!("{:?}: {e}", machine.location()))?;
        match step {
            Step::Finished => return Err("script ended before title menu".into()),
            Step::Request(request) => {
                let (source, line) = machine.location().unwrap();
                writeln!(
                    output,
                    "location\t{}\t{}\t{}",
                    String::from_utf8_lossy(source),
                    line,
                    machine.depth()
                )?;
                if menus > 0 {
                    if !entered {
                        writeln!(
                            output,
                            "after-menu\t{}\t{}",
                            String::from_utf8_lossy(source),
                            line
                        )?;
                        entered = true;
                    }
                    if !matches!(&request,Request::Command{name,..} if name==b"route") {
                        writeln!(
                            output,
                            "story-boundary\t{}\t{}\t{}",
                            String::from_utf8_lossy(source),
                            line,
                            machine.depth()
                        )?;
                        break;
                    }
                }
                match request {
                    Request::Speaker(_) | Request::Text(_) => {
                        return Err("unexpected dialogue during title bootstrap".into())
                    }
                    Request::System(name) => {
                        let data = store.system(&name)?;
                        system
                            .eval_with_resources(
                                std::str::from_utf8(&name)?,
                                std::str::from_utf8(&data)?,
                                &mut machine.variables,
                                store,
                            )
                            .map_err(|e| {
                                format!("module {}: {e}", String::from_utf8_lossy(&name))
                            })?;
                        writeln!(output, "system\t{}", String::from_utf8_lossy(&name))?;
                        modules += 1;
                        machine.complete(None)?;
                    }
                    Request::Command { name, args, rows } => {
                        if stop_at_menu && name == b"map" {
                            return Ok((machine, system));
                        }
                        writeln!(
                            output,
                            "command\t{}\t{}\t{}",
                            String::from_utf8_lossy(&name),
                            args.len(),
                            rows.len()
                        )?;
                        if [b"img".as_slice(), b"bgm", b"wait", b"fade"].contains(&name.as_slice())
                        {
                            system.eval_with_resources(
                                "native-effect",
                                &format!("NativeCommand({},{})", quote(&name)?, array(&args)?),
                                &mut machine.variables,
                                store,
                            )?;
                            machine.complete(None)?;
                        } else {
                            let invoke=format!("local f=com[{}]\nlocal result=f.call(getroottable(),{}{})\nreturn result==null ? \"null\" : result.tostring()",quote(&name)?,array(&args)?,if name==b"map" {format!(",{}",array(&rows)?)} else {String::new()});
                            system
                                .eval_with_resources(
                                    "dispatch",
                                    &invoke,
                                    &mut machine.variables,
                                    store,
                                )
                                .map_err(|e| {
                                    format!("command {}: {e}", String::from_utf8_lossy(&name))
                                })?;
                            if name == b"map" {
                                let selected = system.result().parse::<i32>()?;
                                machine.complete(Some(selected))?;
                                menus += 1;
                                writeln!(output, "menu-result\t{selected}")?;
                            } else {
                                machine.complete(None)?;
                            }
                        }
                    }
                }
            }
        }
    }
    system.eval(
        "report",
        r#"
        local keys=[]
        foreach(k,v in com.entries)keys.append(k)
        keys.sort()
        local s=""
        foreach(k in keys)s+=k+","
        return s+"\nframes\t"+Host.frames.len()+"\neffects\t"+Host.effects.len()
    "#,
    )?;
    writeln!(output, "registered\t{}", system.result())?;
    writeln!(
        output,
        "complete\t{modules}\t{}\t{}",
        machine.steps(),
        machine.variables.slots().len()
    )?;
    for ((bank, index), value) in machine.variables.slots() {
        writeln!(output, "slot\t{}\t{index}\t{value}", char::from(*bank))?;
    }
    Ok((machine, system))
}

#[cfg(test)]
mod bootstrap_tests {
    use super::*;
    use nekopack_script::runtime::Source;
    struct Store;
    impl Scripts for Store {
        fn load(&mut self, name: &[u8]) -> Result<Source, String> {
            let bytes = match name {
                b"start" => b".var L1 counter\n? counter = 7\n.system menu.nut\n.map title\n Start\n.case 1\n.load story\n.end\n".as_slice(),
                b"story" => b".img bg\n",
                _ => return Err("unexpected script".into()),
            };
            Ok(Source {
                name: name.to_vec(),
                bytes: bytes.to_vec(),
            })
        }
    }
    impl ResourceProvider for Store {
        fn resource(&mut self, _: &str, _: &str) -> Result<String, String> {
            Err("unexpected resource".into())
        }
    }
    impl GameResources for Store {
        fn setup(&self) -> Result<String, String> {
            Ok(String::new())
        }
        fn system(&mut self, _: &[u8]) -> Result<Vec<u8>, String> {
            Ok(b"MenuCalls <- 0\ncom[\"map\"] = function(args,rows){MenuCalls++;return InputChoice}".to_vec())
        }
    }
    #[test]
    fn title_pause_retains_variables_vm_and_pending_map_until_dispatch() {
        let mut store = Store;
        let (mut machine, mut system) =
            bootstrap_begin(&mut store, 1, &mut std::io::sink(), true).unwrap();
        let steps = machine.steps();
        let request = machine.advance(&mut store).unwrap();
        assert_eq!(machine.advance(&mut store).unwrap(), request);
        assert_eq!(machine.steps(), steps);
        assert_eq!(machine.variables.get(b"counter").unwrap(), 7);
        system.eval("calls", "return MenuCalls.tostring()").unwrap();
        assert_eq!(system.result(), "0");
        machine.variables.set(b"counter", 9).unwrap();
        let (machine, mut system) =
            bootstrap_continue(&mut store, machine, system, &mut std::io::sink(), false).unwrap();
        assert_eq!(machine.location().unwrap().0, b"story");
        assert_eq!(machine.variables.get(b"counter").unwrap(), 9);
        system.eval("calls", "return MenuCalls.tostring()").unwrap();
        assert_eq!(system.result(), "1");
    }
}
