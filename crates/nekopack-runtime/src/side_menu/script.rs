use super::*;

struct Button {
    action: String,
    enabled: bool,
    normal: Option<usize>,
    hover: Option<usize>,
    active: Option<usize>,
    disabled: Option<usize>,
    area: usize,
}
pub(crate) struct ScriptSideMenu {
    images: BTreeMap<usize, Sprite>,
    buttons: Vec<Button>,
    size: [u32; 2],
    bounds: [i32; 4],
    keep_icon: bool,
    pressed: Option<i32>,
    hover: Option<i32>,
    sequence: u64,
}
impl ScriptSideMenu {
    pub(super) fn open<R: Read + Seek>(
        store: &mut DatGame<R>,
        vm: &mut SystemVm,
        variables: &mut Variables,
        quick: bool,
        _popup: bool,
        keep_icon: bool,
        in_route: bool,
        voice: bool,
        skip: bool,
        auto_mode: bool,
        next: bool,
        back: bool,
    ) -> Result<Self> {
        vm.eval("side-menu-host", include_str!("../../host/side_menu.nut"))?;
        vm.eval_with_resources(
            "side-menu-plan",
            &format!(
                "return SideMenuPlan({quick},{in_route},{voice},{skip},{auto_mode},{next},{back})"
            ),
            variables,
            store,
        )?;
        let plan = vm.result();
        let mut buttons = Vec::new();
        let mut needed = std::collections::BTreeSet::new();
        for line in plan.lines() {
            let fields: Vec<_> = line.split('\t').collect();
            let ["B", action, enabled, normal, hover, active, disabled, area] = fields.as_slice()
            else {
                return Err("invalid script shortcut plan".into());
            };
            let ordinal = |value: &str| -> Result<Option<usize>> {
                let n = value.parse::<i32>()?;
                Ok(if n == -1 {
                    None
                } else {
                    Some(usize::try_from(n)?)
                })
            };
            let normal = ordinal(normal)?;
            let hover = ordinal(hover)?;
            let active = ordinal(active)?;
            let disabled = ordinal(disabled)?;
            let area = ordinal(area)?.ok_or("script shortcut has no hit area")?;
            needed.extend(
                normal
                    .into_iter()
                    .chain(hover)
                    .chain(active)
                    .chain(disabled)
                    .chain(Some(area)),
            );
            buttons.push(Button {
                action: (*action).into(),
                enabled: *enabled == "1",
                normal,
                hover,
                active,
                disabled,
                area,
            });
        }
        if buttons.is_empty() {
            return Err("script shortcut handler produced no buttons".into());
        }
        buttons.sort_by(|a, b| a.action.cmp(&b.action));
        let bytes = store.read("system", "shortcut.mng")?;
        let mng = Mng::parse(&bytes, 100000)?;
        let size = [mng.header[0], mng.header[1]];
        let sprites = mng.sprites()?;
        let mut retained = RgbaImage::transparent(size[0], size[1], BUDGET)?
            .pixels()
            .len();
        let mut images = BTreeMap::new();
        for ordinal in needed {
            let sprite = sprites
                .get(ordinal)
                .ok_or("invalid shortcut sprite ordinal")?;
            let image = mng.decode_image(ordinal, BUDGET - retained)?;
            retained += image.pixels().len();
            images.insert(
                ordinal,
                Sprite {
                    image,
                    position: sprite.position,
                },
            );
        }
        let left = buttons
            .iter()
            .map(|b| images[&b.area].position[0])
            .min()
            .unwrap();
        let top = buttons
            .iter()
            .map(|b| images[&b.area].position[1])
            .min()
            .unwrap();
        let right = buttons
            .iter()
            .map(|b| {
                i64::from(images[&b.area].position[0]) + i64::from(images[&b.area].image.width())
            })
            .max()
            .unwrap();
        let bottom = buttons
            .iter()
            .map(|b| {
                i64::from(images[&b.area].position[1]) + i64::from(images[&b.area].image.height())
            })
            .max()
            .unwrap();
        Ok(Self {
            images,
            buttons,
            size,
            bounds: [
                left,
                top,
                i32::try_from(right - i64::from(left))?,
                i32::try_from(bottom - i64::from(top))?,
            ],
            keep_icon,
            pressed: None,
            hover: None,
            sequence: 0,
        })
    }
    fn supported(action: &str) -> bool {
        matches!(
            action,
            "QSAVE"
                | "QLOAD"
                | "SAVE"
                | "LOAD"
                | "AUTO"
                | "SKIP"
                | "LOG"
                | "CONFIG"
                | "VOICE"
                | "RESET"
                | "HIDE"
                | "QUIT"
                | "NEXT"
                | "BACK"
        )
    }
    pub(super) fn pointer(&mut self, kind: i32, x: i32, y: i32) -> Result<Vec<u8>> {
        let hit = self
            .buttons
            .iter()
            .enumerate()
            .find(|(_, b)| {
                let area = &self.images[&b.area];
                b.enabled
                    && Self::supported(&b.action)
                    && contains(
                        area.position,
                        [area.image.width(), area.image.height()],
                        [x, y],
                    )
            })
            .map(|(i, _)| i as i32);
        self.hover = if kind == 3 { None } else { hit };
        let action = if kind == 4 {
            None
        } else {
            update_pointer(&mut self.pressed, kind, hit)?
        };
        self.sequence += 1;
        self.packet(
            action
                .map(|i| self.buttons[i as usize].action.as_str())
                .unwrap_or(""),
        )
    }
    pub(super) fn packet(&self, action: &str) -> Result<Vec<u8>> {
        let mut canvas = RgbaImage::transparent(self.size[0], self.size[1], BUDGET)?;
        let mut buttons = Vec::new();
        for (i, button) in self.buttons.iter().enumerate() {
            let ordinal = if !button.enabled {
                button.disabled.or(button.normal)
            } else if self.hover == Some(i as i32) {
                button.hover.or(button.normal)
            } else {
                button.active.or(button.normal)
            };
            if let Some(ordinal) = ordinal {
                let sprite = &self.images[&ordinal];
                canvas.draw(&sprite.image, sprite.position);
            }
            let area = &self.images[&button.area];
            let supported = Self::supported(&button.action);
            buttons.push(format!("{{\"action\":{},\"enabled\":{},\"supported\":{supported},\"bounds\":[{},{},{},{}]}}",
                quote(button.action.as_bytes())?,button.enabled&&supported,area.position[0],area.position[1],area.image.width(),area.image.height()));
        }
        let mut packet=format!("{{\"kind\":\"side_menu\",\"size\":[{},{}],\"bounds\":[{},{},{},{}],\"input_sequence\":{},\"action\":{},\"buttons\":[{}],\"keep_icon\":{},\"tooltip\":\"\"}}\n",
            self.size[0],self.size[1],self.bounds[0],self.bounds[1],self.bounds[2],self.bounds[3],self.sequence,quote(action.as_bytes())?,buttons.join(","),self.keep_icon).into_bytes();
        canvas.write_png(&mut packet)?;
        Ok(packet)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_shortcut_update_owns_bindings_and_enable_state() {
        let mut vm = SystemVm::new();
        vm.eval(
            "assets",
            r#"Assets <- {shortcut={
            ["=save"]=["system/shortcut.mng:0",1,2,3,4],
            ["+save"]=["system/shortcut.mng:1",1,2,3,4],
            ["=qload"]=["system/shortcut.mng:2",5,2,3,4],
            ["=skip"]=["system/shortcut.mng:3",9,2,3,4],
            ["@skip"]=["system/shortcut.mng:4",9,2,3,4],
            ["=next"]=["system/shortcut.mng:5",13,2,3,4],
            ["=back"]=["system/shortcut.mng:6",17,2,3,4]}}"#,
        )
        .unwrap();
        vm.eval(
            "host",
            include_str!("../../../nekopack-script/host/headless.nut"),
        )
        .unwrap();
        vm.eval(
            "synthetic-shortcut",
            r#"
            function QuickCheck(){return false}
            function InRoute(){return false}
            function HasLastVoice(){return false}
            function IsEnableSkipNext(){return false}
            function IsEnableSkipBack(){return true}
            SHORTCUTS <- {SAVE=1,QLOAD=2,SKIP=3,NEXT=4,BACK=5}
            ShortCut <- {m_spots=Spots(Images("shortcut")),Update=function(){
                m_spots.Enable(1,InRoute())
                m_spots.Enable(2,QuickCheck()&&InRoute())
                m_spots.Active(3,InSkip())
                m_spots.Enable(4,!InSkip() && !InAuto() && InRoute() && IsEnableSkipNext())
                m_spots.Enable(5,!InSkip() && !InAuto() && InRoute() && IsEnableSkipBack())
            }}
            foreach(name,id in SHORTCUTS)ShortCut.m_spots.BindR(name,id)
        "#,
        )
        .unwrap();
        vm.eval("side-host", include_str!("../../host/side_menu.nut"))
            .unwrap();
        vm.eval(
            "plan",
            "return SideMenuPlan(false,true,false,true,false,true,false)",
        )
        .unwrap();
        assert!(vm.result().contains("B\tSAVE\t1\t0\t1\t-1\t-1\t0"));
        assert!(vm.result().contains("B\tQLOAD\t0\t2"));
        assert!(vm.result().contains("B\tSKIP\t1\t3\t-1\t4"));
        vm.eval("restored","return (!QuickCheck() && !InRoute() && !InSkip() && !(\"NativeSideInputs\" in getroottable())).tostring()").unwrap();
        assert_eq!(vm.result(), "true");
        vm.eval(
            "plan",
            "return SideMenuPlan(false,true,false,false,false,true,false)",
        )
        .unwrap();
        assert!(vm.result().contains("B\tNEXT\t1\t5"));
        assert!(vm.result().contains("B\tBACK\t0\t6"));
        vm.eval(
            "plan",
            "return SideMenuPlan(false,true,false,false,false,false,true)",
        )
        .unwrap();
        assert!(vm.result().contains("B\tNEXT\t0\t5"));
        assert!(vm.result().contains("B\tBACK\t1\t6"));
        vm.eval(
            "navigation-restored",
            "return (!IsEnableSkipNext() && IsEnableSkipBack()).tostring()",
        )
        .unwrap();
        assert_eq!(vm.result(), "true");
    }

    #[test]
    fn next_and_back_are_supported_shortcut_actions() {
        for action in ["NEXT", "BACK"] {
            assert!(ScriptSideMenu::supported(action));
            let mut images = BTreeMap::new();
            images.insert(
                0,
                Sprite {
                    image: RgbaImage::transparent(10, 10, 400).unwrap(),
                    position: [0, 0],
                },
            );
            let mut menu = ScriptSideMenu {
                images,
                buttons: vec![Button {
                    action: action.into(),
                    enabled: true,
                    normal: Some(0),
                    hover: None,
                    active: None,
                    disabled: None,
                    area: 0,
                }],
                size: [10, 10],
                bounds: [0, 0, 10, 10],
                keep_icon: true,
                pressed: None,
                hover: None,
                sequence: 0,
            };
            menu.pointer(0, 5, 5).unwrap();
            let packet = menu.pointer(2, 5, 5).unwrap();
            let header =
                std::str::from_utf8(packet.split(|b| *b == b'\n').next().unwrap()).unwrap();
            assert!(header.contains(&format!("\"action\":\"{action}\"")));
            assert!(header.contains("\"enabled\":true,\"supported\":true"));
        }
    }

    #[test]
    fn scripted_hit_areas_preserve_disabled_and_unsupported_actions() {
        let mut images = BTreeMap::new();
        for (i, x) in [(0, 0), (1, 10), (2, 20)] {
            images.insert(
                i,
                Sprite {
                    image: RgbaImage::filled(10, 10, [200, 0, 0, 255], 400).unwrap(),
                    position: [x, 0],
                },
            );
        }
        let button = |action: &str, enabled, area| Button {
            action: action.into(),
            enabled,
            normal: Some(area),
            hover: None,
            active: None,
            disabled: None,
            area,
        };
        let mut menu = ScriptSideMenu {
            images,
            buttons: vec![
                button("SAVE", true, 0),
                button("QLOAD", false, 1),
                button("UNKNOWN", true, 2),
            ],
            size: [30, 10],
            bounds: [0, 0, 30, 10],
            keep_icon: true,
            pressed: None,
            hover: None,
            sequence: 0,
        };
        let header = |packet: Vec<u8>| {
            String::from_utf8(packet.split(|b| *b == b'\n').next().unwrap().to_vec()).unwrap()
        };
        menu.pointer(0, 15, 5).unwrap();
        assert!(header(menu.pointer(2, 15, 5).unwrap()).contains("\"action\":\"\""));
        menu.pointer(0, 25, 5).unwrap();
        let unsupported = header(menu.pointer(2, 25, 5).unwrap());
        assert!(
            unsupported.contains("\"action\":\"UNKNOWN\",\"enabled\":false,\"supported\":false")
        );
        menu.pointer(0, 5, 5).unwrap();
        menu.pointer(1, 40, 5).unwrap();
        assert!(header(menu.pointer(2, 5, 5).unwrap()).contains("\"action\":\"\""));
        menu.pointer(0, 5, 5).unwrap();
        assert!(header(menu.pointer(2, 5, 5).unwrap()).contains("\"action\":\"SAVE\""));
    }
}
