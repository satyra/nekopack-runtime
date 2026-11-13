//! Read-only virtual file/coroutine contracts; no original save byte compatibility.
use nekopack_script::system::SystemVm;

fn vm() -> SystemVm {
    let mut vm=SystemVm::try_new().unwrap();
    vm.eval("load-test-host",r#"
        SystemCommandHost <- {actions=[],delay=0,host_pending=false}
        Host <- {clock=0}
        function SavePath(name){return "profile/"+name}
        function SystemCommandFile(path,mode){throw "unsupported write"}
        function SystemCommandHex(text){local out="";foreach(c in text)out+=format("%02x",c);return out}
        function SystemCommandWait(delay){SystemCommandHost.delay=delay;suspend();return true}
        function check(value){if(!value)throw "load bridge contract assertion"}
        class CommandRegistry {entries=null;constructor(){entries={}}
            function _set(k,v){entries.rawset(k,v)}function _get(k){return entries[k]}}
        function record(name,modified,id){return {path=SavePath(name),modified=modified,state_id=id,
            thumbnail={marker=7},values=["2026/11/13","8:20","route","title","text",[],3,[]]}}
    "#).unwrap();
    vm.eval("system-load",include_str!("../host/system_load.nut")).unwrap();
    vm.eval("bind-load",r#"foreach(key,value in SystemLoadBindings())getroottable().rawset(key,value)"#).unwrap();
    vm
}

#[test]
fn enumeration_is_latest_normal_file_and_read_handles_have_independent_cursors() {
    let mut vm=vm();
    vm.eval("contract",r#"
        SystemLoadInstall([record("save001.sav",100,"one"),record("save002.sav",300,"two"),record("qsave01.sav",500,"quick")])
        local files=EnumFiles(SavePath(""),"save*.sav")
        check(files.len()==2 && files[0]=="save002.sav")
        local a=file(SavePath(files[0]),"rb"),b=file(SavePath(files[0]),"rb")
        check(SystemLoadThumbnail(a).marker==7 && SystemLoadThumbnail(b).marker==7)
        check(Load(a)=="2026/11/13" && Load(a)=="8:20" && Load(b)=="2026/11/13")
        a.record.values[5]=[{value=1}]
        for(local i=0;i<3;i++)Load(a)
        local history=Load(a);history[0].value=9
        check(a.record.values[5][0].value==1)
        local missing=false;try{file(SavePath("save003.sav"),"rb")}catch(error){missing=true}
        check(missing)
    "#).unwrap();
}

#[test]
fn lazy_catalog_waits_for_install_and_refreshes_original_newest_index() {
    let mut vm=vm();
    vm.eval("lazy-catalog",r#"
        SystemLoadReset()
        Selected <- null
        Thread <- newthread(function(){Selected=EnumFiles(SavePath(""),"save*.sav")})
        Thread.call()
        check(Thread.getstatus()=="suspended" && SystemLoadHost.catalog_pending)
        check(SystemCommandHost.actions[0]=="load_catalog" && SystemCommandHost.host_pending)
        SystemLoadInstall([record("save001.sav","00000000000000000100","one"),record("save002.sav","00000000000000000300","two")])
        Thread.wakeup()
        check(Thread.getstatus()=="idle" && Selected[0]=="save002.sav")
    "#).unwrap();
}

fn install_handler(vm: &mut SystemVm) {
    vm.eval("nested-load-contract",r#"
        SystemLoadInstall([record("save002.sav",300,"two")])
        Marker <- 0;Caught <- false
        function LoadDataContract(name){
            local f=file(name,"rb");SystemLoadThumbnail(f)
            foreach(item in f.record.values)Load(f)
            if(LoadState(f)){Sleep(1000);throw null}
        }
        function InnerPage(){LoadDataContract(SavePath("save002.sav"));Marker=1}
        Thread <- newthread(function(){
            try{InnerPage();Marker=2}catch(error){if(!SystemLoadHandleExit(error)){Caught=true;throw error}}
        })
        Thread.call()
        check(Thread.getstatus()=="suspended" && Marker==0)
        check(SystemCommandHost.actions[0]=="load_state\t74776f")
    "#).unwrap();
}

#[test]
fn success_keeps_nested_stack_until_original_sleep_and_null_unwind() {
    let mut vm=vm();install_handler(&mut vm);
    vm.eval("accepted",r#"
        SystemLoadAcknowledge("two",true);Thread.wakeup()
        check(Thread.getstatus()=="suspended" && SystemCommandHost.delay==1000 && Marker==0)
        check(SystemCommandHost.actions.len()==1)
        Thread.wakeup()
        check(Thread.getstatus()=="idle" && Marker==0 && !Caught)
        check(SystemCommandHost.actions[1]=="load_commit\t74776f")
        check(!SystemLoadHandleExit(null))
    "#).unwrap();
}

#[test]
fn rejected_load_returns_to_original_caller_without_commit_or_sleep() {
    let mut vm=vm();install_handler(&mut vm);
    vm.eval("rejected",r#"
        SystemLoadAcknowledge("two",false);Thread.wakeup()
        check(Thread.getstatus()=="idle" && Marker==2 && !Caught)
        check(SystemCommandHost.actions.len()==1 && SystemLoadHost.exit_state==null)
        check(!SystemLoadHandleExit(null))
    "#).unwrap();
}

#[test]
fn metadata_order_and_acknowledgement_are_explicit_boundaries() {
    let mut vm=vm();install_handler(&mut vm);
    vm.eval("wrong-ack",r#"
        local rejected=false;try{SystemLoadAcknowledge("one",true)}catch(error){rejected=true}
        check(rejected)
        local f=file(SavePath("save002.sav"),"rb")
        rejected=false
        try{Load(f)}catch(error){rejected=true}
        check(rejected)
        SystemLoadThumbnail(f)
        rejected=false
        try{LoadState(f)}catch(error){rejected=true}
        check(rejected)
        check(!SystemLoadHandleExit("real failure"))
    "#).unwrap();
}

#[test]
#[ignore = "requires local user-owned technical saveload.nut fixture"]
fn original_continue_and_saveinfo_execute_through_virtual_files() {
    let mut vm=vm();
    vm.eval("original-primitives",r#"
        com <- CommandRegistry()
        MessageLog <- []
        ClimaxCount <- 0
        SkipBackData <- []
        StoryTitle <- "";StoryRoute <- "";CaptionUpdates <- 0
        function UpdateCaption(){CaptionUpdates++}
        class Sprite {width=0;height=0;source=null;constructor(w=0,h=0){width=w;height=h}
            function LoadFile(f){source=SystemLoadThumbnail(f)}}
        SystemLoadInstall([record("save001.sav",100,"one"),record("save002.sav",300,"two"),record("qsave01.sav",500,"quick")])
    "#).unwrap();
    let fixture=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.local/system-command-coverage/sorakoi/saveload.nut");
    vm.eval("original-saveload",&std::fs::read_to_string(fixture).unwrap()).unwrap();
    vm.eval("original-continue",r#"
        check(NEWEST_SAVE==2)
        MessageLog=["unchanged"]
        local info=SaveInfo();info.Get(file(SaveName(2),"rb"))
        check(MessageLog[0]=="unchanged" && info.thumb.source.marker==7)
        Marker <- 0
        Thread <- newthread(function(){try{local handler=com["continue"];handler([]);Marker=1}catch(error){if(!SystemLoadHandleExit(error))throw error}})
        Thread.call()
        check(SystemLoadHost.pending=="two" && CaptionUpdates==1 && ClimaxCount==3 && MessageLog.len()==0)
        SystemLoadAcknowledge("two",true);Thread.wakeup();check(SystemCommandHost.delay==1000)
        Thread.wakeup();check(Thread.getstatus()=="idle" && Marker==0)
        check(SystemCommandHost.actions[1]=="load_commit\t74776f")
    "#).unwrap();
}

#[test]
#[ignore = "requires local user-owned technical saveload/config fixtures"]
fn original_sysload_keeps_page_stack_across_nested_confirmation_cancel() {
    let mut vm=vm();
    vm.eval("original-ui-primitives",r#"
        com <- CommandRegistry()
        MessageLog <- []
        ClimaxCount <- 0
        SkipBackData <- []
        StoryTitle <- "";StoryRoute <- "";Event <- 0;DialogResult <- false;Dialogs <- 0;Draws <- 0
        const ID_SAVE=101
        const ID_LOAD=102
        const ID_QLOAD=103
        const ID_CANCEL=104
        const ID_OK=105
        const ID_RESET=106
        const ID_USER=1000
        function UpdateCaption(){}
        function sound(name){}
        function event(){return Event}
        function wait(delay){return SystemCommandWait(delay)}
        class Sprite {
            x=0;y=0;width=206;height=116;source=null
            constructor(w=null,h=null){if(typeof w=="integer"){width=w;height=h}}
            function LoadFile(f){source=SystemLoadThumbnail(f)}
            function Base(){} function Draw(a,x=0,y=0){} function Clear(){}
            function Alpha(a){} function Fill(c,a){} function Update(){Draws++}
        }
        class Screen extends Sprite {}
        class Spot {id=0;is_enable=false;normal=null;hover=null;disable=null}
        class Spots {
            entries=null;constructor(images){entries=[]}
            function BindR(name,id){} function Add(spot){entries.append(spot)}
            function Check(){foreach(spot in entries)if(spot.is_enable)return spot.id;return 0}
            function Draw(screen){}
        }
        class Glyph {Face="";Color=0;Size=0;Bold=false;function Print(s,t,x,y){}}
        class GUI {page=0;function Bind(a,b){} function Set(k,v){page=v}
            function Get(k){return page}function Update(){}function Draw(s){}}
        function RadioButton(a,b,c){return null}
        function Images(name){
            local result={};foreach(key in ["SAMPLE","DATA=","DATA+","BASE","SAVE","LOAD"])result[key]<-Sprite()
            for(local i=0;i<8;i++)result["DATA"+i]<-Sprite()
            return result
        }
        function ViewDialog(name){Dialogs++;wait(-1);Event=0;return DialogResult}
        SystemLoadInstall([record("save001.sav",100,"one")])
    "#).unwrap();
    let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.local/system-command-coverage/sorakoi");
    vm.eval("original-saveload",&std::fs::read_to_string(root.join("saveload.nut")).unwrap()).unwrap();
    // Execute the original accessors without evaluating unrelated config globals.
    let config=std::fs::read_to_string(root.join("config.nut")).unwrap();
    let accessors=config.lines().map(str::trim).filter(|line|
        line.starts_with("function ViewLoad()") || line.starts_with("com[\"sysload\"]"))
        .collect::<Vec<_>>();
    assert_eq!(accessors.len(),2);
    vm.eval("original-config-accessors",&accessors.join("\n")).unwrap();
    vm.eval("original-sysload-cancel",r#"
        Marker <- 0
        Thread <- newthread(function(){try{local handler=com["sysload"];handler([]);Marker=1}catch(error){if(!SystemLoadHandleExit(error))throw error}})
        Thread.call();if(SystemCommandHost.delay!=0)throw "initial page wait"
        Event=ID_OK;Thread.wakeup()
        if(Dialogs!=1 || Marker!=0 || SystemCommandHost.delay!=-1)throw "nested confirmation not suspended"
        Thread.wakeup()
        if(Marker!=0 || Thread.getstatus()!="suspended")throw "page not resumed after confirmation cancel"
        Thread.wakeup()
        if(Draws<=0 || Marker!=0)throw "page not redrawn after confirmation cancel"
        if(SystemCommandHost.actions.len()!=0)throw "cancel unexpectedly loaded"
        Event=ID_CANCEL;Thread.wakeup()
        if(Marker!=1 || Thread.getstatus()!="idle")throw "page cancel not returned"
    "#).unwrap();
}

#[test]
#[ignore = "requires local user-owned TD1..4/ED/Sorakoi technical SaveInfo fixtures"]
fn original_saveinfo_writers_determine_all_six_metadata_streams() {
    let sources=[
        ("D:/codex/电脑移植安卓/research/inspection/pc-utf8/TD1/saveload.nut",4,"128,72"),
        ("D:/codex/电脑移植安卓/research/inspection/pc-utf8/TD2/saveload.nut",4,"128,72"),
        ("D:/codex/电脑移植安卓/research/inspection/pc-utf8/TD3/saveload.nut",4,"128,72"),
        ("D:/codex/电脑移植安卓/research/inspection/pc-utf8/TD4/saveload.nut",4,"128,72"),
        ("D:/codex/neko/.local/system-command-coverage/ed/saveload.nut",6,"128,72"),
        ("D:/codex/neko/.local/system-command-coverage/sorakoi/saveload.nut",8,"206,116"),
    ];
    for (path,count,dimensions) in sources {
        let mut vm=vm();
        vm.eval("projection-host",r#"
            com <- CommandRegistry()
            MessageLog <- [{text="live"}];ClimaxCount <- 11;SkipBackData <- [{pos=5,data="live"}]
            class SystemCommandSprite {
                width=0;height=0;source=null
                constructor(w=0,h=0){width=w;height=h}
                function LoadFile(f){source=SystemLoadThumbnail(f)}
            }
            Sprite <- SystemCommandSprite
            function Save(file,value){throw "real write must not run"}
            SystemLoadInstall([])
        "#).unwrap();
        vm.eval(path,&std::fs::read_to_string(path).unwrap()).unwrap();
        vm.eval("dimensions","return SystemLoadDimensions()").unwrap();assert_eq!(vm.result(),dimensions,"{path}");
        vm.eval("original-projection",r#"
            BeforeLog <- MessageLog;BeforeBack <- SkipBackData;BeforeSave <- Save;BeforeSprite <- Sprite
            local metadata={fields={sdate="2026/11/13",stime="22:04",stamp="2026/11/13 22:04",route="route",title="title",text="saved"},
                globals={MessageLog=[{size=22,name="name",voice="voice",face="FA",mask="role",text="saved"}],ClimaxCount=3,SkipBackData=[{pos=2,data="NKSC:actual"}]}}
            SystemLoadInstall([{path=SavePath("save002.sav"),modified="0001",state_id="2",thumbnail={marker=9},metadata=metadata}])
            check(MessageLog==BeforeLog && SkipBackData==BeforeBack && ClimaxCount==11 && Save==BeforeSave && Sprite==BeforeSprite)
            local info=SaveInfo();info.Get(file(SaveName(2),"rb"))
            check(info.route=="route" && info.title=="title" && info.thumb.source.marker==9)
            if("stamp" in info)check(info.stamp=="2026/11/13 22:04")
            if("sdate" in info)check(info.sdate=="2026/11/13" && info.stime=="22:04" && info.text=="saved")
            check(MessageLog==BeforeLog && SkipBackData==BeforeBack && ClimaxCount==11)
            local f=file(SaveName(2),"rb");info.Load(f)
            check(f.at==f.record.values.len())
            check(MessageLog[0].text=="saved" && MessageLog[0].face=="FA" && MessageLog[0].mask=="role")
            return f.at.tostring()
        "#).unwrap();
        assert_eq!(vm.result(),count.to_string(),"{path}");
    }
}
