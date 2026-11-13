//! Execute mounted system handlers without replacing their timing or branches.
//! Story retains the scenario request until this coroutine closes; the caller
//! applies returned state actions and presents frames in memory.
use crate::{array, dat::DatGame, quote, settings::script_view::ScriptView};
use nekopack_image::RgbaImage;
use nekopack_script::{runtime::Variables, system::SystemVm};
use std::{error::Error, io::{Read, Seek}};
use std::rc::Rc;

pub(crate) mod mask;
pub(crate) mod blit;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

#[derive(Clone, Copy, Debug, Default)]
pub(crate) enum SystemEvent {
    #[default]
    None,
    Confirm,
    Cancel,
    Drag,
    ConfigSaved,
    NativeAck,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct SystemInput {
    pub elapsed_ms: u32,
    pub event: SystemEvent,
    pub skip: bool,
    pub pointer: Option<[i32;2]>,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ScriptReturn {
    Null,
    Integer(i64),
    Bool(bool),
    String(Vec<u8>),
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum StateAction {
    ClearScene,
    ToggleAuto,
    CancelAuto,
    CancelSkip,
    EnableDelay(i32),
    SaveConfig,
    RestoreAudio,
    RestoreScreen,
    Quit,
    LoadCatalog,
    LoadState(String),
    LoadCommit(String),
    Room(crate::story::native_room::Action),
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct OriginalSound {
    pub name: String,
    pub volume: i32,
}

pub(crate) struct SystemFrame {
    pub closed: bool,
    pub native_pending: bool,
    /// -1 waits for input; 0 schedules another frame; positive values are ms.
    pub delay_ms: i32,
    pub return_value: Option<ScriptReturn>,
    pub state_actions: Vec<StateAction>,
    pub sounds: Vec<OriginalSound>,
    pub frame: Option<Rc<RgbaImage>>,
    pub buttons: std::collections::BTreeMap<String,[i32;4]>,
    pub size: [u32;2],
}

pub(crate) struct SystemCommand {
    view: ScriptView,
    scene: RgbaImage,
    current: RgbaImage,
    closed: bool,
    awaiting_config_save: bool,
    awaiting_native: bool,
    mask: mask::MaskState,
    font: Option<crate::text_render::SharedFont>,
    snapshot: std::rc::Rc<RgbaImage>,
    next_surface: u32,
}

impl SystemCommand {
    pub(crate) fn open<R: Read + Seek>(
        store: &mut DatGame<R>,
        vm: &mut SystemVm,
        variables: &mut Variables,
        name: &str,
        args: &[Vec<u8>],
        rows: Option<&[Vec<u8>]>,
        scene: RgbaImage,
        input: SystemInput,
    ) -> Result<(Self, SystemFrame)> {
        let current=RgbaImage::from_rgba(scene.width(),scene.height(),scene.pixels().to_vec(),64*1024*1024)?;
        Self::open_with_screen(store,vm,variables,name,args,rows,scene,current,input)
    }

    pub(crate) fn open_with_screen<R:Read+Seek>(
        store:&mut DatGame<R>,vm:&mut SystemVm,variables:&mut Variables,
        name:&str,args:&[Vec<u8>],rows:Option<&[Vec<u8>]>,scene:RgbaImage,current:RgbaImage,input:SystemInput,
    )->Result<(Self,SystemFrame)> {
        Self::open_with_screen_and_mask(store,vm,variables,name,args,rows,scene,current,input,mask::MaskState::default(),None,None)
    }

    pub(crate) fn open_with_screen_and_mask<R:Read+Seek>(
        store:&mut DatGame<R>,vm:&mut SystemVm,variables:&mut Variables,
        name:&str,args:&[Vec<u8>],rows:Option<&[Vec<u8>]>,scene:RgbaImage,current:RgbaImage,input:SystemInput,
        mask:mask::MaskState,font:Option<crate::text_render::SharedFont>,snapshot:Option<std::rc::Rc<RgbaImage>>,
    )->Result<(Self,SystemFrame)> {
        if [scene.width(),scene.height()]!=[current.width(),current.height()] {
            return Err("system command scene/current dimensions differ".into());
        }
        crate::audio_config::install_snapshotter(vm)?;
        vm.eval("system-command-host", include_str!("../host/system_command.nut"))?;
        vm.eval("system-gui-host", include_str!("../host/system_gui.nut"))?;
        vm.eval("system-exit-host", include_str!("../host/system_exit.nut"))?;
        vm.eval("system-load-host", include_str!("../host/system_load.nut"))?;
        vm.eval("system-room-host", include_str!("../host/system_room.nut"))?;
        vm.eval(
            "system-command-install",
            &format!("SystemCommandHost.canvas=[{},{}];SystemCommandInstall({},{},{})",scene.width(),scene.height(),quote(name.as_bytes())?, array(args)?,
                     rows.map(array).transpose()?.unwrap_or_else(|| "null".into())),
        )?;
        let snapshot=match snapshot {Some(image)=>image,None=>std::rc::Rc::new(RgbaImage::filled(scene.width(),scene.height(),[0;4],64*1024*1024)?) };
        let mut view=ScriptView::default();view.saved_screens.insert(0,std::rc::Rc::clone(&snapshot));
        let mut command = Self {view, scene,current,closed: false,awaiting_config_save:false,awaiting_native:false,mask,font,snapshot,next_surface:1_000_000};
        let frame = command.step(store, vm, variables, input)?;
        Ok((command, frame))
    }

    pub(crate) fn step<R: Read + Seek>(
        &mut self,
        store: &mut DatGame<R>,
        vm: &mut SystemVm,
        variables: &mut Variables,
        input: SystemInput,
    ) -> Result<SystemFrame> {
        if self.closed {return Err("system command already closed".into());}
        if input.elapsed_ms > i32::MAX as u32 {return Err("system command elapsed time exceeds VM range".into());}
        let event = match input.event {
            SystemEvent::None => 0,
            SystemEvent::Confirm => 2_000_000_001,
            SystemEvent::Cancel => 2_000_000_002,
            SystemEvent::Drag => 2_000_000_003,
            SystemEvent::ConfigSaved => 2_000_000_004,
            SystemEvent::NativeAck => 2_000_000_006,
        };
        let pointer=input.pointer.map(|p|format!("[{},{}]",p[0],p[1])).unwrap_or_else(||"null".into());
        vm.eval_with_resources(
            "system-command-step",
            &format!("return SystemCommandStep({event},{},{},{pointer})", input.skip, input.elapsed_ms),
            variables, store,
        )?;
        let frame=self.consume(store, &vm.result())?;
        let opaque=self.view.new_opaque_assets([self.scene.width(),self.scene.height()]);
        let assignments=opaque.iter().map(|(name,[width,height])|
                Ok(format!("SystemCommandHost.opaque_assets.rawset({},[{},{}]);",quote(name.as_bytes())?,width,height)))
                .collect::<Result<Vec<_>>>()?.join("");
        let expression=if frame.frame.is_some() {format!("SystemCommandRasterized({})",self.next_surface)}
                       else {"\"0\\t\"+SystemCommandLiveSurfaces()".into()};
        vm.eval("system-command-rendered-surface",&format!("{assignments}return {expression}"))?;
        let result=vm.result();
        let (stored,live)=result.split_once('\t').ok_or("invalid rendered surface acknowledgement")?;
        let live=if live.is_empty(){Vec::new()}else{live.split(',').map(str::parse::<u32>).collect::<std::result::Result<Vec<_>,_>>()?};
        self.view.saved_screens.retain(|id,_|*id<1_000_000 || live.contains(id));
        match stored {
            "1" => {
                self.view.saved_screens.insert(self.next_surface,Rc::clone(frame.frame.as_ref().ok_or("surface acknowledgement without frame")?));
                self.next_surface=self.next_surface.checked_add(1).ok_or("rendered surface id exhausted")?;
            },
            "0" => {},
            _ => return Err("invalid rendered surface acknowledgement".into()),
        }
        if self.view.saved_screens.values().map(|image|image.pixels().len()).sum::<usize>()>64*1024*1024 {
            return Err("original screen snapshot budget exceeded".into());
        }
        Ok(frame)
    }

    pub(crate) fn awaiting_native(&self)->bool {self.awaiting_native}
    pub(crate) fn set_font(&mut self,font:crate::text_render::SharedFont){self.font=Some(font);}
    pub(crate) fn set_scene(&mut self,image:RgbaImage){self.scene=image;}
    pub(crate) fn install_save_thumbnails(&mut self,images:Vec<(u32,RgbaImage)>){
        self.view.saved_screens.retain(|id,_|*id<1000 || *id>=1_000_000);
        for (id,image) in images {self.view.saved_screens.insert(id,std::rc::Rc::new(image));}
    }

    pub(crate) fn awaiting_config_save(&self)->bool {self.awaiting_config_save}

    pub(crate) fn into_scene_state(self)->(mask::MaskState,std::rc::Rc<RgbaImage>){(self.mask,self.snapshot)}

    pub(crate) fn into_mask_state(self) -> mask::MaskState { self.mask }

    fn consume<R: Read + Seek>(&mut self, store: &mut DatGame<R>, plan: &str) -> Result<SystemFrame> {
        let parsed = ParsedFrame::parse(plan)?;
        for (id,name) in &parsed.masks {
            self.view.load_transition_mask(store,&mut self.mask,*id,name,[self.scene.width(),self.scene.height()])?;
        }
        for (id,plan) in &parsed.snapshots {
            self.snapshot=self.view.load_saved_screen(store,*id,plan,[self.scene.width(),self.scene.height()],self.font.as_ref(),&self.scene,&self.current)?;
        }
        if parsed.has_frame {self.view.apply_plan(store, &parsed.draws)?;}
        let frame = if parsed.has_frame {
            Some(Rc::new(self.view.render_with_scenes([self.scene.width(), self.scene.height()], self.font.as_ref(), Some(&self.scene),Some(&self.current))?))
        } else {None};
        self.awaiting_native=parsed.native_pending;
        self.awaiting_config_save=parsed.state_actions.contains(&StateAction::SaveConfig);
        self.closed = parsed.return_value.is_some();
        Ok(SystemFrame {
            closed: self.closed, native_pending: parsed.native_pending, delay_ms: parsed.delay_ms, return_value: parsed.return_value,
            state_actions: parsed.state_actions, sounds: parsed.sounds, frame,
            buttons:self.view.buttons.clone(),size:[self.scene.width(),self.scene.height()],
        })
    }
}

struct ParsedFrame {
    delay_ms: i32,
    return_value: Option<ScriptReturn>,
    state_actions: Vec<StateAction>,
    sounds: Vec<OriginalSound>,
    has_frame: bool,
    draws: String,
    native_pending: bool,
    masks: Vec<(u32,String)>,
    snapshots: Vec<(u32,String)>,
}

impl ParsedFrame {
    fn parse(plan: &str) -> Result<Self> {
        let mut frame = Self {delay_ms: 0, return_value: None, state_actions: Vec::new(),
                              sounds: Vec::new(), has_frame: false, draws: String::new(), native_pending:false, masks:Vec::new(),snapshots:Vec::new()};
        for line in plan.lines() {
            let fields: Vec<_> = line.split('\t').collect();
            match fields.as_slice() {
                ["H", "native"] => frame.native_pending=true,
                ["U", "load_catalog"] => frame.state_actions.push(StateAction::LoadCatalog),
                ["U", "load_state", id] => frame.state_actions.push(StateAction::LoadState(String::from_utf8(unhex(id)?)?)),
                ["U", "load_commit", id] => frame.state_actions.push(StateAction::LoadCommit(String::from_utf8(unhex(id)?)?)),
                ["U", fields @ ..] if fields.first().is_some_and(|name|name.starts_with("room_")) => {
                    frame.state_actions.push(StateAction::Room(crate::story::native_room::parse(fields)?.ok_or("unknown room action")?));
                }
                ["U", "quit"] => frame.state_actions.push(StateAction::Quit),
                ["U", "clear_scene"] => frame.state_actions.push(StateAction::ClearScene),
                ["U", "save_config"] => frame.state_actions.push(StateAction::SaveConfig),
                ["U", "restore_audio"] => frame.state_actions.push(StateAction::RestoreAudio),
                ["U", "restore_screen"] => frame.state_actions.push(StateAction::RestoreScreen),
                ["U", "toggle_auto"] => frame.state_actions.push(StateAction::ToggleAuto),
                ["U", "cancel_auto"] => frame.state_actions.push(StateAction::CancelAuto),
                ["U", "cancel_skip"] => frame.state_actions.push(StateAction::CancelSkip),
                ["U", "enable_delay", value] => frame.state_actions.push(StateAction::EnableDelay(value.parse()?)),
                ["S", name, volume] => frame.sounds.push(OriginalSound {
                    name: String::from_utf8(unhex(name)?)?,
                    volume: crate::audio_config::snapshot_category(&String::from_utf8(unhex(volume)?)?, "VolSYS")? as i32,
                }),
                ["N",id,plan] => frame.snapshots.push((id.parse()?,String::from_utf8(unhex(plan)?)?)),
                ["M", id, name] => frame.masks.push((id.parse()?,String::from_utf8(unhex(name)?)?)),
                ["W", delay] => frame.delay_ms = delay.parse()?,
                ["R", "null"] => frame.return_value = Some(ScriptReturn::Null),
                ["R", "integer", value] => frame.return_value = Some(ScriptReturn::Integer(value.parse()?)),
                ["R", "bool", value] => frame.return_value = Some(ScriptReturn::Bool(value.parse()?)),
                ["R", "string", value] => frame.return_value = Some(ScriptReturn::String(unhex(value)?)),
                ["P", "1", "0", delay] => {
                    frame.has_frame = true;
                    frame.delay_ms = delay.parse()?;
                    frame.draws.push_str(line);frame.draws.push('\n');
                }
                ["C"] | ["Q"] | ["A", ..] | ["O", ..] | ["X", ..] | ["Y", ..] | ["T", ..] | ["B", ..] | ["K", ..] | ["Z", ..] | ["G", ..] | ["J", ..] | ["D", ..] => {frame.draws.push_str(line);frame.draws.push('\n');}
                _ => return Err("invalid system command frame protocol".into()),
            }
        }
        Ok(frame)
    }
}

fn unhex(value: &str) -> Result<Vec<u8>> {
    if value.len() % 2 != 0 {return Err("invalid system command hex value".into());}
    (0..value.len()).step_by(2).map(|i|u8::from_str_radix(&value[i..i+2],16).map_err(Into::into)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn drawing_fixture(base:[u8;4]) -> Vec<u8> {
        fn chunk(out:&mut Vec<u8>,kind:&[u8;4],body:&[u8]) {
            out.extend((body.len() as u32).to_be_bytes());out.extend(kind);out.extend(body);
            let mut crc=!0u32;
            for byte in kind.iter().chain(body) {
                crc^=u32::from(*byte);
                for _ in 0..8 {crc=(crc>>1)^if crc&1!=0 {0xedb88320}else{0};}
            }
            out.extend((!crc).to_be_bytes());
        }
        let mut result=b"\x8aMNG\r\n\x1a\n".to_vec();
        chunk(&mut result,b"MHDR",&[2u32,2,0,0,0,0,0].into_iter().flat_map(u32::to_be_bytes).collect::<Vec<_>>());
        for (name,pixels) in [("BASE",base.repeat(4)),("OVER",vec![0,0,255,0,0,0,255,128,0,0,255,128,0,0,255,128])] {
            let mut frame=vec![2];frame.extend(name.as_bytes());chunk(&mut result,b"FRAM",&frame);
            let mut png=Vec::new();RgbaImage::from_rgba(2,2,pixels,64).unwrap().write_png(&mut png).unwrap();
            result.extend(&png[8..]);
        }
        chunk(&mut result,b"MEND",&[]);result
    }

    #[test]
    fn opaque_screen_cover_bounds_1200_frames_and_keeps_transparent_pixels() {
        let mut store=DatGame::mount([resource_pack(b"system",b"opaque.mng",&drawing_fixture([255,0,0,255]))]).unwrap();
        let mut vm=host(r#"
            com["repeat"] = function(args){
                local image=Images("opaque"),screen=Screen()
                Before <- Sprite();Before.Screen()
                for(local i=0;i<1201;++i){
                    screen.Draw(image["BASE"]);screen.Draw(image["OVER"])
                    screen.Update();wait(50)
                }
                screen.Draw(Before);screen.Update()
            }
        "#,"repeat");
        let mut variables=Variables::default();
        let scene=RgbaImage::filled(2,2,[0,255,0,255],64).unwrap();
        let (mut command,first)=SystemCommand::open(&mut store,&mut vm,&mut variables,"repeat",&[],None,scene,SystemInput::default()).unwrap();
        let reference=first.frame.unwrap().pixels().to_vec();
        assert_eq!(&reference[..4],&[255,0,0,255]);
        let mut plan_length=None;
        for _ in 0..1200 {
            let frame=command.step(&mut store,&mut vm,&mut variables,SystemInput {elapsed_ms:50,..Default::default()}).unwrap();
            assert_eq!(frame.frame.unwrap().pixels(),reference);
            vm.eval("bounded plan", "return SystemCommandHost.current.len()+\":\"+SystemCommandLines(SystemCommandHost.current).len()+\":\"+Before.layers.len()").unwrap();
            let state=vm.result();
            assert!(state.starts_with("1:"),"{state}");
            assert!(state.ends_with(":1"),"captured screen was mutated: {state}");
            if let Some(expected)=&plan_length {assert_eq!(&state,expected);} else {plan_length=Some(state);}
        }
        let final_frame=command.step(&mut store,&mut vm,&mut variables,SystemInput::default()).unwrap();
        assert!(final_frame.closed);
        assert_eq!(final_frame.frame.unwrap().pixels(),[0,255,0,255].repeat(4));
        vm.eval("opaque cache", "return SystemCommandHost.opaque_assets.len().tostring()").unwrap();
        assert_eq!(vm.result(),"1");
    }

    #[test]
    fn translucent_screen_accumulates_exactly_for_1201_frames_with_bounded_leases() {
        let mut store=DatGame::mount([resource_pack(b"system",b"partial.mng",&drawing_fixture([200,30,60,128]))]).unwrap();
        let mut vm=host(r#"
            com["repeat"] = function(args){
                local image=Images("partial"),screen=Screen()
                for(local i=0;i<1201;++i){
                    if(i==2){Captured <- Sprite();Captured.Screen();screen.Snap()}
                    screen.Draw(image["BASE"]);screen.Update();wait(50)
                }
                screen.Draw(Captured);screen.Update();wait(0)
                local saved=Sprite();saved.Snap();screen.Draw(saved);screen.Update()
            }
        "#,"repeat");
        let mut vars=Variables::default();
        let mut expected=RgbaImage::filled(2,2,[10,220,100,255],64).unwrap();
        let source=RgbaImage::filled(2,2,[200,30,60,128],64).unwrap();
        let (mut command,first)=SystemCommand::open(&mut store,&mut vm,&mut vars,"repeat",&[],None,
            RgbaImage::filled(2,2,[10,220,100,255],64).unwrap(),SystemInput::default()).unwrap();
        expected.draw(&source,[0,0]);assert_eq!(first.frame.unwrap().pixels(),expected.pixels());
        let mut captured=Vec::new();
        for i in 1..1201 {
            if i==500 {command.install_save_thumbnails(vec![(1001,RgbaImage::filled(1,1,[1,2,3,255],64).unwrap())]);}
            let frame=command.step(&mut store,&mut vm,&mut vars,SystemInput {elapsed_ms:50,..Default::default()}).unwrap();
            expected.draw(&source,[0,0]);
            assert_eq!(frame.frame.unwrap().pixels(),expected.pixels(),"frame {i}");
            if i==1 {captured=expected.pixels().to_vec();}
            assert!(command.view.saved_screens.len()<=5);
            vm.eval("surface leases","return SystemCommandHost.current.len()+\":\"+SystemCommandHost.surface_leases.len()+\":\"+SystemCommandHost.opaque_assets.len()").unwrap();
            assert!(matches!(vm.result().as_str(),"1:1:0"|"1:2:0"),"{}",vm.result());
        }
        for _ in 0..2 {
            let frame=command.step(&mut store,&mut vm,&mut vars,SystemInput::default()).unwrap();
            assert_eq!(frame.frame.unwrap().pixels(),captured);
        }
        assert!(command.closed);
    }

    #[test]
    fn drawings_after_update_are_not_discarded_by_raster_compaction() {
        let mut store=DatGame::mount([resource_pack(b"system",b"probe.bin",b"test")]).unwrap();
        let mut vm=host(r#"
            com["after_update"] = function(args){
                local screen=Screen();screen.Fill(0xff0000);screen.Update()
                screen.Fill(0x00ff00);wait(0);screen.Update()
            }
        "#,"after_update");
        let mut vars=Variables::default();
        let (mut command,first)=SystemCommand::open(&mut store,&mut vm,&mut vars,"after_update",&[],None,
            RgbaImage::filled(1,1,[0,0,0,255],64).unwrap(),SystemInput::default()).unwrap();
        assert_eq!(first.frame.unwrap().pixels(),[255,0,0,255]);
        assert_eq!(command.next_surface,1_000_000);
        let next=command.step(&mut store,&mut vm,&mut vars,SystemInput::default()).unwrap();
        assert_eq!(next.frame.unwrap().pixels(),[0,255,0,255]);
    }

    #[test]
    #[ignore="requires prepared nested-room-fixture and installed Windows font; metadata-only original UI run"]
    fn original_music_1201_frames_retain_bounded_surfaces_and_plans() {
        let directory=std::path::Path::new("../../.local/system-integration/nested-room-fixture");
        let mut paths=std::fs::read_dir(directory).unwrap().map(|e|e.unwrap().path()).filter(|p|
            p.extension().is_some_and(|e|e=="dat"||e=="exe")).collect::<Vec<_>>();paths.sort();
        let inputs=paths.iter().map(|p|(p.file_name().unwrap().to_str().unwrap().to_owned(),std::fs::File::open(p).unwrap())).collect::<Vec<_>>();
        let mut store=DatGame::mount_named(inputs,true).unwrap();
        let (mut machine,mut vm)=crate::bootstrap_begin(&mut store,1,&mut std::io::sink(),true).unwrap();
        let font=crate::text_render::load_font(std::fs::read("C:/Windows/Fonts/msgothic.ttc").unwrap(),0).unwrap();
        let scene=||RgbaImage::filled(1600,900,[0,0,0,255],64*1024*1024).unwrap();
        let (mut command,first)=SystemCommand::open_with_screen_and_mask(&mut store,&mut vm,&mut machine.variables,
            "room",&[b"music".to_vec()],None,scene(),scene(),SystemInput::default(),mask::MaskState::default(),Some(font),None).unwrap();
        assert!(first.state_actions.contains(&StateAction::LoadCatalog));
        vm.eval("empty isolated catalog","SystemLoadInstall([])").unwrap();
        command.step(&mut store,&mut vm,&mut machine.variables,SystemInput {event:SystemEvent::NativeAck,..Default::default()}).unwrap();
        let started=std::time::Instant::now();
        let mut lengths=std::collections::BTreeSet::new();let mut max_surfaces=0;let mut max_leases=0;
        for i in 0..1201 {
            let frame=command.step(&mut store,&mut vm,&mut machine.variables,SystemInput {elapsed_ms:50,..Default::default()}).unwrap();
            assert!(frame.frame.is_some() && !frame.closed);
            max_surfaces=max_surfaces.max(command.view.saved_screens.len());
            assert!(command.view.saved_screens.len()<=3);
            vm.eval("original plan statistics","return SystemCommandHost.current.len()+\":\"+SystemCommandLines(SystemCommandHost.current).len()+\":\"+SystemCommandHost.surface_leases.len()+\":\"+SystemCommandHost.opaque_assets.len()").unwrap();
            let result=vm.result();let numbers=result.split(':').map(|s|s.parse::<usize>().unwrap()).collect::<Vec<_>>();
            assert_eq!(numbers[0],1);lengths.insert(numbers[1]);max_leases=max_leases.max(numbers[2]);
            assert!(numbers[2]<=2);assert_eq!(numbers[3],0,"original BASE has some alpha248..254 pixels");
            if (i+1)%200==0 {eprintln!("original music frames={} elapsed={:.2}s plans={lengths:?} surfaces={max_surfaces} leases={max_leases}",i+1,started.elapsed().as_secs_f64());}
        }
        assert_eq!(lengths.len(),1);
        let report=format!("{{\"passed\":true,\"frames\":1201,\"plan_lengths\":{:?},\"max_rust_surfaces\":{max_surfaces},\"max_vm_leases\":{max_leases},\"opaque_assets\":0,\"elapsed_seconds\":{:.3},\"artwork_inspected\":false}}\n",lengths.into_iter().collect::<Vec<_>>(),started.elapsed().as_secs_f64());
        std::fs::write(directory.join("desktop-long-loop-verification.json"),report).unwrap();
    }

    #[test]
    fn opaque_cover_requires_full_opacity_full_coverage_and_unmodified_asset() {
        let mut vm=host(r#"
            com["bounds"] = function(args){
                local screen=Screen(),sprite=Sprite(4,4)
                sprite.asset="known";SystemCommandHost.canvas=[4,4]
                SystemCommandHost.opaque_assets.rawset("known",[4,4])
                screen.Fill(0x112233)
                local saved=Sprite();saved.Screen();screen.Snap()
                screen.Draw(sprite,0,0,128)
                if(SystemCommandHost.current.len()!=2)throw "half opacity erased history"
                screen.Draw(sprite,1,0)
                if(SystemCommandHost.current.len()!=3)throw "partial coverage erased history"
                screen.Draw(sprite,0,0,2,2,255)
                if(SystemCommandHost.current.len()!=4)throw "crop erased history"
                sprite.layers=[["O",0x123456,100]]
                screen.Draw(sprite)
                if(SystemCommandHost.current.len()!=5)throw "modified asset erased history"
                sprite.layers=[]
                screen.Draw(sprite)
                if(SystemCommandHost.current.len()!=1)throw "opaque cover did not prune"
                if(saved.layers.len()!=1 || saved.layers[0][0]!="O")throw "Screen capture changed"
                if(SystemCommandHost.snapshot.len()!=1 || SystemCommandHost.snapshot[0][0]!="O")throw "Snap capture changed"
                sprite.asset="unknown"
                screen.Draw(sprite)
                if(SystemCommandHost.current.len()!=2)throw "unverified asset erased history"
            }
        "#,"bounds");
        vm.eval("bounded covers","return SystemCommandStep(0,false,0)").unwrap();
    }

    #[test]
    fn native_cancel_modes_emit_state_actions_and_change_current_skip_query() {
        let mut vm=host(r#"
            Host.auto_mode=true;Host.skip=true
            com["cancel_modes"] = function(args){
                CancelAuto();CancelSkip()
                if(InSkip() || Host.auto_mode || Host.skip)throw "native mode cancellation did not apply"
            }
        "#,"cancel_modes");
        vm.eval("cancel modes","return SystemCommandStep(0,true,0)").unwrap();
        assert_eq!(ParsedFrame::parse(&vm.result()).unwrap().state_actions,
                   [StateAction::CancelAuto,StateAction::CancelSkip]);
    }

        fn resource_pack(directory:&[u8],name:&[u8],payload:&[u8])->std::io::Cursor<Vec<u8>> {
            let length=payload.len() as u32;
            let mut bytes=b"NEKOPACK".to_vec();
            for word in [1,0,0,16,nekopack_archive::name_hash(1,directory),1,nekopack_archive::name_hash(1,name),length,0,length] {bytes.extend(word.to_le_bytes());}
            bytes.extend(payload);std::io::Cursor::new(bytes)
        }


    #[test]
    fn screen_snapshot_persists_between_commands_and_repeated_captures_are_bounded() {
        let mut store=DatGame::mount([resource_pack(b"system",b"probe.bin",b"test")]).unwrap();
        let mut vm=host(r#"
            com["capture"] = function(args){
                local s=Screen();s.Fill(0x112233);s.Snap();s.Fill(0xffffff)
            }
            com["show"] = function(args){local b=Sprite();b.Snap();local s=Screen();s.Draw(b);s.Update()}
            com["repeat"] = function(args){local s=Screen();for(local i=0;i<20;++i){s.Fill(i);s.Snap();wait(0)}}
        "#,"capture");
        let mut variables=Variables::default();
        let scene=||RgbaImage::filled(1,1,[0,0,0,255],64).unwrap();
        let (first,frame)=SystemCommand::open(&mut store,&mut vm,&mut variables,"capture",&[],None,scene(),SystemInput::default()).unwrap();
        assert!(frame.closed && frame.frame.is_none());
        let (mask,snapshot)=first.into_scene_state();
        let (_,frame)=SystemCommand::open_with_screen_and_mask(&mut store,&mut vm,&mut variables,"show",&[],None,scene(),scene(),SystemInput::default(),mask,None,Some(snapshot)).unwrap();
        assert_eq!(frame.frame.unwrap().pixels(),[17,34,51,255]);
        let (mut command,_)=SystemCommand::open(&mut store,&mut vm,&mut variables,"repeat",&[],None,scene(),SystemInput::default()).unwrap();
        for _ in 0..20 {command.step(&mut store,&mut vm,&mut variables,SystemInput::default()).unwrap();assert_eq!(command.view.saved_screens.len(),2);}
        assert!(command.closed);
    }

    #[test]
    fn mask_load_without_frame_is_retained_for_the_next_command() {
        let mask=RgbaImage::from_rgba(1,1,vec![255,0,128,255],64).unwrap();
        let mut png=Vec::new();mask.write_png(&mut png).unwrap();
        let mut store=DatGame::mount([resource_pack(b"image/mask",b"probe.png",&png)]).unwrap();
        let mut vm=host(r#"
            Config <- {Effect=1}
            com["load"] = function(args){TransSet(100,"probe")}
            com["draw"] = function(args){
                local s=Screen();s.Fill(0);TransBegin();s.Fill(0xffffff);TransEnd()
                TransSet(100,"missing");RenderTrans(128);s.Update()
            }
        "#,"load");
        let mut vars=Variables::default();
        let scene=||RgbaImage::filled(1,1,[0,0,0,255],64).unwrap();
        let (first,frame)=SystemCommand::open(&mut store,&mut vm,&mut vars,"load",&[],None,scene(),SystemInput::default()).unwrap();
        assert!(frame.closed && frame.frame.is_none());
        let (_,frame)=SystemCommand::open_with_screen_and_mask(&mut store,&mut vm,&mut vars,"draw",&[],None,scene(),scene(),SystemInput::default(),first.into_mask_state(),None,None).unwrap();
        assert_eq!(frame.frame.unwrap().pixels(),[255,0,255,255]);
    }

    #[test]
    fn images_empty_named_and_missing_members_remain_iterable() {
        struct Resources {calls: Vec<String>}
        impl nekopack_script::system::ResourceProvider for Resources {
            fn resource(&mut self,kind:&str,name:&str)->std::result::Result<String,String> {
                assert_eq!(kind,"Assets");self.calls.push(name.into());
                match name {
                    "present"=>Ok(r#"{["BASE"]=["system/synthetic.mng:0",1,2,3,4],["=TEST"]=["system/synthetic.mng:1",5,6,7,8]}"#.into()),
                    "missing"=>Ok("{}".into()),
                    _=>Err("unexpected test resource".into()),
                }
            }
        }
        let mut vm=host(r#"
            com["images"] = function(args){
                local empty=Images(),count=0
                foreach(key,sprite in empty)count++
                if(count!=0 || empty["absent"].avail || empty["absent"].asset!=null)throw "empty Images contract"
                local absent=Images("missing")
                foreach(key,sprite in absent)count++
                if(count!=0 || absent["absent"].width!=0)throw "missing collection contract"
                local images=Images("present")
                foreach(key,sprite in images){
                    if(typeof sprite!="instance")throw "sprite type"
                    count++
                }
                if(count!=2 || images["BASE"].width!=3 || images["=TEST"].x!=5)throw "named Images contract"
                if(images["absent"].avail || images["absent"].layers.len()!=0)throw "missing sprite contract"
                count=0;foreach(key,sprite in images)count++
                if(count!=2)throw "missing access inserted a phantom sprite"
                local spots=Spots(images)
                if(!spots.Bind("TEST",77))throw "original Spots foreach contract"
                return count
            }
        "#,"images");
        let mut resources=Resources {calls:Vec::new()};
        vm.eval_with_resources("images","return SystemCommandStep(0,false,0)",&mut Variables::default(),&mut resources).unwrap();
        assert_eq!(ParsedFrame::parse(&vm.result()).unwrap().return_value,Some(ScriptReturn::Integer(2)));
        assert_eq!(resources.calls,["missing","present"]);
    }

    fn host(script: &str, name: &str) -> SystemVm {
        let mut vm=SystemVm::new();
        vm.eval("headless", include_str!("../../nekopack-script/host/headless.nut")).unwrap();
        vm.eval("handler", script).unwrap();
        crate::audio_config::install_snapshotter(&mut vm).unwrap();
        vm.eval("host", include_str!("../host/system_command.nut")).unwrap();
        vm.eval("install", &format!("SystemCommandInstall(\"{name}\",[],null)")).unwrap();
        vm
    }

    #[test]
    fn sounds_keep_trigger_time_audio_snapshot_and_apply_modern_master_once() {
        let mut vm = host(r#"
            Config <- {VolMAS=67,MuteMAS=0,VolBGM=75,MuteBGM=0,VolVO=80,MuteVO=0,
                VolSE=60,MuteSE=0,VolENV=40,MuteENV=0,VolBGV=30,MuteBGV=0,
                VolSYS=50,MuteSYS=0,VolMV=50,MuteMV=0}
            com["capture"] = function(args){
                sound("first")
                Config.VolSYS=3;sound("second")
                Config.MuteSYS=1;sound("muted")
                Config.MuteSYS=0;Config.MuteMAS=1;sound("master_muted")
            }
        "#, "capture");
        vm.eval("capture", "return SystemCommandStep(0,false,0)").unwrap();
        let frame = ParsedFrame::parse(&vm.result()).unwrap();
        assert_eq!(frame.sounds.iter().map(|sound| sound.volume).collect::<Vec<_>>(), [33,2,0,0]);
    }

    #[test]
    fn original_wait_boundaries_preserve_scene_timer_input_and_restore_globals() {
        let mut vm=host(r#"
            Config <- {VolMAS=1,VolBGM=50,VolVO=50,VolSE=50,VolENV=50,VolSYS=45}
            OriginalScreen <- Screen
            com["timed"] = function(args){
                local base=Sprite();base.Screen()
                ClearImage()
                local s=Screen(),timer=Timer()
                while(wait(0)){
                    if(InSkip() || event()==ID_OK || timer.Get()>3000)break
                    s.Draw(base);s.Fill(0,timer.Get()-2700);s.Update()
                }
                sound("CUSTOM");return 7
            }
        "#, "timed");
        vm.eval("first", "return SystemCommandStep(0,false,0)").unwrap();
        let first=ParsedFrame::parse(&vm.result()).unwrap();
        assert!(!first.has_frame);assert_eq!(first.state_actions,[StateAction::ClearScene]);
        vm.eval("tick", "return SystemCommandStep(0,false,1000)").unwrap();
        let frame=ParsedFrame::parse(&vm.result()).unwrap();
        assert!(frame.has_frame);assert_eq!(frame.draws,"P\t1\t0\t0\nQ\n");
        vm.eval("restored", "return (Screen==OriginalScreen && !(\"ClearImage\" in getroottable())).tostring()").unwrap();
        assert_eq!(vm.result(),"true");
        vm.eval("confirm", "return SystemCommandStep(2000000001,false,100)").unwrap();
        let frame=ParsedFrame::parse(&vm.result()).unwrap();
        assert_eq!(frame.return_value,Some(ScriptReturn::Integer(7)));
        assert_eq!(frame.sounds,[OriginalSound{name:"CUSTOM".into(),volume:45}]);
    }

    #[test]
    fn immediate_handlers_keep_original_state_and_return_values() {
        let mut vm=host(r#"
            Config <- {VolSYS=50,KeepAuto=0}
            DemoMode <- false
            DelayTime <- 0
            com["state"] = function(args){
                Config.KeepAuto=1
                SwitchAuto()
                DemoMode=true
                DelayTime=125
                EnableDelay()
                return "a\tb\n"
            }
        "#, "state");
        vm.eval("step", "return SystemCommandStep(0,false,0)").unwrap();
        let frame=ParsedFrame::parse(&vm.result()).unwrap();
        assert!(!frame.has_frame);
        assert_eq!(frame.state_actions,[StateAction::ToggleAuto,StateAction::EnableDelay(125)]);
        assert_eq!(frame.return_value,Some(ScriptReturn::String(b"a\tb\n".to_vec())));
        vm.eval("state", "return (Config.KeepAuto==1 && DemoMode && DelayTime==125).tostring()").unwrap();
        assert_eq!(vm.result(),"true");
    }

    #[test]
    fn transition_without_snapshots_fails_and_restores_host_bindings() {
        let mut vm=host("OriginalScreen <- Screen;com[\"date\"] = function(args){RenderTrans()}","date");
        let failure=vm.eval("step","return SystemCommandStep(0,false,0)").unwrap_err();
        assert!(failure.contains("original transition snapshots are not installed"),"{failure}");
        vm.eval("restore","return (Screen==OriginalScreen).tostring()").unwrap();
        assert_eq!(vm.result(),"true");
    }

    #[test]
    fn captured_transition_and_screen_survive_update_and_source_changes() {
        let mut vm=host(r#"
            Config <- {Effect=1,VolSYS=50}
            com["date"] = function(args){
                local s=Screen()
                s.Fill(0x646464)
                TransBegin()
                s.Fill(0xc8c8c8)
                TransEnd()
                TransSet(2000)
                s.Fill(0)
                while(wait(0)){
                    RenderTrans()
                    s.Update()
                    local copy=Sprite();copy.Screen()
                    local second=Screen();second.Draw(copy)
                    if(event()==ID_OK)return true
                }
            }
        "#, "date");
        vm.eval("start","return SystemCommandStep(0,false,0)").unwrap();
        vm.eval("half","return SystemCommandStep(0,false,1000)").unwrap();
        let frame=ParsedFrame::parse(&vm.result()).unwrap();
        let encoded_a="4f0936353739333030093235360a"; // O 0x646464 256
        let encoded_b="4f093133313538363030093235360a"; // O 0xc8c8c8 256
        assert!(frame.draws.contains(&format!("X\t128\t{encoded_a}\t{encoded_b}\n")),"{}",frame.draws);
        vm.eval("closed","return SystemCommandStep(2000000001,false,1000)").unwrap();
        let frame=ParsedFrame::parse(&vm.result()).unwrap();
        assert_eq!(frame.return_value,Some(ScriptReturn::Bool(true)));
        assert!(frame.draws.contains("X\t256\t"));
    }
}
