//! Run the mounted title handler through the shared retained UI draw protocol.
use super::*;
use crate::settings::script_view::ScriptView;
use nekopack_script::system::ResourceProvider;

pub(super) struct NativeTitle {
    view: ScriptView,
    args: Vec<Vec<u8>>,
    rows: Vec<Vec<u8>>,
    last_frame: String,
    pub closed: bool,
}
impl NativeTitle {
    pub fn open<R: Read + Seek>(
        store: &mut DatGame<R>,
        vm: &mut SystemVm,
        variables: &mut nekopack_script::runtime::Variables,
        args: Vec<Vec<u8>>,
        rows: Vec<Vec<u8>>,
    ) -> Result<Self> {
        vm.eval(
            "title-primitives",
            include_str!("../../host/config_page.nut"),
        )?;
        vm.eval(
            "title-coroutine-host",
            include_str!("../../host/title_page.nut"),
        )?;
        let assets = store.resource("Assets", "title")?;
        vm.eval_with_resources(
            "title-coroutine-install",
            &format!(
                "TitlePageInstall({{title={assets}}},{},{})",
                array(&args)?,
                array(&rows)?
            ),
            variables,
            store,
        )?;
        let mut view = Self {
            view: ScriptView::default(),
            args,
            rows,
            last_frame: String::new(),
            closed: false,
        };
        view.step(store, vm, variables, "0", -1, -1, 0)?;
        Ok(view)
    }
    pub fn restart<R: Read + Seek>(
        &mut self,
        store: &mut DatGame<R>,
        vm: &mut SystemVm,
        variables: &mut nekopack_script::runtime::Variables,
    ) -> Result<()> {
        *self = Self::open(store, vm, variables, self.args.clone(), self.rows.clone())?;
        Ok(())
    }
    pub fn step<R: Read + Seek>(
        &mut self,
        store: &mut DatGame<R>,
        vm: &mut SystemVm,
        variables: &mut nekopack_script::runtime::Variables,
        event: &str,
        x: i32,
        y: i32,
        elapsed: u64,
    ) -> Result<(Option<i32>, Vec<crate::message_log::Cue>)> {
        vm.eval_with_resources(
            "title-coroutine-step",
            &format!("return TitlePageStep({event},{x},{y},{elapsed})"),
            variables,
            store,
        )?;
        let mut action = None;
        let plan = vm.result();
        let mut draws = String::new();
        for line in plan.lines() {
            if let Some(id) = line.strip_prefix("closed:") {
                action = Some(id.parse()?);
                self.closed = true;
            } else {
                draws.push_str(line);
                draws.push('\n');
            }
        }
        if self.closed {
            draws.push_str(&self.last_frame);
        } else {
            self.last_frame = plan
                .lines()
                .filter(|line| !line.starts_with("S\t"))
                .map(|line| format!("{line}\n"))
                .collect();
        }
        let cues = self.view.apply_plan(store, &draws)?;
        Ok((action, cues))
    }
    pub fn delay(&self) -> i32 {
        if self.closed {
            0
        } else {
            self.view.delay.max(0)
        }
    }
    pub fn buttons(&self) -> Result<Vec<(i32, bool, [i32; 4])>> {
        self.view
            .buttons
            .iter()
            .map(|(name, bounds)| {
                let (id, enabled) = name.split_once(':').ok_or("invalid title binding")?;
                Ok((id.parse()?, enabled == "1", *bounds))
            })
            .collect()
    }
    pub fn hit(&self, x: i32, y: i32) -> Result<Option<i32>> {
        Ok(self
            .buttons()?
            .into_iter()
            .find_map(|(id, enabled, [left, top, width, height])| {
                (enabled
                    && width >= 0
                    && height >= 0
                    && contains([left, top], [width as u32, height as u32], [x, y]))
                .then_some(id)
            }))
    }
    pub fn buttons_json(&self) -> Result<String> {
        Ok(self
            .buttons()?
            .into_iter()
            .map(|(id, enabled, bounds)| {
                format!("{{\"id\":{id},\"enabled\":{enabled},\"bounds\":{bounds:?}}}")
            })
            .collect::<Vec<_>>()
            .join(","))
    }
    pub fn render(&self, size: [u32; 2]) -> Result<RgbaImage> {
        self.view.render(size, None)
    }
}
