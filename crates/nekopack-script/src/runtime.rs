//! Cooperative script state. Host effects are acknowledged before the PC advances.
use crate::{parse_bootstrap, Statement};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    Constant(i32),
    Slot(u8, u32),
}
#[derive(Default, Debug)]
pub struct Variables {
    pub names: BTreeMap<Vec<u8>, Value>,
    pub slots: BTreeMap<(u8, u32), i32>,
}
impl Variables {
    /// Observed system `def(address, alias)` profile. GF aliases are read as @alias.
    /// Flag storage is separate from integer G slots; packed original layout is unverified.
    pub fn define_system(&mut self, address: &[u8], alias: &[u8]) -> Result<(), String> {
        let (bank, digits, prefix) = if let Some(n) = address.strip_prefix(b"GF") {
            (b'F', n, true)
        } else if let Some(n) = address.strip_prefix(b"G") {
            (b'G', n, false)
        } else if let Some(n) = address.strip_prefix(b"L") {
            (b'L', n, false)
        } else {
            return Err("unsupported system variable address".into());
        };
        if digits.is_empty() || !digits.iter().all(u8::is_ascii_digit) {
            return Err("invalid system variable address".into());
        }
        let index = std::str::from_utf8(digits)
            .unwrap()
            .parse::<u32>()
            .map_err(|_| "slot address overflow")?;
        if alias.is_empty() || alias.contains(&0) {
            return Err("invalid variable alias".into());
        }
        let name = if prefix {
            [b"@".as_slice(), alias].concat()
        } else {
            alias.to_vec()
        };
        let slot = Value::Slot(bank, index);
        if let Some(existing) = self.names.get(&name) {
            if existing != &slot {
                return Err("conflicting variable alias".into());
            }
            return Ok(());
        }
        self.names.insert(name, slot);
        self.slots.entry((bank, index)).or_insert(0);
        Ok(())
    }
    pub fn get(&self, name: &[u8]) -> Result<i32, String> {
        match self.names.get(name) {
            Some(Value::Constant(v)) => Ok(*v),
            Some(Value::Slot(b, i)) => Ok(self.slots[&(*b, *i)]),
            None => Err(format!("undefined variable {name:?}")),
        }
    }
    pub fn set(&mut self, name: &[u8], value: i32) -> Result<(), String> {
        match self.names.get(name) {
            Some(Value::Slot(b, i)) => {
                self.slots.insert((*b, *i), value);
                Ok(())
            }
            _ => Err(format!(
                "assignment requires declared mutable variable {name:?}"
            )),
        }
    }
}
#[derive(Clone, Debug)]
pub struct Source {
    pub name: Vec<u8>,
    pub bytes: Vec<u8>,
}
#[derive(Clone, Debug)]
struct Frame {
    source: Source,
    lines: Vec<(usize, Vec<u8>)>,
    pc: usize,
    branch_end: Option<usize>,
}
impl Frame {
    fn new(source: Source) -> Self {
        let lines = source
            .bytes
            .split(|b| *b == b'\n')
            .enumerate()
            .map(|(i, l)| (i + 1, l.trim_ascii().to_vec()))
            .collect();
        Self {
            source,
            lines,
            pc: 0,
            branch_end: None,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    System(Vec<u8>),
    Command {
        name: Vec<u8>,
        args: Vec<Vec<u8>>,
        rows: Vec<Vec<u8>>,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    Request(Request),
    Finished,
}
pub trait Scripts {
    fn load(&mut self, name: &[u8]) -> Result<Source, String>;
}
pub struct Machine {
    pub variables: Variables,
    stack: Vec<Frame>,
    pending: Option<Request>,
    pub steps: usize,
    max_steps: usize,
    max_depth: usize,
}
impl Machine {
    pub fn new(source: Source, max_steps: usize, max_depth: usize) -> Self {
        Self {
            variables: Variables::default(),
            stack: vec![Frame::new(source)],
            pending: None,
            steps: 0,
            max_steps,
            max_depth,
        }
    }
    pub fn location(&self) -> Option<(&[u8], usize)> {
        self.stack.last().map(|f| {
            (
                f.source.name.as_slice(),
                f.lines.get(f.pc).map_or(f.lines.len() + 1, |l| l.0),
            )
        })
    }
    pub fn depth(&self) -> usize {
        self.stack.len()
    }
    pub fn advance(&mut self, scripts: &mut impl Scripts) -> Result<Step, String> {
        if let Some(request) = &self.pending {
            return Ok(Step::Request(request.clone()));
        }
        loop {
            let Some(frame) = self.stack.last_mut() else {
                return Ok(Step::Finished);
            };
            if frame.pc == frame.lines.len() {
                self.stack.pop();
                continue;
            }
            if self.steps == self.max_steps {
                return Err("instruction budget exceeded".into());
            }
            let raw = frame.lines[frame.pc].1.clone();
            if raw.is_empty() || raw.starts_with(b";") {
                frame.pc += 1;
                continue;
            }
            self.steps += 1;
            if raw.starts_with(b".case ") || raw == b".end" {
                let end = frame
                    .branch_end
                    .take()
                    .ok_or("case/end outside selected map branch")?;
                frame.pc = end + 1;
                continue;
            }
            let word = raw.split(|b| b.is_ascii_whitespace()).next().unwrap();
            if [b".def".as_slice(), b".var", b".call", b".load", b".system"].contains(&word)
                || raw.starts_with(b"?")
            {
                let ast = parse_bootstrap(&raw, 1).map_err(|e| e.to_string())?;
                match &ast[0].statement {
                    Statement::Define { name, value } => {
                        if self.variables.names.contains_key(*name) {
                            return Err("duplicate name declaration".into());
                        }
                        self.variables
                            .names
                            .insert(name.to_vec(), Value::Constant(*value));
                    }
                    Statement::Variables { bank, first, names } => {
                        let last = first
                            .checked_add(
                                u32::try_from(names.len() - 1).map_err(|_| "too many variables")?,
                            )
                            .ok_or("slot address overflow")?;
                        let mut seen = std::collections::BTreeSet::new();
                        if names
                            .iter()
                            .any(|n| self.variables.names.contains_key(*n) || !seen.insert(*n))
                        {
                            return Err("duplicate variable declaration".into());
                        }
                        for (name, index) in names.iter().zip(*first..=last) {
                            self.variables
                                .names
                                .insert(name.to_vec(), Value::Slot(*bank, index));
                            self.variables.slots.entry((*bank, index)).or_insert(0);
                        }
                    }
                    Statement::Assign { name, value } => self.variables.set(name, *value)?,
                    Statement::Call(name) => {
                        if self.stack.len() >= self.max_depth {
                            return Err("call depth exceeded".into());
                        }
                        let next = Frame::new(scripts.load(name)?);
                        self.stack.last_mut().unwrap().pc += 1;
                        self.stack.push(next);
                        continue;
                    }
                    Statement::Load(name) => {
                        let next = Frame::new(scripts.load(name)?);
                        *self.stack.last_mut().unwrap() = next;
                        continue;
                    }
                    Statement::System(name) => {
                        let request = Request::System(name.to_vec());
                        self.pending = Some(request.clone());
                        return Ok(Step::Request(request));
                    }
                }
                self.stack.last_mut().unwrap().pc += 1;
            } else if word.starts_with(b".") {
                let mut parts = raw
                    .split(|b| b.is_ascii_whitespace())
                    .filter(|p| !p.is_empty());
                let name = parts.next().unwrap()[1..].to_vec();
                let args = parts.map(|p| p.to_vec()).collect();
                let mut rows = Vec::new();
                if name == b"map" {
                    for (_, row) in &frame.lines[frame.pc + 1..] {
                        if row.starts_with(b".") {
                            break;
                        }
                        if !row.is_empty() && !row.starts_with(b";") {
                            rows.push(row.clone());
                        }
                    }
                }
                let request = Request::Command { name, args, rows };
                self.pending = Some(request.clone());
                return Ok(Step::Request(request));
            } else {
                return Err(format!(
                    "unsupported statement at {:?}:{}",
                    frame.source.name, frame.lines[frame.pc].0
                ));
            }
        }
    }
    /// Successful host completion only. A failed/blocked host must not call this.
    pub fn complete(&mut self, result: Option<i32>) -> Result<(), String> {
        let request = self.pending.as_ref().ok_or("no pending request")?;
        let frame = self.stack.last_mut().unwrap();
        if matches!(request,Request::Command{name,..} if name==b"map") {
            let choice = result.ok_or("map requires integer result")?;
            let mut selected = None;
            let mut end = None;
            for (i, (_, line)) in frame.lines.iter().enumerate().skip(frame.pc + 1) {
                if line == b".end" {
                    end = Some(i);
                    break;
                }
                if let Some(value) = line.strip_prefix(b".case ") {
                    let id = std::str::from_utf8(value)
                        .map_err(|_| "non-ASCII case")?
                        .trim()
                        .parse::<i32>()
                        .map_err(|_| "invalid case")?;
                    if id == choice {
                        if selected.is_some() {
                            return Err("duplicate selected case".into());
                        }
                        selected = Some(i + 1);
                    }
                }
            }
            let end = end.ok_or("unterminated map cases")?;
            let pc = selected.ok_or("map result has no matching case")?;
            frame.pc = pc;
            frame.branch_end = Some(end);
        } else {
            if result.is_some() {
                return Err("unexpected host result".into());
            }
            frame.pc += 1;
        }
        self.pending = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Store;
    impl Scripts for Store {
        fn load(&mut self, name: &[u8]) -> Result<Source, String> {
            let text = match name {
                b"patch" => b"? x = 7".as_slice(),
                b"title" => b".map title\n start\n.case 1\n.call patch\n.end\n.done",
                _ => return Err("missing script".into()),
            };
            Ok(Source {
                name: name.to_vec(),
                bytes: text.to_vec(),
            })
        }
    }
    #[test]
    fn calls_return_load_replaces_and_menu_resumes() {
        let mut vm = Machine::new(
            Source {
                name: b"start".to_vec(),
                bytes: b".var L1 x\n.call patch\n.system gui.nut\n.load title\n? x = 99".to_vec(),
            },
            100,
            8,
        );
        let mut store = Store;
        assert_eq!(
            vm.advance(&mut store).unwrap(),
            Step::Request(Request::System(b"gui.nut".to_vec()))
        );
        assert_eq!(vm.variables.get(b"x").unwrap(), 7);
        assert_eq!(vm.depth(), 1);
        let before = vm.steps;
        vm.advance(&mut store).unwrap();
        assert_eq!(vm.steps, before);
        vm.complete(None).unwrap();
        assert!(
            matches!(vm.advance(&mut store).unwrap(),Step::Request(Request::Command{name,..}) if name==b"map")
        );
        assert!(vm.complete(Some(99)).is_err());
        vm.complete(Some(1)).unwrap();
        assert!(
            matches!(vm.advance(&mut store).unwrap(),Step::Request(Request::Command{name,..}) if name==b"done")
        );
        vm.complete(None).unwrap();
        assert_eq!(vm.advance(&mut store).unwrap(), Step::Finished);
        assert_eq!(vm.variables.get(b"x").unwrap(), 7);
    }
    #[test]
    fn errors_do_not_skip_calls_or_effects() {
        let mut vm = Machine::new(
            Source {
                name: b"bad".to_vec(),
                bytes: b".call absent\n.done".to_vec(),
            },
            10,
            4,
        );
        assert!(vm.advance(&mut Store).is_err());
        assert_eq!(vm.location().unwrap().1, 1);
        let mut vars = Variables::default();
        vars.names.insert(b"c".to_vec(), Value::Constant(1));
        assert!(vars.set(b"c", 2).is_err());
        assert!(vars.get(b"missing").is_err());
    }
}
