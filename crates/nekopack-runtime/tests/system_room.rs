//! Room native boundaries execute inside the caller's SystemCommand thread.
use nekopack_script::{runtime::Variables,system::{ResourceProvider,SystemVm}};

struct NoResources;
impl ResourceProvider for NoResources {
    fn resource(&mut self,kind:&str,name:&str)->Result<String,String>{
        panic!("native room probe unexpectedly read {kind}:{name}")
    }
}
fn host()->SystemVm {
    let mut vm=SystemVm::new();
    vm.eval("headless",include_str!("../../nekopack-script/host/headless.nut")).unwrap();
    vm.eval("command",include_str!("../host/system_command.nut")).unwrap();
    vm.eval("gui",include_str!("../host/system_gui.nut")).unwrap();
    vm.eval("room",include_str!("../host/system_room.nut")).unwrap();
    vm.eval("fixture",r#"
        Config <- {VolBGM=75,VolVO=100,VolSE=100,VolENV=100,VolSYS=100,Effect=1}
        function NativeAudioConfigSnapshot(){return "synthetic audio metadata"}
        BaseGuiBindings <- SystemGuiBindings
        function SystemGuiBindings(){
            local bindings=BaseGuiBindings()
            foreach(key,value in SystemRoomBindings())bindings.rawset(key,value)
            return bindings
        }
        OriginalPlayMusic <- "untouched original binding"
        PlayMusic <- OriginalPlayMusic
    "#).unwrap();
    vm
}
fn step(vm:&mut SystemVm,event:i32)->String{
    vm.eval_with_resources("step",&format!("return SystemCommandStep({event},false,50)"),
        &mut Variables::default(),&mut NoResources).unwrap();
    vm.result()
}

#[test]
fn nested_room_audio_yields_and_returns_on_the_callers_thread(){
    let mut vm=host();
    vm.eval("nested original contract",r#"
        Returned <- 0
        function NativeRoomLoop(){
            StopMusic()
            while(wait(50)){
                local ev=event()
                if(ev==ID_CANCEL)break
                if(ev==ID_OK){PlayMusic("bgm_fixture",false);VolumeMusic(65)}
            }
            StopMusic()
        }
        com["room_probe"] = function(args){
            local volume=Config.VolBGM
            NativeRoomLoop()
            Config.VolBGM=volume
            RestoreVolume()
            PlayMusic("title_fixture",true)
            Returned++
        }
        SystemCommandInstall("room_probe",[],null)
    "#).unwrap();
    let opened=step(&mut vm,0);
    assert!(opened.contains("U\troom_music_stop\n"));
    assert!(opened.contains("W\t50\n"));
    let played=step(&mut vm,2_000_000_001);
    assert!(played.contains("U\troom_music_play\t62676d5f66697874757265\t0\t100\n"),"{played}");
    assert!(played.contains("U\troom_music_volume\t65\n"));
    vm.eval("same thread", "return (SystemRoomHost.music_playing && Returned==0 && PlayMusic==OriginalPlayMusic).tostring()").unwrap();
    assert_eq!(vm.result(),"true");
    let closed=step(&mut vm,2_000_000_002);
    assert!(closed.contains("U\troom_music_stop\nU\trestore_audio\nU\troom_music_play\t7469746c655f66697874757265\t1\t100\n"),"{closed}");
    assert!(closed.contains("R\tnull\n"));
    vm.eval("returned", "return Returned.tostring()").unwrap();
    assert_eq!(vm.result(),"1");
    vm.eval("external completion", "SystemRoomMusicState(false);return SystemRoomIsPlayMusic().tostring()").unwrap();
    assert_eq!(vm.result(),"false");
}

#[test]
fn replay_boundary_records_metadata_and_keeps_original_stack_suspended(){
    let mut vm=host();
    vm.eval("replay fixture",r#"
        Returned <- false
        com["metadata_probe"] = function(args){
            ImageAction("0 #fixture&1")
            ScriptReset()
            AssignImage("")
            ScriptEval("replay_fixture")
            Returned=true
        }
        SystemCommandInstall("metadata_probe",[],null)
    "#).unwrap();
    let image=step(&mut vm,0);
    assert!(image.contains("U\troom_image\t302023666978747572652631\n"),"{image}");
    assert!(image.contains("H\tnative\n"));
    let output=step(&mut vm,2_000_000_006);
    assert!(output.contains("U\troom_script_reset\nU\troom_assign_image\t\n"));
    assert!(output.contains("U\troom_replay\t7265706c61795f66697874757265\nW\t-1\n"));
    vm.eval("suspended", "return Returned.tostring()").unwrap();
    assert_eq!(vm.result(),"false");
    // Even a premature host ACK cannot run the original post-replay branch.
    let still_pending=step(&mut vm,2_000_000_006);
    assert!(still_pending.contains("W\t-1\n"));
    vm.eval("still suspended", "return Returned.tostring()").unwrap();
    assert_eq!(vm.result(),"false");
    vm.eval("explicit replay completion", "SystemRoomReplayComplete()").unwrap();
    assert!(step(&mut vm,2_000_000_006).contains("R\tnull\n"));
    vm.eval("returned after replay", "return Returned.tostring()").unwrap();
    assert_eq!(vm.result(),"true");
}

#[test]
fn cg_commit_waits_for_scene_refresh_before_rendering(){
    let mut vm=host();
    vm.eval("cg fixture",r#"
        com["cg_probe"] = function(args){
            ImageAction("1 white")
            local screen=Screen()
            screen.Commit()
            ImageAction("0 #fixture&1")
            screen.Render();screen.Update();wait(-1)
        }
        SystemCommandInstall("cg_probe",[],null)
    "#).unwrap();
    let image=step(&mut vm,0);
    assert!(image.contains("U\troom_image\t31207768697465\n"));
    assert!(!image.contains("P\t"));
    let commit=step(&mut vm,2_000_000_006);
    assert!(commit.contains("U\troom_commit\n"));
    assert!(!commit.contains("P\t"));
    let visual=step(&mut vm,2_000_000_006);
    assert!(visual.contains("U\troom_image\t302023666978747572652631\n"));
    let rendered=step(&mut vm,2_000_000_006);
    assert!(rendered.contains("P\t1\t0\t-1\nC\n"),"{rendered}");
}

#[test]
fn room_glyph_keeps_original_caption_styles_and_audio_inputs_are_validated(){
    let mut vm=host();
    vm.eval("utf8 bytes", "return SystemCommandHex(\"音楽\")").unwrap();
    assert_eq!(vm.result(),"e99fb3e6a5bd");
    vm.eval("room glyph",r#"
        local target=SystemCommandSprite(),g=SystemRoomGlyph()
        g.Bold=true;g.Size=28;g.Color=0x4466ff;g.Print(target,"fixture",3,4)
        local p=target.layers[0]
        return format("%s %s %d %d %d %d %d %d",p[0],p[1],p[2],p[3],p[4],p[5],p[6],p[7])
    "#).unwrap();
    assert_eq!(vm.result(),"G fixture 3 4 28 4482815 1 -1");
    assert!(vm.eval("invalid path", "SystemRoomPlayMusic(\"../fixture\")").unwrap_err().contains("invalid original room music name"));
    assert!(vm.eval("invalid volume", "SystemRoomVolumeMusic(101)").unwrap_err().contains("invalid original room music volume"));
}
