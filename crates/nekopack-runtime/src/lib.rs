//! Shared bounded title execution and raw DAT resources; no platform APIs.
pub mod dat;
mod filter;
pub mod flash;
mod scene;
pub mod shake;
pub mod story;
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
    let mut machine = Machine::new(store.load(b"start")?, 10000, 64);
    let mut system = SystemVm::try_new()?;
    let metadata = store.setup()?;
    system.eval("assets", &metadata)?;
    system.eval("choice", &format!("InputChoice <- {choice}"))?;
    system.eval(
        "host",
        include_str!("../../nekopack-script/host/headless.nut"),
    )?;
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
