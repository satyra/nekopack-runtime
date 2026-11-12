use super::*;
use nekopack_script::system::ResourceProvider;

const FRAME_BUDGET: usize = 64 * 1024 * 1024;

enum Draw {
    Sprite(String, [i32; 2], u8),
    Fill(u32, u8),
    Text(String, [i32; 2], bool, bool),
}

#[derive(Default)]
pub(super) struct ScriptView {
    draws: Vec<Draw>,
    assets: BTreeMap<String, RgbaImage>,
    pub page: u32,
    pub index: i32,
    pub delay: i32,
    pub buttons: BTreeMap<String, [i32; 4]>,
    frame_mask: Option<([i32; 2], RgbaImage)>,
}

impl ScriptView {
    pub fn open<R: Read + Seek>(
        store: &mut DatGame<R>,
        vm: &mut SystemVm,
        pages: u32,
        scrolling: bool,
        page: u32,
    ) -> Result<Self> {
        let mut packs = vec!["config".to_owned()];
        if scrolling {
            packs.extend(["config_items".into(), "config_shortcut".into()]);
        } else {
            packs.extend((1..=pages).map(|page| format!("config{page}")));
        }
        if store.has_resource("system", "dialog.mng")? {
            packs.push("dialog".into());
        }
        let assets = packs.iter().map(|name| {
            Ok(format!("[{}]={}", quote(name.as_bytes())?, store.resource("Assets", name)?))
        }).collect::<Result<Vec<_>>>()?.join(",");
        vm.eval("config-page-host", include_str!("../../host/config_page.nut"))?;
        vm.eval("config-page-install", &format!("ConfigPageInstall({{{assets}}},{page})"))?;
        let mut view = Self::default();
        let bytes = store.read("system", "config.mng")?;
        let mng = Mng::parse(&bytes, 100000)?;
        for sprite in mng.sprites()? {
            if normalize(&crate::dat::decode(sprite.name)?) == "* FRAME" {
                view.frame_mask = Some((sprite.position, mng.decode_image(sprite.ordinal, FRAME_BUDGET)?));
            }
        }
        view.step(store, vm, "0", -1, -1, 0)?;
        Ok(view)
    }

    pub fn step<R: Read + Seek>(
        &mut self,
        store: &mut DatGame<R>,
        vm: &mut SystemVm,
        event: &str,
        x: i32,
        y: i32,
        elapsed: u64,
    ) -> Result<Vec<crate::message_log::Cue>> {
        let frame_hit = self.frame_mask.as_ref().is_some_and(|([left, top], mask)| {
            let dx = i64::from(x) - i64::from(*left);
            let dy = i64::from(y) - i64::from(*top);
            dx >= 0 && dy >= 0 && dx < i64::from(mask.width()) && dy < i64::from(mask.height())
                && mask.pixels()[(dy as usize * mask.width() as usize + dx as usize) * 4 + 3] != 0
        });
        vm.eval("config-page-step", &format!("return ConfigPageStep({event},{x},{y},{elapsed},{frame_hit})"))?;
        let plan = vm.result();
        let mut cues = Vec::new();
        let mut draws = Vec::new();
        self.buttons.clear();
        for line in plan.lines() {
            let fields = line.splitn(6, '\t').collect::<Vec<_>>();
            match fields.as_slice() {
                ["S", name, volume] => {
                    let id = match *name {
                        "PAGE" => 1, "CLICK" | "CANCEL" => 2, "OK" => 3, "SELECT" => 4,
                        _ => continue,
                    };
                    cues.push((id, volume.parse()?));
                }
                ["closed"] => self.page = 0,
                ["P", page, index, delay] => {
                    self.page = page.parse()?;
                    self.index = index.parse()?;
                    self.delay = delay.parse()?;
                }
                ["B", name, x, y, w, h] => {
                    self.buttons.insert((*name).to_owned(), [x.parse()?, y.parse()?, w.parse()?, h.parse()?]);
                }
                ["A", name, x, y, opacity] => draws.push(Draw::Sprite(
                    (*name).into(), [x.parse()?, y.parse()?], opacity.parse()?,
                )),
                ["F", color, opacity] => draws.push(Draw::Fill(color.parse()?, opacity.parse()?)),
                ["T", x, y, bold, border, text] => draws.push(Draw::Text(
                    // The original preview slices a byte string. A frame can end in an incomplete UTF-8 character.
                    text.trim_end_matches('\u{fffd}').into(),
                    [x.parse()?, y.parse()?], *bold == "1", *border == "1",
                )),
                _ => return Err(format!("invalid original settings draw command: {line}").into()),
            }
        }
        self.assets.retain(|key, _| draws.iter().any(|draw| {
            matches!(draw, Draw::Sprite(name, ..) if name == key)
        }));
        let mut retained = self.assets.values().map(|image| image.pixels().len()).sum::<usize>();
        let mut packs: BTreeMap<&str, Vec<(&str, usize)>> = BTreeMap::new();
        for draw in &draws {
            let Draw::Sprite(name, ..) = draw else { continue };
            if self.assets.contains_key(name) { continue; }
            let (path, ordinal) = name.rsplit_once(':').ok_or("invalid original settings asset")?;
            packs.entry(path).or_default().push((name, ordinal.parse()?));
        }
        for (path, sprites) in packs {
            let (dir, file) = path.rsplit_once('/').ok_or("invalid original settings pack")?;
            let bytes = store.read(dir, file)?;
            let mng = Mng::parse(&bytes, 100000)?;
            for (name, ordinal) in sprites {
                if self.assets.contains_key(name) { continue; }
                let pixels = mng.decode_image(ordinal, FRAME_BUDGET - retained)?;
                retained += pixels.pixels().len();
                self.assets.insert(name.into(), pixels);
            }
        }
        self.draws = draws;
        Ok(cues)
    }

    pub fn render(
        &self,
        size: [u32; 2],
        font: Option<&crate::text_render::SharedFont>,
    ) -> Result<RgbaImage> {
        let [width, height] = size;
        let mut canvas = RgbaImage::filled(width, height, [0, 0, 0, 255], FRAME_BUDGET)?;
        for draw in &self.draws {
            match draw {
                Draw::Sprite(name, position, 255) => canvas.draw(&self.assets[name], *position),
                Draw::Sprite(name, position, opacity) => {
                    let source = &self.assets[name];
                    let mut image = RgbaImage::from_rgba(source.width(), source.height(),
                        source.pixels().to_vec(), FRAME_BUDGET)?;
                    for pixel in image.pixels_mut().as_chunks_mut::<4>().0 {
                        pixel[3] = (u16::from(pixel[3]) * u16::from(*opacity) / 255) as u8;
                    }
                    canvas.draw(&image, *position);
                }
                Draw::Fill(color, opacity) => {
                    let fill = RgbaImage::filled(width, height,
                        [(color >> 16) as u8, (color >> 8) as u8, *color as u8, *opacity],
                        FRAME_BUDGET)?;
                    canvas.draw(&fill, [0, 0]);
                }
                Draw::Text(text, position, bold, border) if !text.is_empty() => {
                    crate::text_render::DialogueRenderer::draw_preview(
                        font.ok_or("settings preview font is not installed")?,
                        text, &mut canvas, *position, *bold, *border,
                    )?;
                }
                Draw::Text(..) => {}
            }
        }
        Ok(canvas)
    }
}
