//! Executes user-owned system/saveload scripts. The recording file/renderer are
//! test doubles: this verifies label ordering and gating, not original save bytes.
use nekopack_script::system::SystemVm;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let paths = std::env::args().skip(1).collect::<Vec<_>>();
    if paths.len() != 2 {
        return Err("usage: route_semantics system.nut saveload.nut".into());
    }
    let mut vm = SystemVm::try_new()?;
    vm.eval("recording-host", r#"
        class Registry {
            entries=null
            constructor() { entries={} }
            function _set(k,v) { entries.rawset(k,v) }
            function _get(k) { return entries[k] }
        }
        com <- Registry()
        Caption <- ""
        MessageLog <- ["line"]
        Saved <- null
        SaveCalls <- 0; LoadCalls <- 0; Sounds <- 0; LoadSucceeds <- false;
        function title(s) { Caption=s }
        function ini(n) { return {} }
        function data(n) { return [] }
        function date() { return {year=2026,month=10,day=7,hour=12,min=34} }
        class Sprite {
            constructor(w=0,h=0) {}
            function Snap() {}
            function Reduction(s) {}
            function SaveFile(f) { f.values.append("thumbnail") }
            function LoadFile(f) { if(f.values[f.at++]!="thumbnail")throw "thumbnail order" }
        }
        function file(name,mode) {
            if(mode=="wb")Saved={values=[],at=0};
            else Saved.at=0;
            return Saved;
        }
        function Save(f,v) { f.values.append(v) }
        function Load(f) { return f.values[f.at++] }
        function SaveState(f) { SaveCalls++; f.values.append("native-state"); return true }
        function LoadState(f) { LoadCalls++; if(f.values[f.at++]!="native-state")throw "native order"; return LoadSucceeds }
        function SavePath(n) { return n }
        function sound(n) { Sounds++ }
        function Sleep(n) { if(n!=1000)throw "sleep" }
        function check(v) { if(!v)throw "route semantics assertion" }
    "#).map_err(|e| format!("recording host: {e}"))?;
    for (name, path) in ["original-system", "original-saveload"].iter().zip(&paths) {
        vm.eval(name, &std::fs::read_to_string(path)?)
            .map_err(|e| format!("{name}: {e}"))?;
    }
    vm.eval("route-save-contract", r#"
        MessageLog=["line"];
        check(StoryRoute=="" && !InRoute());
        NormalSave(1); QuickSave(); check(SaveCalls==0 && Sounds==0);
        com.route.call(getroottable(),["第１章", "ignored"]); check(StoryRoute=="第１章" && InRoute());
        com.title.call(getroottable(),["２４日目"]); check(Caption.find("第１章 ２４日目")!=null);
        local caption=Caption;
        NormalSave(1); check(SaveCalls==1 && Sounds==1);
        check(Saved.values.len()==6 && Saved.values[0]=="thumbnail");
        check(Saved.values[1]=="第１章" && Saved.values[3]=="２４日目");
        check(Saved.values[2]=="2026/11/07 12:34");
        MessageLog=["unchanged"]; Saved.at=0;
        local info=SaveInfo(); info.Get(Saved);
        check(info.route=="第１章" && info.title=="２４日目");
        check(Saved.at==4 && MessageLog[0]=="unchanged");
        check(Saved.values[4][0]=="line" && Saved.values[5]=="native-state");
        com.route.call(getroottable(),[]); check(StoryRoute=="" && !InRoute() && StoryTitle=="２４日目");
        com.title.call(getroottable(),[]); MessageLog=[];
        LoadData("save"); check(LoadCalls==1 && StoryRoute=="第１章" && StoryTitle=="２４日目");
        check(InRoute() && Caption==caption && MessageLog[0]=="line");
        LoadSucceeds=true; local interrupted=false;
        try { LoadData("save"); } catch(e) { check(e==null); interrupted=true; }
        check(interrupted && LoadCalls==2);
        ResetRoute(); check(!InRoute() && StoryTitle=="２４日目" && Caption==caption);
        com.route.call(getroottable(),["　"]); check(InRoute() && StoryRoute=="　");
        com.route.call(getroottable(),[]); com.title.call(getroottable(),[]); check(!InRoute() && StoryTitle=="");
        return "PASS original route, title, reset, save gating, metadata order, load restore and unwind";
    "#).map_err(|e| format!("contract: {e}"))?;
    println!("{}", vm.result());
    Ok(())
}
