use super::*;
use nekopack_script::system::ResourceProvider;
use crate::system_command::mask::{self, MaskState, Mode as MaskMode};
use std::rc::Rc;

const FRAME_BUDGET: usize = 64 * 1024 * 1024;

enum Draw {
    Scene,
    CurrentScreen,
    SavedScreen(u32),
    Transition(i32, Vec<Draw>, Vec<Draw>),
    MaskTransition(i32, MaskMode, u32, Vec<Draw>, Vec<Draw>),
    OriginalFill(u32, i32),
    Sprite(String, [i32; 2], u8),
    Crop(String,[i32;2],[i32;2],u8),
    Fill(u32, u8),
    Text(String, [i32; 2], bool, bool),
    StyledText(String, [i32;2], i32, u32, bool, Option<u32>),
    Composite([u32;2], [i32;2], u8, Vec<Draw>),
    Pixels([u32;2], PixelOperation, [u32;2], Vec<Draw>, Vec<Draw>),
}

enum PixelOperation { Alpha, Reduction }
fn pixel_draw(width:&str,height:&str,op:&str,sw:&str,sh:&str,source:&str,mask:&str)->Result<Draw> {
    let operation=match op {"alpha"=>PixelOperation::Alpha,"reduction"=>PixelOperation::Reduction,_=>return Err("invalid sprite pixel operation".into())};
    Ok(Draw::Pixels([width.parse()?,height.parse()?],operation,[sw.parse()?,sh.parse()?],transition_draws(source)?,transition_draws(mask)?))
}

fn draw_assets(draws: &[Draw]) -> Vec<&str> {
    let mut names = Vec::new();
    for draw in draws {
        match draw {
            Draw::Sprite(name, ..) | Draw::Crop(name, ..) => names.push(name.as_str()),
            Draw::Transition(_, begin, end) | Draw::MaskTransition(_, _, _, begin, end) => {
                names.extend(draw_assets(begin));
                names.extend(draw_assets(end));
            }
            Draw::Composite(_,_,_,children) => names.extend(draw_assets(children)),
            Draw::Pixels(_,_,_,source,mask) => {names.extend(draw_assets(source));names.extend(draw_assets(mask));},
            _ => {}
        }
    }
    names
}

fn decoded_text(encoded:&str)->Result<String> {
    if encoded.len() % 2 != 0 {return Err("invalid transition snapshot encoding".into());}
    let bytes = encoded.as_bytes().chunks_exact(2)
        .map(|pair| -> Result<u8> {Ok(u8::from_str_radix(std::str::from_utf8(pair)?,16)?)})
        .collect::<Result<Vec<_>>>()?;
    Ok(String::from_utf8(bytes)?)
}

#[cfg(test)]
#[path="script_view_drawing_tests.rs"]
mod drawing_tests;

fn styled_text(x:&str,y:&str,size:&str,color:&str,bold:&str,border:&str,text:&str)->Result<Draw> {
    let size=size.parse()?;
    if size<=0 {return Err("original glyph size must be positive".into());}
    let bold=match bold {"0"=>false,"1"=>true,_=>return Err("invalid original glyph bold flag".into())};
    let border=match border.parse::<i64>()? {-1=>None,value=>Some(u32::try_from(value)?)};
    Ok(Draw::StyledText(decoded_text(text)?,[x.parse()?,y.parse()?],size,color.parse()?,bold,border))
}

