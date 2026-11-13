//! Single-coroutine GUI probes. Only synthetic metadata/draw plans are used.
use nekopack_script::{runtime::Variables,system::{ResourceProvider,SystemVm}};

struct Resources {calls:usize}
impl ResourceProvider for Resources {
    fn resource(&mut self,kind:&str,name:&str)->Result<String,String> {
        assert_eq!((kind,name),("Assets","dialog"));self.calls+=1;
        Ok(r#"{
            BASE=["system/synthetic.mng:0",0,0,100,60],
            CONFIRM=["system/synthetic.mng:1",0,0,100,60],
            ["= OK"]=["system/synthetic.mng:2",10,20,30,20],
            ["+ OK"]=["system/synthetic.mng:3",10,20,30,20],
            ["* OK"]=["system/synthetic.mng:4",10,20,30,20],
            ["= CANCEL"]=["system/synthetic.mng:5",60,20,30,20],
            ["- OK"]=["system/synthetic.mng:6",10,20,30,20]
        }"#.into())
    }
}

fn host()->SystemVm {
    let mut vm=SystemVm::new();
    vm.eval("headless",include_str!("../../nekopack-script/host/headless.nut")).unwrap();
    vm.eval("command",include_str!("../host/system_command.nut")).unwrap();
    vm.eval("gui",include_str!("../host/system_gui.nut")).unwrap();
    vm.eval("fixture",r#"
        Config <- {Dialog={CONFIRM=true},AutoSelect=false,Effect=1}
        function NativeAudioConfigSnapshot(){return "synthetic audio metadata"}
        SavedSpots <- Spots
        SavedSprite <- Sprite
    "#).unwrap();
    vm
}

fn step(vm:&mut SystemVm,resources:&mut Resources,event:i32,pointer:Option<[i32;2]>)->String {
    let point=pointer.map_or("null".into(),|[x,y]|format!("[{x},{y}]"));
    vm.eval_with_resources("step",&format!("return SystemCommandStep({event},false,0,{point})"),
        &mut Variables::default(),resources).unwrap();
    vm.result()
}

#[test]
fn nested_dialog_keeps_wait_boundaries_buttons_and_caller_return() {
    for (event,point,expected) in [
        (2_000_000_001,Some([15,25]),true),
        (2_000_000_001,Some([65,25]),false),
        (2_000_000_002,None,false),
    ] {
        let mut vm=host();let mut resources=Resources {calls:0};
        vm.eval("dialog",include_str!("fixtures/system_gui_dialog.nut")).unwrap();
        vm.eval("install","SystemCommandInstall(\"dialog_probe\",[\"CONFIRM\"],null)").unwrap();
        assert!(step(&mut vm,&mut resources,0,None).contains("W\t0\n"));
        let frame=step(&mut vm,&mut resources,0,Some([0,0]));
        assert!(frame.contains("B\t0:OK:2000000001\t10\t20\t30\t20\n"),"{frame}");
        assert!(frame.contains("B\t0:CANCEL:2000000002\t60\t20\t30\t20\n"),"{frame}");
        let closing=step(&mut vm,&mut resources,event,point);
        assert!(closing.contains("W\t0\n"));assert!(!closing.contains("R\t"));
        vm.eval("before return","return AfterDialog.tostring()").unwrap();assert_eq!(vm.result(),"0");
        assert!(step(&mut vm,&mut resources,0,None).contains("R\tnull\n"));
        vm.eval("result","return format(\"%s %d %s\",DialogResult.tostring(),AfterDialog,(Spots==SavedSpots && Sprite==SavedSprite).tostring())").unwrap();
        assert_eq!(vm.result(),format!("{expected} 1 true"));
        assert_eq!(resources.calls,1);
    }
}

#[test]
fn dialog_disabled_gate_and_focus_keep_original_branches() {
    let mut vm=host();let mut resources=Resources {calls:0};
    vm.eval("dialog",include_str!("fixtures/system_gui_dialog.nut")).unwrap();
    vm.eval("disabled","Config.Dialog.CONFIRM=false;SystemCommandInstall(\"dialog_probe\",[\"CONFIRM\"],null)").unwrap();
    assert!(step(&mut vm,&mut resources,0,None).contains("R\tnull\n"));
    vm.eval("gate result","return DialogResult.tostring()").unwrap();assert_eq!(vm.result(),"true");
    assert_eq!(resources.calls,0);

    let mut vm=host();let mut resources=Resources {calls:0};
    vm.eval("dialog",include_str!("fixtures/system_gui_dialog.nut")).unwrap();
    vm.eval("focus","Config.AutoSelect=true;SystemCommandInstall(\"dialog_probe\",[\"CONFIRM\"],null)").unwrap();
    step(&mut vm,&mut resources,0,None);
    assert!(step(&mut vm,&mut resources,2_000_000_001,None).contains("W\t0\n"));
    step(&mut vm,&mut resources,0,None);
    vm.eval("focused result","return DialogResult.tostring()").unwrap();assert_eq!(vm.result(),"true");
}

#[test]
fn gui_geometry_visibility_position_and_glyph_plan_are_explicit() {
    let mut vm=host();let mut resources=Resources {calls:0};
    vm.eval_with_resources("geometry",r#"
        SystemCommandHost.x=30;SystemCommandHost.y=37
        local images=SystemCommandImages("dialog")
        local spots=SystemGuiSpots(images)
        spots.BindR("ok",7);spots.BindR("cancel",8)
        if(spots.Check(5,7)!=7 || spots.GetPosH(100)!=50 || spots.GetPosV(100)!=50)throw "offset geometry"
        spots.Enable(7,false);if(spots.Check(5,7)!=0)throw "disabled hit"
        spots.Enable(7);spots.Visible(7,false);if(spots.Check(5,7)!=0)throw "hidden hit"
        spots.VisibleAll(true);spots.Active(7)
        SystemCommandHost.buttons=[]
        local target=SystemCommandSprite()
        spots.Draw(target,5,7)
        local g=SystemGuiGlyph();g.Bold=true;g.Print(target,"probe",9,11,0)
        local t=target.layers[target.layers.len()-1]
        return format("%d %d %d %s %d %d %d %d",SystemCommandHost.buttons.len(),
            SystemCommandHost.buttons[0][3],SystemCommandHost.buttons[0][4],t[0],t[2],t[3],t[4],t[5])
    "#,&mut Variables::default(),&mut resources).unwrap();
    assert_eq!(vm.result(),"2 15 27 T 9 11 1 1");
    vm.eval("styled glyph",r#"local target=SystemCommandSprite(),g=SystemGuiGlyph();g.Size=30;g.Color=0x4466ff;g.Print(target,"probe",0,0);return SystemCommandLines(target.layers)"#).unwrap();
    assert_eq!(vm.result(),"G\t0\t0\t30\t4482815\t0\t-1\t70726f6265\n");
    assert!(vm.eval("invalid glyph","local g=SystemGuiGlyph();g.Size=0;g.Print(SystemCommandSprite(),\"probe\",0,0)")
        .unwrap_err().contains("invalid system GUI Glyph size"));
}

#[test]
fn side_menu_updates_spots_retained_after_system_gui_bindings_restore() {
    let mut vm=host();let mut resources=Resources {calls:0};
    vm.eval("side menu",include_str!("../host/side_menu.nut")).unwrap();
    vm.eval("shortcut fixture",r#"
        function QuickCheck(){return false}
        function InRoute(){return false}
        function HasLastVoice(){return false}
        function IsEnableSkipNext(){return false}
        function IsEnableSkipBack(){return false}
        SHORTCUTS <- {QLOAD=11,SKIP=12}
        class SyntheticShortCut {
            m_spots=null
            function Init(){
                m_spots=Spots(Images("dialog"))
                m_spots.BindR("ok",11);m_spots.BindR("cancel",12)
                Update()
            }
            function Update(){
                m_spots.Enable(11,InRoute() && QuickCheck())
                m_spots.Enable(12,!InAuto())
                m_spots.Active(12,InSkip())
            }
        }
        ShortCut <- SyntheticShortCut()
        com["shortcut_probe"] = function(args){ShortCut.Init();wait(0)}
        SystemCommandInstall("shortcut_probe",[],null)
    "#).unwrap();
    assert!(step(&mut vm,&mut resources,0,None).contains("W\t0\n"));
    assert!(step(&mut vm,&mut resources,0,None).contains("R\tnull\n"));
    vm.eval("retained spots",r#"
        return (Spots==SavedSpots && ShortCut.m_spots instanceof SystemGuiSpots).tostring()
    "#).unwrap();
    assert_eq!(vm.result(),"true");
    vm.eval("disabled quick load",r#"
        return SideMenuPlan(false,true,false,false,false,false,false)
    "#).unwrap();
    let disabled=vm.result();
    assert!(disabled.contains("B\tQLOAD\t0\t2\t3\t-1\t6\t4\n"),"{disabled}");
    vm.eval("enabled quick load",r#"
        return SideMenuPlan(true,true,false,true,false,false,false)
    "#).unwrap();
    let enabled=vm.result();
    assert!(enabled.contains("B\tQLOAD\t1\t2\t3\t-1\t6\t4\n"),"{enabled}");
    vm.eval("updated public state",r#"
        return (ShortCut.m_spots.items.OK.enabled && ShortCut.m_spots.items.CANCEL.selected).tostring()
    "#).unwrap();
    assert_eq!(vm.result(),"true");
    vm.eval("auto disables skip",r#"
        return SideMenuPlan(true,true,false,false,true,false,false)
    "#).unwrap();
    assert!(vm.result().contains("B\tSKIP\t0\t5\t-1\t-1\t-1\t5\n"));
    assert_eq!(resources.calls,1);
}

#[test]
fn switch_mode_is_idempotent_and_uses_active_or_normal_under_pointer() {
    let mut vm=host();
    vm.eval("switch mode",r#"
        local images={}
        foreach(prefix,ordinal in { ["="]=1,["+"]=2,["@"]=3 }){
            local image=SystemCommandSprite()
            image.asset="system/synthetic.mng:"+ordinal
            image.avail=true;image.x=10;image.y=20;image.width=30;image.height=20
            images.rawset(prefix+" CHOICE",image)
        }
        local spots=SystemGuiSpots(images)
        spots.Bind("CHOICE",7)
        SystemCommandHost.x=15;SystemCommandHost.y=25
        spots.Active(7,true)
        if(spots.Check()!=0)throw "default mode selected hit"
        spots.SwitchMode(true);spots.SwitchMode(true)
        if(!spots.CHOICE.selected || spots.Check()!=7)throw "switch mode must remain enabled"
        local active=SystemCommandSprite()
        spots.Draw(active)
        // The original checkbox controller owns the toggle after a hit.
        spots.Active(7,!spots.CHOICE.selected)
        if(spots.CHOICE.selected || spots.Check()!=7)throw "selected choice cannot be cancelled"
        local normal=SystemCommandSprite()
        spots.Draw(normal)
        spots.SwitchMode(false);spots.Active(7,true)
        if(spots.Check()!=0)throw "default mode was not restored"
        return SystemCommandLines(active.layers)+SystemCommandLines(normal.layers)
    "#).unwrap();
    assert_eq!(vm.result(),"A\tsystem/synthetic.mng:3\t10\t20\t255\nA\tsystem/synthetic.mng:1\t10\t20\t255\n");
}

#[test]
fn overlapping_spots_keep_first_bound_hit_instead_of_smallest_area() {
    let mut vm=host();
    vm.eval("overlap order",r#"
        local images={}
        foreach(name,size in {FIRST=100,SECOND=20}){
            local image=SystemCommandSprite()
            image.avail=true;image.width=size;image.height=size
            images.rawset("= "+name,image)
        }
        local spots=SystemGuiSpots(images)
        spots.Bind("FIRST",7);spots.Bind("SECOND",8)
        SystemCommandHost.x=10;SystemCommandHost.y=10
        local first=spots.Check()
        spots.Enable(7,false)
        return format("%d %d",first,spots.Check())
    "#).unwrap();
    assert_eq!(vm.result(),"7 8");
}

#[test]
fn screen_and_sprite_snap_keep_same_coroutine_snapshot_immutable() {
    let mut vm=host();let mut resources=Resources {calls:0};
    vm.eval("snapshot fixture",r#"
        com["snapshot_probe"] = function(args){
            local screen=Screen()
            screen.Fill(17)
            screen.Snap()
            local changed=Sprite();changed.Snap();changed.Fill(51)
            screen.Fill(34)
            local captured=Sprite();captured.Snap()
            screen.Draw(captured);screen.Update();wait(0)
            screen.Fill(68)
            local retained=Sprite();retained.Snap()
            screen.Draw(retained);screen.Update();wait(0)
        }
        SystemCommandInstall("snapshot_probe",[],null)
    "#).unwrap();
    let first=step(&mut vm,&mut resources,0,None);
    assert_eq!(first,"N\t1\t4f093137093235360a\nW\t0\nP\t1\t0\t0\nO\t34\t256\nO\t17\t256\n");
    let second=step(&mut vm,&mut resources,0,None);
    assert_eq!(second,"W\t0\nP\t1\t0\t0\nO\t68\t256\nO\t17\t256\n");
    assert_eq!(step(&mut vm,&mut resources,0,None),"R\tnull\n");
    assert_eq!(resources.calls,0);
}

#[test]
fn native_enable_property_and_spots_enable_share_one_state() {
    let mut vm=host();
    vm.eval("native enable alias",r#"
        local spots=SystemGuiSpots(),spot=spots.Find("fixture")
        spot.id=10
        spots.Enable(10,false)
        if(spot.is_enable)throw "property differs from native Enable"
        spot.is_enable=true
        if(!spot.enabled)throw "native property did not enable spot"
    "#).unwrap();
}

#[test]
fn protocol_hex_uses_unsigned_utf8_bytes() {
    let mut vm=host();
    vm.eval("unicode protocol", "return SystemCommandHex(\"音乐\")").unwrap();
    assert_eq!(vm.result(),"e99fb3e4b990");
}
