//! Platform NKSV files projected into the original SaveInfo/LoadState stream.
use super::*;
use nekopack_image::RgbaImage;
use nekopack_script::checkpoint::{Reader, Writer};

#[derive(Clone, Debug)]
pub(super) struct NativeMessage {
    pub size: i64,
    pub name: String,
    pub voice: String,
    pub mask: String,
    pub text: String,
    pub climax: Option<i64>,
}
impl NativeMessage {
    pub(super) fn save(&self, w: &mut Writer) {
        w.number(self.size as u64);
        for value in [&self.name, &self.voice, &self.mask, &self.text] { w.text(value); }
        w.number(u64::from(self.climax.is_some()));
        if let Some(value) = self.climax { w.number(value as u64); }
    }
    pub(super) fn load(r: &mut Reader<'_>) -> Result<Self, Box<dyn Error>> {
        let size = r.number()? as i64;
        let name = r.text()?; let voice = r.text()?; let mask = r.text()?; let text = r.text()?;
        let climax = if r.count(1)? == 1 { Some(r.number()? as i64) } else { None };
        Ok(Self { size, name, voice, mask, text, climax })
    }
    pub(super) fn expression(&self, face: &str) -> Result<String, Box<dyn Error>> {
        Ok(format!("{{size={},name={},voice={},mask={},text={},face={}{}}}", self.size,
            quote(self.name.as_bytes())?, quote(self.voice.as_bytes())?, quote(self.mask.as_bytes())?,
            quote(self.text.as_bytes())?, quote(face.as_bytes())?, self.climax.map(|value| format!(",climax={value}")).unwrap_or_default()))
    }
}
struct Record { bytes: Vec<u8>, expression: String, messages: Vec<String>, back: Vec<String> }
#[derive(Default)]
pub(super) struct NativeLoad {
    records: BTreeMap<String, Record>,
    thumbnails: Vec<(u32, RgbaImage)>,
    prepared: Option<(String, saves::Saved)>,
    pub messages: Vec<NativeMessage>,
}

impl<R: Read + Seek> StorySession<R> {
    /// Called once after a new history entry is appended, never when rebuilding it.
    pub(super) fn capture_native_message(&mut self) -> Result<(), Box<dyn Error>> {
        let entry = self.message_history.entries.last().ok_or("native history capture without message")?;
        let (size, mask) = self.message_layout.as_ref()
            .map(|layout| (i64::from(layout.text_size), layout.message.mask.clone()))
            .unwrap_or((30, String::new()));
        self.native_load.messages.push(NativeMessage { size, mask, name: entry.name.clone(),
            voice: entry.voice.clone(), text: entry.text.clone(),
            climax: self.system_save_state.get("ClimaxCount").copied().filter(|value| *value != 0) });
        Ok(())
    }

    pub fn system_load_catalog_clear(&mut self) -> Result<(), Box<dyn Error>> {
        self.native_load.records.clear();
        self.native_load.thumbnails.clear();
        self.native_load.prepared = None;
        Ok(())
    }