fn transition_draws(encoded: &str) -> Result<Vec<Draw>> {
    decoded_text(encoded)?.lines().map(|line| {
        let fields: Vec<_> = line.split('\t').collect();
        Ok(match fields.as_slice() {
            ["C"] => Draw::Scene,
            ["Q"] => Draw::CurrentScreen,
            ["Z",id] => Draw::SavedScreen(id.parse()?),
            ["A", name, x, y, opacity] => Draw::Sprite((*name).into(), [x.parse()?, y.parse()?], opacity.parse()?),
            ["K",name,x,y,w,h,opacity] => Draw::Crop((*name).into(),[x.parse()?,y.parse()?],[w.parse()?,h.parse()?],opacity.parse()?),
            ["T",x,y,bold,border,text] => Draw::Text((*text).into(),[x.parse()?,y.parse()?],*bold=="1",*border=="1"),
            ["G",x,y,size,color,bold,border,text] => styled_text(x,y,size,color,bold,border,text)?,
            ["D",w,h,op,sw,sh,source,mask] => pixel_draw(w,h,op,sw,sh,source,mask)?,
            ["J",width,height,x,y,opacity,plan] => Draw::Composite([width.parse()?,height.parse()?],[x.parse()?,y.parse()?],opacity.parse()?,transition_draws(plan)?),
            ["F", color, opacity] => Draw::Fill(color.parse()?, opacity.parse()?),
            ["O", color, opacity] => Draw::OriginalFill(color.parse()?, opacity.parse()?),
            ["X", alpha, begin, end] => Draw::Transition(alpha.parse()?, transition_draws(begin)?, transition_draws(end)?),
            ["Y", alpha, mode, id, begin, end] => Draw::MaskTransition(alpha.parse()?, MaskMode::parse(mode)?, id.parse()?, transition_draws(begin)?, transition_draws(end)?),
            _ => return Err("invalid original transition snapshot".into()),
        })
    }).collect()
}

#[derive(Default)]
pub(crate) struct ScriptView {
    draws: Vec<Draw>,
    assets: BTreeMap<String, RgbaImage>,
    asset_opacity: BTreeMap<String, bool>,
    pub page: u32,
    pub index: i32,
    pub delay: i32,
    pub buttons: BTreeMap<String, [i32; 4]>,
    frame_mask: Option<([i32; 2], RgbaImage)>,
    transition_masks: BTreeMap<u32, Rc<RgbaImage>>,
    pub(crate) saved_screens: BTreeMap<u32,Rc<RgbaImage>>,
}

impl ScriptView {
    /// Only screen-sized decoded resources need an alpha scan. Resource pixels
    /// are immutable during this view; retain the result even after eviction.
    pub(crate) fn new_opaque_assets(&mut self, size:[u32;2]) -> Vec<(String,[u32;2])> {
        let mut confirmed=Vec::new();
        for (name,image) in &self.assets {
            if image.width()<size[0] || image.height()<size[1] {continue;}
            if let std::collections::btree_map::Entry::Vacant(entry)=self.asset_opacity.entry(name.clone()) {
                let opaque=image.pixels().chunks_exact(4).all(|pixel|pixel[3]==255);
                entry.insert(opaque);
                if opaque {confirmed.push((name.clone(),[image.width(),image.height()]));}
            }
        }
        confirmed
    }

    pub(crate) fn load_saved_screen<R:Read+Seek>(&mut self,store:&mut DatGame<R>,id:u32,plan:&str,size:[u32;2],font:Option<&crate::text_render::SharedFont>,scene:&RgbaImage,current:&RgbaImage)->Result<Rc<RgbaImage>> {
        let old=std::mem::take(&mut self.draws);
        self.apply_plan(store,plan)?;
        let image=Rc::new(self.render_with_scenes(size,font,Some(scene),Some(current))?);
        self.draws=old;
        let bytes=self.saved_screens.iter().filter(|(key,_)|**key!=id).map(|(_,s)|s.pixels().len()).sum::<usize>()+image.pixels().len();
        if bytes>FRAME_BUDGET{return Err("original screen snapshot budget exceeded".into());}
        self.saved_screens.insert(id,Rc::clone(&image));
        Ok(image)
    }

