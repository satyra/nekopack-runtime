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
    names: BTreeMap<Vec<u8>, Value>,
    slots: BTreeMap<(u8, u32), i32>,
}
impl Variables {
    pub fn slots(&self) -> &BTreeMap<(u8, u32), i32> {
        &self.slots
    }
    pub fn define_constant(&mut self, name: &[u8], value: i32) -> Result<(), String> {
        if name.is_empty() || name.contains(&0) {
            return Err("invalid variable name".into());
        }
        if self.names.contains_key(name) {
            return Err("duplicate name declaration".into());
        }
        self.names.insert(name.to_vec(), Value::Constant(value));
        Ok(())
    }
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
            Some(Value::Slot(b, i)) => self
                .slots
                .get(&(*b, *i))
                .copied()
                .ok_or_else(|| "invalid variable slot state".into()),
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
    branch_ends: Vec<usize>,
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
            branch_ends: Vec::new(),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    Speaker(Vec<Vec<u8>>),
    Text(Vec<u8>),
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
    steps: usize,
    max_steps: usize,
    dialogue: bool,
    max_depth: usize,
}
impl Machine {
    pub fn pending_text(&self) -> Option<&[u8]> {
        if let Some(Request::Text(text)) = &self.pending {
            Some(text)
        } else {
            None
        }
    }
    /// Save only at a pending text boundary; native Squirrel stacks are never serialized.
    pub fn save_dialogue(&self) -> Result<Vec<u8>, String> {
        if !matches!(self.pending, Some(Request::Text(_))) {
            return Err("save requires pending text".into());
        }
        let mut w = crate::checkpoint::Writer::default();
        w.number(self.max_steps as u64);
        w.number(self.max_depth as u64);
        w.number(self.steps as u64);
        w.number(self.stack.len() as u64);
        for frame in &self.stack {
            w.bytes(&frame.source.name);
            w.number(crate::checkpoint::digest(&frame.source.bytes));
            w.number(frame.pc as u64);
            w.number(frame.branch_ends.len() as u64);
            for end in &frame.branch_ends {
                w.number(*end as u64);
            }
        }
        w.number(self.variables.slots.len() as u64);
        for ((bank, index), value) in &self.variables.slots {
            w.number(u64::from(*bank));
            w.number(u64::from(*index));
            w.number(u64::from(*value as u32));
        }
        w.number(self.variables.names.len() as u64);
        for (name, value) in &self.variables.names {
            w.bytes(name);
            match value {
                Value::Constant(v) => {
                    w.number(0);
                    w.number(u64::from(*v as u32));
                }
                Value::Slot(b, i) => {
                    w.number(u64::from(*b));
                    w.number(u64::from(*i));
                }
            }
        }
        if w.0.len() > crate::checkpoint::LIMIT {
            return Err("machine checkpoint too large".into());
        }
        Ok(w.0)
    }
    pub fn load_dialogue(
        bytes: &[u8],
        scripts: &mut impl Scripts,
        max_steps: usize,
        max_depth: usize,
    ) -> Result<Self, String> {
        let mut r = crate::checkpoint::Reader::new(bytes)?;
        let max_steps = r.count(max_steps)?;
        let max_depth = r.count(max_depth)?;
        let steps = r.count(max_steps)?;
        let count = r.count(max_depth)?;
        if count == 0 {
            return Err("empty saved call stack".into());
        }
        let mut stack = Vec::new();
        let mut source_bytes = 0usize;
        for _ in 0..count {
            let name = r.bytes()?;
            let source = scripts.load(name)?;
            source_bytes = source_bytes
                .checked_add(source.bytes.len())
                .ok_or("saved script size overflow")?;
            if source_bytes > crate::checkpoint::LIMIT {
                return Err("saved scripts exceed memory limit".into());
            }
            if source.name != name || crate::checkpoint::digest(&source.bytes) != r.number()? {
                return Err("saved script differs from mounted game".into());
            }
            let mut frame = Frame::new(source);
            frame.pc = r.count(frame.lines.len())?;
            let branches = r.count(frame.lines.len())?;
            let mut outer = frame.lines.len();
            for _ in 0..branches {
                let end = r.count(frame.lines.len().saturating_sub(1))?;
                if end < frame.pc || end >= outer || frame.lines[end].1 != b".end" {
                    return Err("invalid saved map boundary".into());
                }
                outer = end;
                frame.branch_ends.push(end);
            }
            stack.push(frame);
        }
        let mut variables = Variables::default();
        for _ in 0..r.count(65536)? {
            let bank = u8::try_from(r.number()?).map_err(|_| "invalid slot bank")?;
            let index = u32::try_from(r.number()?).map_err(|_| "invalid slot index")?;
            let value = u32::try_from(r.number()?).map_err(|_| "invalid slot value")? as i32;
            if !matches!(bank, b'G' | b'L' | b'F')
                || variables.slots.insert((bank, index), value).is_some()
            {
                return Err("invalid or duplicate saved slot".into());
            }
        }
        for _ in 0..r.count(65536)? {
            let name = r.bytes()?.to_vec();
            let bank = u8::try_from(r.number()?).map_err(|_| "invalid alias bank")?;
            let index = u32::try_from(r.number()?).map_err(|_| "invalid alias value")?;
            let value = if bank == 0 {
                Value::Constant(index as i32)
            } else {
                if !variables.slots.contains_key(&(bank, index)) {
                    return Err("saved alias has no slot".into());
                }
                Value::Slot(bank, index)
            };
            if name.is_empty() || name.contains(&0) || variables.names.insert(name, value).is_some()
            {
                return Err("invalid saved name".into());
            }
        }
        r.finish()?;
        let frame = stack.last().unwrap();
        let text = frame
            .lines
            .get(frame.pc)
            .ok_or("saved dialogue past script end")?
            .1
            .clone();
        if text.is_empty()
            || matches!(
                text[0],
                b'.' | b':' | b'@' | b'#' | b'*' | b'!' | b';' | b'?'
            )
        {
            return Err("saved location is not dialogue".into());
        }
        Ok(Self {
            variables,
            stack,
            pending: Some(Request::Text(text)),
            steps,
            max_steps,
            dialogue: true,
            max_depth,
        })
    }
    pub fn new(source: Source, max_steps: usize, max_depth: usize) -> Self {
        Self {
            variables: Variables::default(),
            stack: vec![Frame::new(source)],
            pending: None,
            steps: 0,
            max_steps,
            dialogue: false,
            max_depth,
        }
    }
    /// Enable the observed colon-speaker and UTF-8 text profile explicitly.
    pub fn enable_dialogue(&mut self) {
        self.dialogue = true;
    }
    pub fn location(&self) -> Option<(&[u8], usize)> {
        self.stack.last().map(|f| {
            (
                f.source.name.as_slice(),
                f.lines.get(f.pc).map_or(f.lines.len() + 1, |l| l.0),
            )
        })
    }
    /// Current text, or the next physical text after a pending speaker marker.
    /// Commands are never scanned through or executed by this inspection.
    pub fn dialogue_location(&self) -> Option<(&[u8], usize, &[u8])> {
        let frame = self.stack.last()?;
        let offset = usize::from(matches!(self.pending, Some(Request::Speaker(_))));
        let (line, raw) = frame
            .lines
            .iter()
            .skip(frame.pc + offset)
            .find(|(_, raw)| !raw.is_empty() && !raw.starts_with(b";"))?;
        if matches!(raw[0], b'.' | b':' | b'@' | b'#' | b'*' | b'!') {
            return None;
        }
        Some((&frame.source.name, *line, raw))
    }
    pub fn depth(&self) -> usize {
        self.stack.len()
    }
    pub fn steps(&self) -> usize {
        self.steps
    }
    pub fn advance(&mut self, scripts: &mut impl Scripts) -> Result<Step, String> {
        if self.stack.len() > self.max_depth {
            return Err("call depth exceeded".into());
        }
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
            charge(&mut self.steps, self.max_steps)?;
            let raw = frame.lines[frame.pc].1.clone();
            if raw.is_empty() || raw.starts_with(b";") {
                frame.pc += 1;
                continue;
            }
            let word = raw.split(|b| b.is_ascii_whitespace()).next().unwrap();
            if word == b".case" || raw == b".end" {
                let end = frame
                    .branch_ends
                    .pop()
                    .ok_or("case/end outside selected map branch")?;
                frame.pc = end + 1;
                continue;
            }
            if [b".def".as_slice(), b".var", b".call", b".load", b".system"].contains(&word)
                || raw.starts_with(b"?")
            {
                let ast = parse_bootstrap(&raw, 1).map_err(|e| e.to_string())?;
                match &ast[0].statement {
                    Statement::Define { name, value } => {
                        self.variables.define_constant(name, *value)?;
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
                        charge(&mut self.steps, self.max_steps)?;
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
            } else if self.dialogue {
                std::str::from_utf8(&raw).map_err(|_| "dialogue requires decoded UTF-8")?;
                if raw.iter().any(|b| *b < 32 && *b != b'\t') {
                    return Err("unsupported dialogue control".into());
                }
                let request = if raw.starts_with(b":") {
                    let fields: Vec<_> =
                        raw[1..].split(|b| *b == b':').map(|s| s.to_vec()).collect();
                    if fields.len() != 3 {
                        return Err("speaker requires name:voice:actor".into());
                    }
                    Request::Speaker(fields)
                } else {
                    if raw.iter().any(|b| *b < 32 && *b != b'\t')
                        || matches!(raw[0], b'@' | b'#' | b'*' | b'!')
                    {
                        return Err("unsupported dialogue control".into());
                    }
                    Request::Text(raw)
                };
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
            let mut nesting = 0usize;
            for (i, (_, line)) in frame.lines.iter().enumerate().skip(frame.pc + 1) {
                charge(&mut self.steps, self.max_steps)?;
                let word = line
                    .split(|b| b.is_ascii_whitespace())
                    .next()
                    .unwrap_or_default();
                if word == b".map" {
                    nesting += 1;
                    continue;
                }
                if line == b".end" {
                    if nesting != 0 {
                        nesting -= 1;
                        continue;
                    }
                    end = Some(i);
                    break;
                }
                if nesting == 0 && word == b".case" {
                    let value = &line[word.len()..];
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
            frame.branch_ends.push(end);
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

fn charge(steps: &mut usize, limit: usize) -> Result<(), String> {
    if *steps >= limit {
        return Err("instruction budget exceeded".into());
    }
    *steps += 1;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn saved_nested_calls_maps_variables_and_budget_resume_without_reexecution() {
        struct SavedScripts;
        impl Scripts for SavedScripts {
            fn load(&mut self, name: &[u8]) -> Result<Source, String> {
                let bytes = match name {
                    b"root" => b".var G0 x\n? x = 3\n.call child".as_slice(),
                    b"child" => b".map m\n.case 1\n.map n\n.case 2\none\n? x = 9\ntwo\n.end\n.end",
                    _ => return Err("missing".into()),
                };
                Ok(Source {
                    name: name.to_vec(),
                    bytes: bytes.to_vec(),
                })
            }
        }
        let mut store = SavedScripts;
        let mut original = Machine::new(store.load(b"root").unwrap(), 60, 4);
        original.enable_dialogue();
        assert!(matches!(
            original.advance(&mut store).unwrap(),
            Step::Request(Request::Command { .. })
        ));
        original.complete(Some(1)).unwrap();
        original.advance(&mut store).unwrap();
        original.complete(Some(2)).unwrap();
        assert_eq!(
            original.advance(&mut store).unwrap(),
            Step::Request(Request::Text(b"one".to_vec()))
        );
        let bytes = original.save_dialogue().unwrap();
        let mut restored = Machine::load_dialogue(&bytes, &mut store, 10000, 64).unwrap();
        assert_eq!(restored.steps(), original.steps());
        assert_eq!(restored.max_steps, 60);
        assert_eq!(restored.max_depth, 4);
        assert_eq!(restored.variables.get(b"x"), Ok(3));
        assert_eq!(restored.depth(), 2);
        assert_eq!(restored.save_dialogue().unwrap(), bytes);
        for vm in [&mut original, &mut restored] {
            vm.complete(None).unwrap();
            assert_eq!(
                vm.advance(&mut store).unwrap(),
                Step::Request(Request::Text(b"two".to_vec()))
            );
            assert_eq!(vm.variables.get(b"x"), Ok(9));
            vm.complete(None).unwrap();
            assert_eq!(vm.advance(&mut store).unwrap(), Step::Finished);
        }
        for n in 0..bytes.len() {
            assert!(Machine::load_dialogue(&bytes[..n], &mut store, 10000, 64).is_err());
        }
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert!(Machine::load_dialogue(&trailing, &mut store, 10000, 64).is_err());
        assert!(Machine::load_dialogue(&bytes, &mut store, 1, 64).is_err());
    }
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
    fn variable_declarations_preserve_aliases_and_reject_partial_updates() {
        let mut vars = Variables::default();
        vars.define_system(b"G1", b"x").unwrap();
        vars.set(b"x", 17).unwrap();
        vars.define_system(b"G1", b"alias").unwrap();
        assert!(vars.define_constant(b"x", 99).is_err());
        assert!(vars.define_system(b"G2", b"x").is_err());
        assert!(vars.define_system(b"G4294967296", b"overflow").is_err());
        assert!(vars.define_system(b"G1", b"nul\0name").is_err());
        assert_eq!(vars.get(b"alias").unwrap(), 17);
        assert_eq!(vars.slots().len(), 1);
        vars.define_constant(b"fixed", 9).unwrap();
        assert!(vars.set(b"fixed", 10).is_err());
        assert_eq!(vars.get(b"fixed").unwrap(), 9);
    }
    #[test]
    fn nested_maps_keep_outer_boundaries_and_call_context() {
        let mut vm = Machine::new(
            Source {
                name: b"nested".to_vec(),
                bytes: br#".var L1 x
.map outer
.case 1
.map inner
.case 2
.call patch
.case 1
? x = 99
.end
.observe
.case 2
? x = 88
.end
.done"#
                    .to_vec(),
            },
            200,
            4,
        );
        let mut store = Store;
        vm.advance(&mut store).unwrap();
        // Inner .case 1 must not count as a duplicate outer case.
        vm.complete(Some(1)).unwrap();
        vm.advance(&mut store).unwrap();
        let location = vm.location().unwrap().1;
        assert!(vm.complete(Some(9)).is_err());
        assert_eq!(vm.location().unwrap().1, location);
        vm.complete(Some(2)).unwrap();
        assert!(
            matches!(vm.advance(&mut store).unwrap(),Step::Request(Request::Command{name,..}) if name==b"observe")
        );
        assert_eq!(vm.variables.get(b"x").unwrap(), 7);
        assert_eq!(vm.depth(), 1);
        vm.complete(None).unwrap();
        assert!(
            matches!(vm.advance(&mut store).unwrap(),Step::Request(Request::Command{name,..}) if name==b"done")
        );
        assert_eq!(vm.variables.get(b"x").unwrap(), 7);
        vm.complete(None).unwrap();
        assert_eq!(vm.advance(&mut store).unwrap(), Step::Finished);
    }
    #[test]
    fn malformed_maps_and_scan_budget_do_not_acknowledge_requests() {
        for text in [
            ".map x\n.case 1\n.map inner\n.case 1\n.end",
            ".map x\n.case 1\n.case 1\n.end",
        ] {
            let mut vm = Machine::new(
                Source {
                    name: vec![],
                    bytes: text.as_bytes().to_vec(),
                },
                100,
                4,
            );
            let pending = vm.advance(&mut Store).unwrap();
            assert!(vm.complete(Some(1)).is_err());
            assert_eq!(vm.location().unwrap().1, 1);
            assert_eq!(vm.advance(&mut Store).unwrap(), pending);
        }
        let mut vm = Machine::new(
            Source {
                name: vec![],
                bytes: b".map x\n.case 1\n; a\n; b\n.end".to_vec(),
            },
            3,
            4,
        );
        let pending = vm.advance(&mut Store).unwrap();
        assert!(vm.complete(Some(1)).unwrap_err().contains("budget"));
        assert_eq!(vm.location().unwrap().1, 1);
        assert_eq!(vm.advance(&mut Store).unwrap(), pending);
        assert!(vm.complete(Some(1)).is_err());
        assert_eq!(vm.steps(), 3);
    }
    #[test]
    fn scenario_budget_counts_empty_lines_and_zero_depth_is_rejected() {
        let source = Source {
            name: vec![],
            bytes: b"; comment\n\n.done".to_vec(),
        };
        let mut vm = Machine::new(source.clone(), 2, 1);
        assert!(vm.advance(&mut Store).unwrap_err().contains("budget"));
        assert_eq!(vm.location().unwrap().1, 3);
        assert_eq!(vm.steps(), 2);
        assert!(vm.advance(&mut Store).is_err());
        let mut vm = Machine::new(source, 10, 0);
        assert!(vm.advance(&mut Store).unwrap_err().contains("depth"));
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

#[cfg(test)]
mod dialogue_tests {
    use super::*;
    struct NoCalls;
    impl Scripts for NoCalls {
        fn load(&mut self, _: &[u8]) -> Result<Source, String> {
            Err("unexpected call".into())
        }
    }
    #[test]
    fn dialogue_is_opt_in_and_waits_for_ack() {
        let source = Source {
            name: b"story".to_vec(),
            bytes: b":Alice:voice:actor\nHello\nWorld\n.unknown".to_vec(),
        };
        let mut vm = Machine::new(source.clone(), 100, 4);
        assert!(vm.advance(&mut NoCalls).is_err());
        let mut vm = Machine::new(source, 100, 4);
        vm.enable_dialogue();
        assert!(matches!(
            vm.advance(&mut NoCalls).unwrap(),
            Step::Request(Request::Speaker(_))
        ));
        vm.complete(None).unwrap();
        let first = vm.advance(&mut NoCalls).unwrap();
        let steps = vm.steps();
        let location = vm.location().unwrap().1;
        assert_eq!(first, Step::Request(Request::Text(b"Hello".to_vec())));
        assert_eq!(vm.advance(&mut NoCalls).unwrap(), first);
        assert_eq!(vm.steps(), steps);
        assert_eq!(vm.location().unwrap().1, location);
        vm.complete(None).unwrap();
        assert_eq!(
            vm.advance(&mut NoCalls).unwrap(),
            Step::Request(Request::Text(b"World".to_vec()))
        );
        vm.complete(None).unwrap();
        assert!(
            matches!(vm.advance(&mut NoCalls).unwrap(),Step::Request(Request::Command{name,..}) if name==b"unknown")
        );
    }
    #[test]
    fn malformed_speaker_and_text_budget_stop() {
        let mut vm = Machine::new(
            Source {
                name: vec![],
                bytes: b":incomplete\ntext".to_vec(),
            },
            10,
            4,
        );
        vm.enable_dialogue();
        assert!(vm.advance(&mut NoCalls).unwrap_err().contains("speaker"));
        let mut vm = Machine::new(
            Source {
                name: vec![],
                bytes: b"one\ntwo".to_vec(),
            },
            1,
            4,
        );
        vm.enable_dialogue();
        vm.advance(&mut NoCalls).unwrap();
        vm.complete(None).unwrap();
        assert!(vm.advance(&mut NoCalls).unwrap_err().contains("budget"));
    }
}