    /// Platform normal slots 1..=100, quick slots 101..=110 map to qsave01..qsave10.
    /// Malformed NKSV files remain in enumeration and throw at the original file-open boundary.
    pub fn system_load_catalog_slot(&mut self, id: u32, modified: u64, date: &str, time: &str, bytes: &[u8]) -> Result<(), Box<dyn Error>> {
        let filename = match id {
            1..=100 => format!("save{id:03}.sav"),
            101..=110 => format!("qsave{:02}.sav", id-100),
            _ => return Err("invalid native save slot".into()),
        };
        if date.is_empty() || time.is_empty() || date.chars().chain(time.chars()).any(char::is_control) {
            return Err("invalid native save date/time".into());
        }
        let state_id = id.to_string();
        if self.native_load.records.contains_key(&state_id) { return Err("duplicate native save slot".into()); }
        // Decimal strings preserve the full platform timestamp in the 32-bit Squirrel VM.
        let timestamp = format!("{modified:020}");
        let prefix = format!("{{path=SavePath({}),modified={},state_id={}", quote(filename.as_bytes())?,
            quote(timestamp.as_bytes())?, quote(state_id.as_bytes())?);
        let decoded = saves::Saved::decode(bytes, &mut self.store, self.scene.dimensions, false);
        let mut messages=Vec::new();
        let mut back=Vec::new();
        let expression = match decoded {
            Ok(saved) => {
                let values = saved.native_values(date, time)?;
                messages = saved.native_message_expressions()?;
                back = saved.native_back_expressions(&mut self.store, self.scene.dimensions)?;
                self.system.eval("native-thumbnail-size", "return SystemLoadDimensions()")?;
                let dimensions = self.system.result();
                let (width,height) = dimensions.split_once(',').ok_or("invalid original thumbnail dimensions")?;
                let [width,height]=[width.parse()?,height.parse()?];
                let preview = saved.preview_size(String::new(), [width,height])?;
                let thumbnail_id = 1000 + id;
                self.native_load.thumbnails.push((thumbnail_id, preview.thumbnail));
                format!("{prefix},metadata={values},thumbnail=SystemLoadThumbnailSprite({thumbnail_id},{width},{height})}}")
            }
            Err(error) => format!("{prefix},error={}}}", quote(error.to_string().as_bytes())?),
        };
        self.native_load.records.insert(state_id, Record { bytes: bytes.to_vec(), expression, messages, back });
        Ok(())
    }