    pub(crate) fn load_transition_mask<R: Read + Seek>(
        &mut self, store: &mut DatGame<R>, state: &mut MaskState,
        id: u32, name: &str, size: [u32; 2],
    ) -> Result<()> {
        let snapshot = state.load(store, name, size)?;
        let mut surfaces=std::collections::HashSet::new();
        let retained=self.transition_masks.values().chain(std::iter::once(&snapshot))
            .filter(|image|surfaces.insert(Rc::as_ptr(image)))
            .map(|image|image.pixels().len()).sum::<usize>();
        if retained>FRAME_BUDGET {return Err("original transition mask snapshot budget exceeded".into());}
        self.transition_masks.insert(id, snapshot);
        Ok(())
    }
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
        let assets = packs
            .iter()
            .map(|name| {
                Ok(format!(
                    "[{}]={}",
                    quote(name.as_bytes())?,
                    store.resource("Assets", name)?
                ))
            })
            .collect::<Result<Vec<_>>>()?
            .join(",");
        vm.eval(
            "config-page-host",
            include_str!("../../host/config_page.nut"),
        )?;
        vm.eval(
            "config-page-install",
            &format!("ConfigPageInstall({{{assets}}},{page})"),
        )?;
        let mut view = Self::default();
        let bytes = store.read("system", "config.mng")?;
        let mng = Mng::parse(&bytes, 100000)?;
        for sprite in mng.sprites()? {
            if normalize(&crate::dat::decode(sprite.name)?) == "* FRAME" {
                view.frame_mask = Some((
                    sprite.position,
                    mng.decode_image(sprite.ordinal, FRAME_BUDGET)?,
                ));
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
            dx >= 0
                && dy >= 0
                && dx < i64::from(mask.width())
                && dy < i64::from(mask.height())
                && mask.pixels()[(dy as usize * mask.width() as usize + dx as usize) * 4 + 3] != 0
        });
        vm.eval(
            "config-page-step",
            &format!("return ConfigPageStep({event},{x},{y},{elapsed},{frame_hit})"),
        )?;
        let plan = vm.result();
        self.apply_plan(store, &plan)
    }

    /// Shared retained drawing protocol used by original script UI coroutines.
    pub(crate) fn apply_plan<R: Read + Seek>(
        &mut self,
        store: &mut DatGame<R>,
        plan: &str,
    ) -> Result<Vec<crate::message_log::Cue>> {
        let mut cues = Vec::new();
        let mut draws = Vec::new();
        self.buttons.clear();
        for line in plan.lines() {
            let fields = line.splitn(if line.starts_with("T\t"){6}else{usize::MAX}, '\t').collect::<Vec<_>>();
            match fields.as_slice() {
                ["C"] => draws.push(Draw::Scene),
                ["Q"] => draws.push(Draw::CurrentScreen),
                ["Z",id] => draws.push(Draw::SavedScreen(id.parse()?)),
                ["O", color, opacity] => draws.push(Draw::OriginalFill(color.parse()?, opacity.parse()?)),
                ["X", alpha, begin, end] => draws.push(Draw::Transition(
                    alpha.parse()?, transition_draws(begin)?, transition_draws(end)?,
                )),
                ["Y", alpha, mode, id, begin, end] => draws.push(Draw::MaskTransition(
                    alpha.parse()?, MaskMode::parse(mode)?, id.parse()?, transition_draws(begin)?, transition_draws(end)?,
                )),
                ["S", name, volume] => {
                    let id = match *name {
                        "PAGE" => 1,
                        "CLICK" | "CANCEL" => 2,
                        "OK" => 3,
                        "SELECT" => 4,
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
                    self.buttons.insert(
                        (*name).to_owned(),
                        [x.parse()?, y.parse()?, w.parse()?, h.parse()?],
                    );
                }
                ["A", name, x, y, opacity] => draws.push(Draw::Sprite(
                    (*name).into(),
                    [x.parse()?, y.parse()?],
                    opacity.parse()?,
                )),
                ["K",name,x,y,w,h,opacity] => draws.push(Draw::Crop((*name).into(),[x.parse()?,y.parse()?],[w.parse()?,h.parse()?],opacity.parse()?)),
                ["G",x,y,size,color,bold,border,text] => draws.push(styled_text(x,y,size,color,bold,border,text)?),
                ["D",w,h,op,sw,sh,source,mask] => draws.push(pixel_draw(w,h,op,sw,sh,source,mask)?),
                ["J",width,height,x,y,opacity,plan] => draws.push(Draw::Composite([width.parse()?,height.parse()?],[x.parse()?,y.parse()?],opacity.parse()?,transition_draws(plan)?)),
                ["F", color, opacity] => draws.push(Draw::Fill(color.parse()?, opacity.parse()?)),
                ["T", x, y, bold, border, text] => draws.push(Draw::Text(
                    // The original preview slices a byte string. A frame can end in an incomplete UTF-8 character.
                    text.trim_end_matches('\u{fffd}').into(),
                    [x.parse()?, y.parse()?],
                    *bold == "1",
                    *border == "1",
                )),
                _ => return Err(format!("invalid original settings draw command: {line}").into()),
            }
        }
        let needed = draw_assets(&draws);
        self.assets.retain(|key, _| needed.contains(&key.as_str()));
        let mut retained = self
            .assets
            .values()
            .map(|image| image.pixels().len())
            .sum::<usize>();
        let mut packs: BTreeMap<&str, Vec<(&str, usize)>> = BTreeMap::new();
        for name in needed {
            if self.assets.contains_key(name) {
                continue;
            }
            let (path, ordinal) = name
                .rsplit_once(':')
                .ok_or("invalid original settings asset")?;
            packs
                .entry(path)
                .or_default()
                .push((name, ordinal.parse()?));
        }
        for (path, sprites) in packs {
            let (dir, file) = path
                .rsplit_once('/')
                .ok_or("invalid original settings pack")?;
            let bytes = store.read(dir, file)?;
            let mng = Mng::parse(&bytes, 100000)?;
            for (name, ordinal) in sprites {
                if self.assets.contains_key(name) {
                    continue;
                }
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
        self.render_with_scene(size, font, None)
    }

    /// A system command's Base/Screen snapshot is supplied by its caller.
    pub(crate) fn render_with_scene(
        &self,
        size: [u32; 2],
        font: Option<&crate::text_render::SharedFont>,
        scene: Option<&RgbaImage>,
    ) -> Result<RgbaImage> {
        self.render_with_scenes(size,font,scene,None)
    }

    pub(crate) fn render_with_scenes(
        &self,size:[u32;2],font:Option<&crate::text_render::SharedFont>,
        scene:Option<&RgbaImage>,current:Option<&RgbaImage>,
    )->Result<RgbaImage> {
        self.render_draws(&self.draws,size,font,scene,current)
    }

    fn render_pixel_source(&self,draws:&[Draw],size:[u32;2],font:Option<&crate::text_render::SharedFont>,scene:Option<&RgbaImage>,current:Option<&RgbaImage>)->Result<RgbaImage> {
        // Native Alpha copies the fourth byte, including transparent RGB, so an
        // unmodified asset/snapshot must bypass compositing onto transparent black.
        let image=match draws {
            [Draw::Sprite(name,[0,0],255)]=>Some(&self.assets[name]),
            [Draw::SavedScreen(id)]=>Some(self.saved_screens.get(id).ok_or("original screen snapshot is not installed")?.as_ref()),
            _=>None,
        };
        if let Some(image)=image {
            let mut output=RgbaImage::filled(size[0],size[1],[0;4],FRAME_BUDGET)?;
            let w=size[0].min(image.width()) as usize;
            for y in 0..size[1].min(image.height()) as usize {
                output.pixels_mut()[y*size[0] as usize*4..][..w*4].copy_from_slice(&image.pixels()[y*image.width() as usize*4..][..w*4]);
            }
            return Ok(output);
        }
        self.render_draws_on(draws,size,font,scene,current,[0;4])
    }

    fn render_draws(
        &self,
        draws: &[Draw],
        size: [u32; 2],
        font: Option<&crate::text_render::SharedFont>,
        scene: Option<&RgbaImage>,
        current: Option<&RgbaImage>,
    ) -> Result<RgbaImage> {
        self.render_draws_on(draws,size,font,scene,current,[0,0,0,255])
    }

    fn render_draws_on(
        &self,draws:&[Draw],size:[u32;2],font:Option<&crate::text_render::SharedFont>,
        scene:Option<&RgbaImage>,current:Option<&RgbaImage>,background:[u8;4],
    )->Result<RgbaImage> {
        let [width, height] = size;
        let mut canvas = RgbaImage::filled(width, height, background, FRAME_BUDGET)?;
        for draw in draws {
            match draw {
                Draw::SavedScreen(id) => canvas.draw(self.saved_screens.get(id).ok_or("original screen snapshot is not installed")?,[0,0]),
                Draw::MaskTransition(alpha, mode, id, begin, end) => {
                    let mask = self.transition_masks.get(id).ok_or("original transition mask snapshot is not installed")?;
                    let a = self.render_draws(begin, size, font, scene,current)?;
                    let b = self.render_draws(end, size, font, scene,current)?;
                    canvas = mask::render(&a, &b, mask, *alpha, *mode)?;
                    for pixel in canvas.pixels_mut().chunks_exact_mut(4) {pixel[3] = 255;}
                }
                Draw::Transition(alpha, begin, end) => {
                    if *alpha <= 0 {
                        canvas = self.render_draws(begin, size, font, scene,current)?;
                    } else if *alpha >= 255 {
                        canvas = self.render_draws(end, size, font, scene,current)?;
                    } else {
                        let a = self.render_draws(begin, size, font, scene,current)?;
                        let b = self.render_draws(end, size, font, scene,current)?;
                        for ((output, a), b) in canvas.pixels_mut().iter_mut().zip(a.pixels()).zip(b.pixels()) {
                            *output = ((u16::from(*a) * (255 - *alpha as u16) + u16::from(*b) * *alpha as u16) >> 8) as u8;
                        }
                    }
                    // Native screen storage's fourth byte is not straight
                    // alpha. GDI presents RGB; Android receives opaque pixels.
                    for pixel in canvas.pixels_mut().chunks_exact_mut(4) {pixel[3] = 255;}
                }
                Draw::OriginalFill(color, alpha) => {
                    if *alpha > 0 {
                        let rgb = [(color >> 16) as u8, (color >> 8) as u8, *color as u8];
                        for pixel in canvas.pixels_mut().chunks_exact_mut(4) {
                            if *alpha > 255 {pixel[..3].copy_from_slice(&rgb);}
                            else {
                                let q = (*alpha + (*alpha >> 7)) as u16;
                                for channel in 0..3 {
                                    let old = u16::from(pixel[channel]);
                                    pixel[channel] = (old - ((old * q) >> 8) + ((u16::from(rgb[channel]) * q) >> 8)) as u8;
                                }
                            }
                            pixel[3] = 255;
                        }
                    }
                }
                Draw::Scene => {
                    let scene = scene.ok_or("system command scene snapshot is not installed")?;
                    if [scene.width(), scene.height()] != size {
                        return Err("system command scene snapshot dimensions differ".into());
                    }
                    canvas.draw(scene, [0, 0]);
                }
                Draw::CurrentScreen => {
                    let current=current.ok_or("system command current screen snapshot is not installed")?;
                    if [current.width(),current.height()]!=size {return Err("system command current screen dimensions differ".into());}
                    canvas.draw(current,[0,0]);
                }
                Draw::Crop(name,position,extent,opacity) => {
                    crate::system_command::blit::blit(&mut canvas,&self.assets[name],*position,*extent,i32::from(*opacity))?;
                }
                Draw::Sprite(name, position, 255) => canvas.draw(&self.assets[name], *position),
                Draw::Sprite(name, position, opacity) => {
                    let source = &self.assets[name];
                    let mut image = RgbaImage::from_rgba(
                        source.width(),
                        source.height(),
                        source.pixels().to_vec(),
                        FRAME_BUDGET,
                    )?;
                    for pixel in image.pixels_mut().as_chunks_mut::<4>().0 {
                        pixel[3] = (u16::from(pixel[3]) * u16::from(*opacity) / 255) as u8;
                    }
                    canvas.draw(&image, *position);
                }
                Draw::Fill(color, opacity) => {
                    let fill = RgbaImage::filled(
                        width,
                        height,
                        [
                            (color >> 16) as u8,
                            (color >> 8) as u8,
                            *color as u8,
                            *opacity,
                        ],
                        FRAME_BUDGET,
                    )?;
                    canvas.draw(&fill, [0, 0]);
                }
                Draw::Text(text, position, bold, border) if !text.is_empty() => {
                    crate::text_render::DialogueRenderer::draw_preview(
                        font.ok_or("settings preview font is not installed")?,
                        text,
                        &mut canvas,
                        *position,
                        *bold,
                        *border,
                    )?;
                }
                Draw::Text(..) => {}
                Draw::StyledText(text,position,size,color,bold,border) if !text.is_empty() => {
                    crate::text_render::DialogueRenderer::draw_styled(
                        font.ok_or("original glyph font is not installed")?,text,&mut canvas,
                        *position,*size,*color,*bold,*border,
                    )?;
                }
                Draw::StyledText(..) => {}
                Draw::Pixels(target,operation,source_size,source,mask) => {
                    let source_size=if *source_size==[0,0] {
                        let image=match source.as_slice() {
                            [Draw::SavedScreen(id)]=>self.saved_screens.get(id).ok_or("original screen snapshot is not installed")?.as_ref(),
                            [Draw::Scene]=>scene.ok_or("system command scene snapshot is not installed")?,
                            [Draw::CurrentScreen]=>current.ok_or("system command current screen snapshot is not installed")?,
                            _=>return Err("pixel source dimensions are not available".into()),
                        };
                        [image.width(),image.height()]
                    }else{*source_size};
                    let source=self.render_pixel_source(source,source_size,font,scene,current)?;
                    let image=match operation {
                        PixelOperation::Alpha => {
                            let mask=self.render_pixel_source(mask,*target,font,scene,current)?;
                            let mut image=source;
                            for (pixel,mask) in image.pixels_mut().chunks_exact_mut(4).zip(mask.pixels().chunks_exact(4)) {pixel[3]=mask[3];}
                            image
                        }
                        PixelOperation::Reduction => native_reduction(&source,*target)?,
                    };
                    // A pixel operation replaces the local bitmap, including alpha.
                    if size!=*target {return Err("sprite pixel canvas dimensions differ".into());}
                    canvas=image;
                }
                Draw::Composite(size,position,opacity,children) => {
                    let image=self.render_draws_on(children,*size,font,scene,current,[0;4])?;
                    crate::system_command::blit::blit(&mut canvas,&image,*position,
                        [i32::try_from(size[0])?,i32::try_from(size[1])?],i32::from(*opacity))?;
                }
            }
        }
        Ok(canvas)
    }
}

fn native_reduction(source:&RgbaImage,target:[u32;2])->Result<RgbaImage> {
    let [w,h]=target;
    let mut output=RgbaImage::filled(w,h,[0,0,0,255],FRAME_BUDGET)?;
    let sw=i64::from(source.width());let sh=i64::from(source.height());
    let qx=i64::from(w)*256/sw;let qy=i64::from(h)*256/sh;
    let rx=sw*256/i64::from(w);let ry=sh*256/i64::from(h);
    for y in 0..i64::from(h) {for x in 0..i64::from(w) {
        let left=(x*rx>>8).max(0);let right=(((x+1)*rx+128>>8)-1).min(sw-1);
        let top=(y*ry>>8).max(0);let bottom=(((y+1)*ry+128>>8)-1).min(sh-1);
        let mut rgb=[0i64;3];let mut total=0;
        for sy in top..=bottom {for sx in left..=right {
            let mut wx=qx;let mut wy=qy;
            if sx==left {wx=(sx+1)*qx-(sx*qx & !255);}
            if sx==right {wx=(x+1)*256-sx*qx;}
            if sy==top {wy=(sy+1)*qy-(sy*qy & !255);}
            if sy==bottom {wy=(y+1)*256-sy*qy;}
            let weight=wx*wy;let pixel=&source.pixels()[((sy*sw+sx)*4) as usize..][..4];
            for channel in 0..3 {rgb[channel]+=i64::from(pixel[channel])*weight;}
            total+=weight;
        }}
        if total==0 {return Err("native Reduction has an empty sample footprint".into());}
        let pixel=&mut output.pixels_mut()[((y*i64::from(w)+x)*4) as usize..][..4];
        for channel in 0..3 {pixel[channel]=(rgb[channel]/total) as u8;}
    }}
    Ok(output)
}

#[cfg(test)]
mod system_transition_tests {
    use super::*;

    #[test]
    fn colored_mask_snapshot_nested_in_plain_transition_is_retained() {
        let mut view=ScriptView::default();
        view.transition_masks.insert(1,Rc::new(RgbaImage::from_rgba(1,1,vec![0,255,127,255],4).unwrap()));
        // The parsed mask transition may itself be a captured begin/end frame.
        // Build the protocol from synthetic draws; no original image resources.
        let hex=|s:&str|s.bytes().map(|b|format!("{b:02x}")).collect::<String>();
        let inner=format!("Y\t128\t2\t1\t{}\t{}\n",hex("O\t6579300\t256\n"),hex("O\t13158600\t256\n"));
        view.draws=vec![Draw::Transition(0,transition_draws(&hex(&inner)).unwrap(),Vec::new())];
        assert_eq!(view.render([1,1],None).unwrap().pixels(), &[100,200,200,255]);
        view.transition_masks.clear();
        assert!(view.render([1,1],None).is_err());
    }

    #[test]
    fn scene_redraw_and_current_screen_capture_are_distinct() {
        let scene=RgbaImage::filled(1,1,[10,20,30,255],4).unwrap();
        let current=RgbaImage::filled(1,1,[90,80,70,255],4).unwrap();
        for (draw,expected) in [(Draw::Scene,[10,20,30,255]),(Draw::CurrentScreen,[90,80,70,255])] {
            let view=ScriptView {draws:vec![draw],..Default::default()};
            assert_eq!(view.render_with_scenes([1,1],None,Some(&scene),Some(&current)).unwrap().pixels(),expected);
        }
    }

    #[test]
    fn native_transition_endpoints_and_integer_blend_keep_display_opaque() {
        for (alpha, expected) in [(-1, 100), (0, 100), (128, 149), (255, 200), (256, 200)] {
            let view = ScriptView {
                draws: vec![Draw::Transition(alpha,
                    vec![Draw::OriginalFill(0x646464, 256)],
                    vec![Draw::OriginalFill(0xc8c8c8, 256)])],
                ..Default::default()
            };
            assert_eq!(view.render([1,1],None).unwrap().pixels(), &[expected,expected,expected,255]);
        }
    }

    #[test]
    fn native_fill_uses_its_own_quantization_and_nonpositive_noop() {
        for (alpha, expected) in [(-50, 100), (0, 100), (128, 150), (255, 200), (256, 200)] {
            let view = ScriptView {
                draws: vec![Draw::OriginalFill(0x646464,256), Draw::OriginalFill(0xc8c8c8,alpha)],
                ..Default::default()
            };
            assert_eq!(view.render([1,1],None).unwrap().pixels(), &[expected,expected,expected,255]);
        }
    }
}
