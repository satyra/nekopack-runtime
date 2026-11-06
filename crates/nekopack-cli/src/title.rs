//! Headless execution adapter for explicitly decoded UTF-8 sample trees.
use nekopack_script::{
    runtime::{Machine, Request, Scripts, Source, Step},
    system::SystemVm,
};
use std::{
    error::Error,
    io::Write,
    path::{Path, PathBuf},
};
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
struct Store {
    root: PathBuf,
}
impl Store {
    fn read(&self, dir: &str, name: &[u8], extension: &str) -> Result<Vec<u8>, String> {
        let name = std::str::from_utf8(name).map_err(|_| "invalid resource name")?;
        if name.contains(['/', '\\', '\0']) || name == ".." {
            return Err("resource name is not a basename".into());
        }
        // Decoded fixtures retain on-disk spelling; scenario resource names use
        // ASCII-insensitive lookup on every host, including case-sensitive Android.
        let wanted = format!("{name}{extension}");
        let mut matches = std::fs::read_dir(self.root.join(dir))
            .map_err(|e| e.to_string())?
            .filter_map(|entry| match entry {
                Ok(entry)
                    if entry
                        .file_name()
                        .to_str()
                        .is_some_and(|n| n.eq_ignore_ascii_case(&wanted)) =>
                {
                    Some(Ok(entry.path()))
                }
                Ok(_) => None,
                Err(error) => Some(Err(error.to_string())),
            });
        let path = matches
            .next()
            .ok_or_else(|| format!("missing fixture resource {wanted}"))??;
        if matches.next().transpose()?.is_some() {
            return Err(format!("case-ambiguous fixture resource {wanted}"));
        }
        let data = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        if data.len() > 64 * 1024 * 1024 {
            return Err("script exceeds 64 MiB budget".into());
        }
        Ok(data)
    }
}

impl Scripts for Store {
    fn load(&mut self, name: &[u8]) -> Result<Source, String> {
        Ok(Source {
            name: name.to_vec(),
            bytes: self.read("script", name, ".txt")?,
        })
    }
}
pub fn run(root: &Path, choice: i32) -> Result<(), Box<dyn Error>> {
    run_to(root, choice, &mut std::io::stdout().lock())
}

/// Shared headless validation path; Android probe uses this exact adapter.
pub fn run_to(root: &Path, choice: i32, output: &mut impl Write) -> Result<(), Box<dyn Error>> {
    let mut store = Store {
        root: root.to_owned(),
    };
    let mut machine = Machine::new(store.load(b"start")?, 10000, 64);
    let mut system = SystemVm::try_new()?;
    let metadata = std::fs::read_to_string(root.join("assets.nut"))?;
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
            .advance(&mut store)
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
                    Request::System(name) => {
                        let data = store.read("system", &name, "")?;
                        system
                            .eval_with_variables(
                                std::str::from_utf8(&name)?,
                                std::str::from_utf8(&data)?,
                                &mut machine.variables,
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
                            system.eval_with_variables(
                                "native-effect",
                                &format!("NativeCommand({},{})", quote(&name)?, array(&args)?),
                                &mut machine.variables,
                            )?;
                            machine.complete(None)?;
                        } else {
                            let invoke=format!("local f=com[{}]\nlocal result=f.call(getroottable(),{}{})\nreturn result==null ? \"null\" : result.tostring()",quote(&name)?,array(&args)?,if name==b"map" {format!(",{}",array(&rows)?)} else {String::new()});
                            system
                                .eval_with_variables("dispatch", &invoke, &mut machine.variables)
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
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fixture_lookup_is_ascii_case_insensitive() {
        let root = std::env::temp_dir().join(format!(
            "neko-fixture-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("script")).unwrap();
        std::fs::write(root.join("script/p01.txt"), b".done").unwrap();
        let mut store = Store { root: root.clone() };
        assert_eq!(store.load(b"P01").unwrap().bytes, b".done");
        assert!(store.load(b"missing").is_err());
        #[cfg(unix)]
        {
            std::fs::write(root.join("script/P01.txt"), b"other").unwrap();
            assert!(store.load(b"P01").unwrap_err().contains("case-ambiguous"));
        }
        std::fs::remove_dir_all(root).unwrap();
    }
}