    /// Installs records while the coroutine is suspended. Host event 6 resumes the original caller.
    pub fn system_load_catalog_install(&mut self) -> Result<(), Box<dyn Error>> {
        // Compile one bounded metadata entry at a time. The aggregate history
        // and catalog can legitimately exceed the VM's module source limit.
        self.system.eval("native-save-catalog", "NekoLoadRecords <- []")?;
        for record in self.native_load.records.values() {
            self.system.eval("native-save-record", &format!("NekoLoadRecords.append({})",record.expression))?;
            for entry in &record.messages {
                self.system.eval("native-save-message", &format!("NekoLoadRecords.top().metadata.globals.MessageLog.append({entry})"))?;
            }
            for entry in &record.back {
                self.system.eval("native-save-back", &format!("NekoLoadRecords.top().metadata.globals.SkipBackData.append({entry})"))?;
            }
            self.system.eval("native-save-project", r#"
                local record=NekoLoadRecords.top()
                if("metadata" in record){
                    record.values <- SystemLoadProject(record.metadata)
                    record.rawdelete("metadata")
                }
            "#)?;
        }
        self.system.eval("native-save-catalog-install", "SystemLoadInstall(NekoLoadRecords);getroottable().rawdelete(\"NekoLoadRecords\")")?;
        let thumbnails = self.take_system_load_thumbnails();
        self.system_command.as_mut().ok_or("native catalog without system command")?.command.install_save_thumbnails(thumbnails);
        Ok(())
    }
    pub(super) fn take_system_load_thumbnails(&mut self) -> Vec<(u32, RgbaImage)> {
        std::mem::take(&mut self.native_load.thumbnails)
    }

    /// Prepares a validated VM/scene/audio snapshot without replacing the executing coroutine.
    pub(super) fn prepare_system_load(&mut self, state_id: &str) -> Result<(), Box<dyn Error>> {
        if self.native_load.prepared.is_some() { return Err("native load already prepared".into()); }
        let record = self.native_load.records.get(state_id).ok_or("native load slot not registered")?;
        let saved = saves::Saved::decode(&record.bytes, &mut self.store, self.scene.dimensions, true)?;
        self.native_load.prepared = Some((state_id.to_owned(), saved));
        self.system.eval("native-load-ack", &format!("SystemLoadAcknowledge({},true)",quote(state_id.as_bytes())?))?;
        Ok(())
    }

    /// Called only after the acknowledged LoadState's original Sleep/throw-null unwind.
    pub(super) fn commit_system_load(&mut self, state_id: &str) -> Result<Vec<u8>, Box<dyn Error>> {
        let (prepared_id, _) = self.native_load.prepared.as_ref().ok_or("native load not prepared")?;
        if prepared_id != state_id { return Err("native load commit identity mismatch".into()); }
        let (_, saved) = self.native_load.prepared.take().unwrap();
        self.system_load_catalog_clear()?;
        self.apply_saved(saved)
    }
}

pub(super) fn thumbnail(image: &RgbaImage, size: [u32;2]) -> Result<RgbaImage, Box<dyn Error>> {
    let [width,height] = size;
    if width == 0 || height == 0 || width > 4096 || height > 4096 { return Err("invalid save thumbnail dimensions".into()); }
    let mut result = RgbaImage::transparent(width,height,64*1024*1024)?;
    for y in 0..height { for x in 0..width {
        let x0=x*image.width()/width;
        let x1=((x+1)*image.width()/width).max(x0+1).min(image.width());
        let y0=y*image.height()/height;
        let y1=((y+1)*image.height()/height).max(y0+1).min(image.height());
        let mut sum=[0u64;4];let mut count=0;
        for sy in y0..y1 { for sx in x0..x1 {
            let at=((sy*image.width()+sx)*4) as usize;
            for (channel,value) in sum.iter_mut().enumerate() { *value+=u64::from(image.pixels()[at+channel]); }
            count+=1;
        }}
        let at=((y*width+x)*4) as usize;
        for (channel,value) in sum.iter().enumerate() { result.pixels_mut()[at+channel]=(value/count) as u8; }
    }}
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::story::tests::{session, resource_pack};
    const SCRIPT: &[u8] = b".var L1 marker\n? marker = 7\nrestored\n? marker = 8\nnext";
    fn configured(script: &[u8]) -> StorySession<std::io::Cursor<Vec<u8>>> {
        let mut story = session(script);
        story.store = DatGame::mount([resource_pack(b"script",b"test.txt",SCRIPT)]).unwrap();
        story.system.eval("headless",include_str!("../../../nekopack-script/host/headless.nut")).unwrap();
        story.system.eval("load-fixture",r#"
            StoryTitle <- "synthetic";StoryRoute <- "probe"
            function InRoute(){return true}
            function SavePath(name){return name}
            const THUMB_WIDTH=206
            const THUMB_HEIGHT=116
            class SaveInfo {
                thumb=null;sdate="";stime="";route="";title="";text=""
                constructor(){thumb=Sprite(206,116)}
                function Save(f){thumb.SaveFile(f);::Save(f,sdate);::Save(f,stime);::Save(f,route);
                    ::Save(f,title);::Save(f,text);::Save(f,MessageLog);::Save(f,ClimaxCount);::Save(f,SkipBackData)}
            }
            Config <- {VolBGM=50,VolVO=50,VolSE=50,VolENV=50,VolSYS=50,Effect=1,
                TextSpeed=100,KeepVoice=1,ReadedFast=0,AutoSpeed=50,SkipNoRead=0}
            function LoadFixture(){
                local files=EnumFiles(SavePath(""),"save*.sav")
                if(files.len()==0)return
                local f=file(SavePath(files[0]),"rb")
                local thumb=Sprite(f)
                local date=Load(f),time=Load(f)
                StoryRoute=Load(f);StoryTitle=Load(f);local text=Load(f)
                MessageLog=Load(f);ClimaxCount=Load(f);SkipBackData=Load(f)
                if(LoadState(f)){Sleep(1000);throw null}
            }
            Marker <- 0
            MessageLog <- [];ClimaxCount <- 0;SkipBackData <- []
            com["continue"]=function(args){LoadFixture();Marker=99}
        "#).unwrap();
        story.labels=StoryLabels {title:"synthetic".into(),route:"probe".into(),in_route:true,..Default::default()};
        story.enable_host_effects();story.enable_interactive_text();
        story
    }
    fn saved() -> Vec<u8> {
        let mut story=configured(SCRIPT);
        story.poll_packet().unwrap();story.text_frame(1,0,false).unwrap();
        story.dialogue_modal=true;
        story.native_load.messages[0].size=42;
        story.native_load.messages[0].mask="role-tag".into();
        story.native_load.messages[0].climax=Some(3);
        story.system_save_state.insert("ClimaxCount".into(),3);
        story.export_save().unwrap()
    }
    fn has(packet:&[u8],value:&str)->bool {
        std::str::from_utf8(packet.split(|b|*b==b'\n').next().unwrap()).unwrap().contains(value)
    }
    #[test]
    fn original_load_transaction_restores_real_nksv_after_sleep_without_completing_old_machine() {
        let bytes=saved();
        assert_eq!(&bytes[..5],b"NKSV\x09");
        let mut story=configured(b".continue\nold-machine-marker");
        let first=story.poll_packet().unwrap();
        assert!(has(&first,"\"load_catalog\":true"));
        story.system_load_catalog_slot(1,100,"2026/11/13","20:05",&bytes).unwrap();
        story.system_load_catalog_slot(2,200,"2026/11/13","20:06",&bytes).unwrap();
        story.system_load_catalog_slot(101,300,"2026/11/13","20:07",&bytes).unwrap();
        story.system_load_catalog_install().unwrap();
        assert_eq!(story.system_command_step(1,0,0).unwrap(),first);
        let prepared=story.system_command_step(1,0,6).unwrap();
        assert!(has(&prepared,"\"native_ack\":true"));
        assert_eq!(story.native_load.prepared.as_ref().unwrap().0,"2");
        story.system.eval("metadata","return MessageLog[0].size+\":\"+MessageLog[0].mask+\":\"+MessageLog[0].climax").unwrap();
        assert_eq!(story.system.result(),"42:role-tag:3");
        let sleeping=story.system_command_step(1,0,6).unwrap();
        assert!(has(&sleeping,"\"delay_ms\":1000"));
        assert_eq!(story.system_command_step(1,999,0).unwrap(),sleeping);
        let restored=story.system_command_step(1,1000,0).unwrap();
        assert!(has(&restored,"restored"));
        assert_eq!(story.machine.variables.get(b"marker").unwrap(),7);
        assert_eq!(story.page.as_ref().unwrap().text,"restored");
        assert!(story.system_command.is_none());
        assert!(story.system_command_step(1,1001,6).is_err());
        story.system.eval("old-stack","return Marker.tostring()").unwrap();assert_eq!(story.system.result(),"0");
    }
    #[test]
    fn corrupt_catalog_record_is_not_silently_removed_or_committed() {
        let mut story=configured(b".continue\nold");
        story.poll_packet().unwrap();
        story.system_load_catalog_slot(1,100,"2026/11/13","20:05",b"NKSVbad").unwrap();
        story.system_load_catalog_install().unwrap();
        let error=story.system_command_step(1,0,6).unwrap_err();
        assert!(error.to_string().contains("unknown runtime save format"));
        assert!(story.native_load.prepared.is_none());
        assert!(story.page.is_none());
    }
    #[test]
    fn empty_catalog_continues_without_preparing_a_load() {
        let mut story=configured(b".continue\nempty-return");
        story.poll_packet().unwrap();story.system_load_catalog_install().unwrap();
        let packet=story.system_command_step(1,0,6).unwrap();
        assert!(has(&packet,"empty-return"));
        assert!(story.native_load.prepared.is_none());
        story.system.eval("marker","return Marker.tostring()").unwrap();assert_eq!(story.system.result(),"99");
    }
    #[test]
    fn thumbnail_has_original_dimensions_and_preserves_pixels() {
        let mut original=RgbaImage::transparent(1,1,4).unwrap();
        original.pixels_mut().copy_from_slice(&[12,34,56,255]);
        let scaled=thumbnail(&original,[206,116]).unwrap();
        assert_eq!([scaled.width(),scaled.height()],[206,116]);
        assert!(scaled.pixels().chunks_exact(4).all(|pixel|pixel==[12,34,56,255]));
    }

    #[test]
    fn nksv9_preserves_real_back_checkpoints_and_selection_history() {
        let script=b".var L1 marker\n.select\none\ntwo\n.case 1\n? marker = 9\nchosen\n.case 2\nother\n.end";
        let mut source=configured(script);
        source.store=DatGame::mount([resource_pack(b"script",b"test.txt",script)]).unwrap();
        source.poll_packet().unwrap();
        source.capture_selection_checkpoint().unwrap();
        source.machine.complete(Some(1)).unwrap();source.pending_effect=None;
        source.poll_packet().unwrap();source.text_frame(1,0,false).unwrap();source.dialogue_modal=true;
        let bytes=source.export_save().unwrap();
        let mut restored=configured(script);
        restored.store=DatGame::mount([resource_pack(b"script",b"test.txt",script)]).unwrap();
        restored.restore_route_preview(&bytes).unwrap();
        assert_eq!(restored.navigation_back.len(),1);
        assert_eq!(restored.machine.variables.get(b"marker").unwrap(),9);
        restored.system.eval("restored-native-back","return SkipBackData.len()+\":\"+SkipBackData[0].pos+\":\"+SkipBackData[0].data.slice(0,5)").unwrap();
        assert_eq!(restored.system.result(),"1:0:NKSC:");
        let sequence=restored.page.as_ref().unwrap().sequence;
        restored.text_frame(sequence,0,false).unwrap();
        restored.request_shortcut_back(sequence).unwrap();
        assert_eq!(restored.machine.variables.get(b"marker").unwrap(),0);
        assert!(restored.navigation_back.is_empty());
        assert!(restored.message_history.entries.is_empty());
    }

    #[test]
    fn long_message_history_restores_through_native_vm() {
        let mut source=configured(SCRIPT);
        source.poll_packet().unwrap();source.text_frame(1,0,false).unwrap();
        source.dialogue_modal=true;
        let current=source.message_history.entries[0].clone();
        let current_native=source.native_load.messages[0].clone();
        let mut entry=current.clone();
        let mut native=current_native.clone();
        entry.text="long history ".repeat(250);
        native.text=entry.text.clone();
        for _ in 1..512 {
            source.message_history.push(entry.clone()).unwrap();
            source.native_load.messages.push(native.clone());
        }
        source.message_history.push(current).unwrap();
        source.native_load.messages.push(current_native);
        let bytes=source.export_save().unwrap();
        let mut restored=configured(SCRIPT);
        restored.restore_route_preview(&bytes).unwrap();
        assert_eq!(restored.message_history.entries.len(),513);
        assert_eq!(restored.native_load.messages.len(),513);
        restored.system.eval("long-history-size","return MessageLog.len().tostring()").unwrap();
        assert_eq!(restored.system.result(),"513");
        restored.system.eval("long-history-content","return MessageLog[1].text.len()+\":\"+MessageLog.top().text").unwrap();
        assert_eq!(restored.system.result(),format!("{}:restored",entry.text.len()));

        let mut catalog=configured(b".continue\nold-machine");
        catalog.poll_packet().unwrap();
        for id in 1..=2 {
            catalog.system_load_catalog_slot(id,u64::from(id),"2026/11/14","12:00",&bytes).unwrap();
        }
        catalog.system_load_catalog_install().unwrap();
        assert!(has(&catalog.system_command_step(1,0,6).unwrap(),"\"native_ack\":true"));
        catalog.system.eval("catalog-history","return MessageLog.len()+\":\"+MessageLog[1].text.len()").unwrap();
        assert_eq!(catalog.system.result(),format!("513:{}",entry.text.len()));
        catalog.system_command_step(1,0,6).unwrap();
        let packet=catalog.system_command_step(1,1000,0).unwrap();
        assert!(has(&packet,"restored"));
        assert_eq!(catalog.native_load.messages.len(),513);
    }

    #[test]
    fn large_back_checkpoint_keeps_full_state_across_repeated_save_and_load() {
        let script=b".var L1 marker\n.select\none\ntwo\n.case 1\n? marker = 9\nchosen\n.case 2\nother\n.end";
        let make=||{
            let mut story=configured(script);
            story.store=DatGame::mount([resource_pack(b"script",b"test.txt",script)]).unwrap();
            story
        };
        let mut source=make();
        source.poll_packet().unwrap();
        let text="checkpoint history ".repeat(150);
        for _ in 0..256 {
            source.message_history.push(crate::message_log::Entry {
                name:String::new(),voice:String::new(),face:String::new(),text:text.clone(),
            }).unwrap();
            source.native_load.messages.push(NativeMessage {
                size:42,name:String::new(),voice:String::new(),mask:"saved-mask".into(),text:text.clone(),climax:Some(3),
            });
        }
        source.capture_selection_checkpoint().unwrap();
        let checkpoint=source.navigation_back[0].clone();
        assert!(checkpoint.len()>1024*1024);
        let identity=format!("NKSC:{:016x}",nekopack_script::checkpoint::digest(&checkpoint));
        source.machine.complete(Some(1)).unwrap();source.pending_effect=None;
        source.poll_packet().unwrap();source.text_frame(1,0,false).unwrap();source.dialogue_modal=true;
        let mut bytes=source.export_save().unwrap();
        for _ in 0..2 {
            let mut restored=make();
            restored.restore_route_preview(&bytes).unwrap();
            assert_eq!(restored.navigation_back,[checkpoint.clone()]);
            restored.system.eval("back-identity","return SkipBackData[0].pos+\":\"+SkipBackData[0].data").unwrap();
            assert_eq!(restored.system.result(),format!("256:{identity}"));
            let sequence=restored.page.as_ref().unwrap().sequence;
            restored.text_frame(sequence,0,false).unwrap();restored.dialogue_modal=true;
            bytes=restored.export_save().unwrap();
            restored.dialogue_modal=false;
            restored.request_shortcut_back(sequence).unwrap();
            assert_eq!(restored.machine.variables.get(b"marker").unwrap(),0);
            assert!(restored.navigation_back.is_empty());
            assert_eq!(restored.message_history.entries.len(),256);
            assert_eq!(restored.message_history.entries[255].text,text);
            assert_eq!(restored.native_load.messages[255].mask,"saved-mask");
            assert_eq!(restored.native_load.messages[255].climax,Some(3));
        }
    }

    #[test]
    fn legacy_nksv8_restores_known_history_without_inventing_back_points() {
        let bytes=saved();
        let mut source=configured(SCRIPT);
        let saved=saves::Saved::decode(&bytes,&mut source.store,[1,1],false).unwrap();
        source.apply_saved(saved).unwrap();
        let mut extension=Writer::default();
        extension.number(source.native_load.messages.len() as u64);
        for message in &source.native_load.messages { message.save(&mut extension); }
        extension.number(0);
        assert!(bytes.ends_with(&extension.0));
        let mut legacy=bytes[..bytes.len()-extension.0.len()].to_vec();legacy[4]=8;
        let checksum=nekopack_script::checkpoint::digest(&legacy[13..]);
        legacy[5..13].copy_from_slice(&checksum.to_le_bytes());
        let mut restored=configured(SCRIPT);restored.restore_route_preview(&legacy).unwrap();
        assert_eq!(restored.page.as_ref().unwrap().text,"restored");
        assert_eq!(restored.machine.variables.get(b"marker").unwrap(),7);
        assert_eq!(restored.native_load.messages[0].size,30);
        assert_eq!(restored.native_load.messages[0].mask,"");
        assert!(restored.navigation_back.is_empty());
        restored.system.eval("legacy-log","return MessageLog.len()+\":\"+SkipBackData.len()+\":\"+ClimaxCount").unwrap();
        assert_eq!(restored.system.result(),"1:0:3");
    }

    #[test]
    #[ignore = "requires local user-owned technical saveload.nut fixture"]
    fn original_saveload_continue_restores_real_nksv() {
        let bytes=saved();
        let mut story=configured(b".continue\nold-machine-marker");
        story.system.eval("original-load-support",r#"
            getconsttable().rawdelete("THUMB_WIDTH");getconsttable().rawdelete("THUMB_HEIGHT")
            function UpdateCaption(){}
        "#).unwrap();
        let path=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.local/system-command-coverage/sorakoi/saveload.nut");
        story.system.eval("original-saveload",&std::fs::read_to_string(path).unwrap()).unwrap();
        let packet=story.poll_packet().unwrap();assert!(has(&packet,"\"load_catalog\":true"));
        story.system_load_catalog_slot(2,100,"2026/11/13","20:06",&bytes).unwrap();
        story.system_load_catalog_install().unwrap();
        assert!(has(&story.system_command_step(1,0,6).unwrap(),"\"native_ack\":true"));
        assert!(has(&story.system_command_step(1,0,6).unwrap(),"\"delay_ms\":1000"));
        let packet=story.system_command_step(1,1000,0).unwrap();assert!(has(&packet,"restored"));
        assert_eq!(story.machine.variables.get(b"marker").unwrap(),7);
        assert!(story.system_command_step(1,1001,6).is_err());
    }

    #[test]
    #[ignore = "requires local user-owned TD1..4/ED technical saveload fixtures"]
    fn five_other_original_load_data_handlers_restore_real_nksv() {
        let bytes=saved();
        for path in [
            "D:/codex/电脑移植安卓/research/inspection/pc-utf8/TD1/saveload.nut",
            "D:/codex/电脑移植安卓/research/inspection/pc-utf8/TD2/saveload.nut",
            "D:/codex/电脑移植安卓/research/inspection/pc-utf8/TD3/saveload.nut",
            "D:/codex/电脑移植安卓/research/inspection/pc-utf8/TD4/saveload.nut",
            "D:/codex/neko/.local/system-command-coverage/ed/saveload.nut",
        ] {
            let mut story=configured(b".continue\nold-machine-marker");
            story.system.eval("original-load-support","function UpdateCaption(){}").unwrap();
            story.system.eval(path,&std::fs::read_to_string(path).unwrap()).unwrap();
            story.system.eval("original-load-entry",r#"com["continue"]=function(args){LoadData(SaveName(2));Marker=99}"#).unwrap();
            assert!(has(&story.poll_packet().unwrap(),"\"load_catalog\":true"),"{path}");
            story.system_load_catalog_slot(2,100,"2026/11/13","20:06",&bytes).unwrap();
            story.system_load_catalog_install().unwrap();
            assert!(has(&story.system_command_step(1,0,6).unwrap(),"\"native_ack\":true"),"{path}");
            assert!(has(&story.system_command_step(1,0,6).unwrap(),"\"delay_ms\":1000"),"{path}");
            assert!(has(&story.system_command_step(1,1000,0).unwrap(),"restored"),"{path}");
            assert_eq!(story.machine.variables.get(b"marker").unwrap(),7,"{path}");
            story.system.eval("marker","return Marker.tostring()").unwrap();assert_eq!(story.system.result(),"0","{path}");
            assert!(story.system_command_step(1,1001,6).is_err(),"{path}");
        }
    }
}
