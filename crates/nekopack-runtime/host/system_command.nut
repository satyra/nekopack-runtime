// Coroutine bridge for mounted system handlers. No game-specific dispatch.
SystemCommandHost <- {
    thread=null,name=null,args=null,rows=null,started=false,result=null,failure=null,terminal_action=null,
    save_pending=false,host_pending=false,x=-1,y=-1,buttons=[],frame_buttons=[],frame=null,previous=[],delay=0,event=0,skip=false,actions=[],sounds=[],
    current=[["Q"]],snapshot=[["Z",0]],snapshots=[],screen=null,scene_cleared=false,
    canvas=[0,0],opaque_assets={},surface_leases={},
    trans_begin=null,trans_end=null,trans_duration=0,trans_start=0,
    trans_mode=1,trans_mask_id=0,trans_loads=[]
}
class SystemCommandConfigFile {}
class SystemCommandSurface {
    id=0
    constructor(value){id=value}
}
function SystemCommandLiveSurfaces(){
    local live="",dead=[]
    // Squirrel 2 dereferences weak table values on lookup/foreach. A dead
    // lease is null; enumerating it never makes the registry a strong owner.
    foreach(id,lease in SystemCommandHost.surface_leases){
        if(lease==null)dead.append(id)
        else live+=(live.len()?",":"")+id
    }
    foreach(id in dead)SystemCommandHost.surface_leases.rawdelete(id)
    return live
}
function SystemCommandRasterized(id){
    local host=SystemCommandHost
    // Update can precede further Draw calls in the same script step. Only the
    // exact plan whose pixels native rendered may replace the current plan.
    if(host.frame==null || SystemCommandLines(host.current)!=SystemCommandLines(host.frame))
        return "0\t"+SystemCommandLiveSurfaces()
    local lease=SystemCommandSurface(id)
    host.surface_leases.rawset(id,lease.weakref())
    host.current=[["V",lease]]
    host.frame=clone host.current;host.previous=clone host.current
    if(host.screen!=null)host.screen.layers=host.current
    return "1\t"+SystemCommandLiveSurfaces()
}
function SystemCommandFile(name,mode){
    if(name!=SavePath("config.sav") || mode!="wb")throw "unsupported original system file operation"
    return SystemCommandConfigFile()
}
function SystemCommandSave(file,value){
    if(!(file instanceof SystemCommandConfigFile) || value!=Config)throw "unsupported original system serialization"
    SystemCommandHost.actions.append("save_config")
    SystemCommandHost.save_pending=true;SystemCommandHost.delay=-1
    suspend()
    SystemCommandHost.save_pending=false
}
function SystemCommandRestoreVolume(){
    Host.volumes=[Config.VolBGM,Config.VolVO,Config.VolSE,Config.VolENV,Config.VolSYS]
    SystemCommandHost.actions.append("restore_audio")
}
function SystemCommandRestoreScreen(){
    Host.screen=Config.Screen
    SystemCommandHost.actions.append("restore_screen")
}
function SystemCommandHex(value){
    local out=""
    for(local i=0;i<value.len();++i)out+=format("%02x",value[i]&255)
    return out
}
class SystemCommandSprite {
    x=0;y=0;width=0;height=0;avail=false;asset=null;layers=null
    constructor(source=null,height=null){
        layers=[]
        if(height!=null){
            width=source;this.height=height;avail=width>0 && height>0
        }else if(source!=null){
            if(("SystemLoadFileHandle" in getroottable()) && source instanceof ::SystemLoadFileHandle)source=::SystemLoadThumbnail(source)
            x=source.x;y=source.y;width=source.width;this.height=source.height
            avail=source.avail;asset=source.asset;layers=clone source.layers
        }
    }
    function LoadFile(file){
        local source=::SystemLoadThumbnail(file)
        x=source.x;y=source.y;width=source.width;height=source.height
        avail=source.avail;asset=source.asset;layers=clone source.layers
    }
    function PixelLayers(){
        local result=[]
        if(asset!=null)result.append(["A",asset,0,0,255])
        foreach(layer in layers)result.append(clone layer)
        return result
    }
    function Alpha(mask){
        if(!avail || !mask.avail)throw "Alpha requires allocated sprites"
        local source=PixelLayers(),mask_layers=mask.PixelLayers()
        layers=[["D",width,height,"alpha",width,height,source,mask_layers]];asset=null
    }
    function Reduction(source){
        if(!avail || (!source.avail && source.layers.len()==0))throw "Reduction requires allocated sprites"
        layers=[["D",width,height,"reduction",source.width,source.height,source.PixelLayers(),[]]];asset=null
    }
    function Clear(){asset=null;layers=[]}
    function Base(){
        if(::SystemCommandHost.scene_cleared)throw "Sprite.Base after ClearImage requires a refreshed scene snapshot"
        asset=null;layers=[["C"]]
    }
    function Screen(){asset=null;layers=clone ::SystemCommandHost.current}
    function Snap(){
        if(::SystemCommandHost.snapshot==null)throw "original snapshot has not been captured"
        asset=null;layers=clone ::SystemCommandHost.snapshot
    }
    function Fill(color,opacity=256){
        if(opacity<=0)return
        if(opacity>=255){layers=[];asset=null}
        layers.append(["O",color,opacity])
    }
    function Draw(source,dx=0,dy=0,width_or_opacity=255,height=null,opacity=255){
        if(height!=null){
            if(source.asset!=null)layers.append(["K",source.asset,dx,dy,width_or_opacity,height,opacity])
            else if(source.layers.len())throw "unsupported cropped composite sprite"
            return
        }
        opacity=width_or_opacity
        if(source.layers.len() && source.width>0 && source.height>0){
            local child=[]
            if(source.asset!=null)child.append(["A",source.asset,0,0,255])
            foreach(layer in source.layers)child.append(clone layer)
            layers.append(["J",source.width,source.height,source.x+dx,source.y+dy,opacity,child])
            return
        }
        if(source.asset!=null)layers.append(["A",source.asset,source.x+dx,source.y+dy,opacity])
        foreach(item in source.layers){
            local copy=clone item
            if(copy[0]=="C" || copy[0]=="Q" || copy[0]=="Z" || copy[0]=="V")layers=[]
            if(copy[0]=="K"){copy[2]+=source.x+dx;copy[3]+=source.y+dy;copy[6]=copy[6]*opacity/255}
            else if(copy[0]=="A"){copy[2]+=source.x+dx;copy[3]+=source.y+dy;copy[4]=copy[4]*opacity/255}
            else if(copy[0]=="J"){copy[3]+=source.x+dx;copy[4]+=source.y+dy;copy[5]=copy[5]*opacity/255}
            else if(copy[0]=="G"){copy[2]+=source.x+dx;copy[3]+=source.y+dy;if(opacity!=255)throw "unsupported glyph opacity"}
            else if(copy[0]=="T"){copy[2]+=source.x+dx;copy[3]+=source.y+dy;if(opacity!=255)throw "unsupported glyph opacity"}
            else if(source.x+dx!=0 || source.y+dy!=0 || opacity!=255)throw "unsupported transformed system command composite"
            layers.append(copy)
        }
    }
}
class SystemCommandScreen extends SystemCommandSprite {
    constructor(){
        ::SystemCommandSprite.constructor.call(this)
        layers=clone ::SystemCommandHost.current
        ::SystemCommandHost.screen=this
    }
    function Fill(color,opacity=256){layers=::SystemCommandHost.current;::SystemCommandSprite.Fill.call(this,color,opacity);::SystemCommandHost.current=layers}
    function Draw(source,dx=0,dy=0,width_or_opacity=255,height=null,opacity=255){
        local host=::SystemCommandHost
        // Only native-verified opaque pixels may hide retained drawing history.
        // Replace the array so prior Screen/Snap captures still own their plan.
        if(height==null && width_or_opacity==255 && source.asset!=null && source.layers.len()==0 &&
           host.opaque_assets.rawin(source.asset) && host.canvas[0]>0 && host.canvas[1]>0){
            local size=host.opaque_assets[source.asset],x=source.x+dx,y=source.y+dy
            if(x<=0 && y<=0 && x+size[0]>=host.canvas[0] && y+size[1]>=host.canvas[1])host.current=[]
        }
        layers=host.current
        ::SystemCommandSprite.Draw.call(this,source,dx,dy,width_or_opacity,height,opacity)
        host.current=layers
    }
    function Snap(){
        ::SystemCommandHost.snapshot=clone ::SystemCommandHost.current
        ::SystemCommandHost.snapshots=[[1,clone ::SystemCommandHost.snapshot]]
    }
    function Update(){
        layers=::SystemCommandHost.current
        ::SystemCommandHost.frame_buttons=::SystemCommandHost.buttons
        ::SystemCommandHost.buttons=[]
        ::SystemCommandHost.frame=clone layers
        ::SystemCommandHost.previous=clone layers
    }
}
function SystemLoadThumbnailSprite(id,width,height){
    local image=SystemCommandSprite(width,height)
    image.layers=[["Z",id]]
    return image
}
function SystemCommandImages(name=null,directory="system"){
    local result={}
    if(name!=null){
        local resource=directory=="system" ? name : directory+"/"+name
        local assets=compilestring("return "+::NekoResource("Assets",resource))()
        foreach(key,meta in assets){
            local sprite=::SystemCommandSprite()
            sprite.asset=meta[0];sprite.x=meta[1];sprite.y=meta[2]
            sprite.width=meta[3];sprite.height=meta[4];sprite.avail=true
            result.rawset(key,sprite)
        }
    }
    return ::NekoTableDelegate(result,{_get=function(key){return ::SystemCommandSprite()}})
}
function SystemCommandWait(delay){
    if(typeof delay!="integer" || delay< -1)throw "invalid system command wait"
    SystemCommandHost.delay=delay
    suspend()
    return true
}
function SystemCommandEvent(){return SystemCommandHost.event}
function SystemCommandInSkip(){return SystemCommandHost.skip}
function SystemCommandSound(name){SystemCommandHost.sounds.append([name,NativeAudioConfigSnapshot()])}
function SystemCommandClear(){SystemCommandHost.scene_cleared=true;SystemCommandHost.actions.append("clear_scene")}
function SystemCommandAuto(){SystemCommandHost.actions.append("toggle_auto")}
function SystemCommandCancelAuto(){Host.auto_mode=false;SystemCommandHost.actions.append("cancel_auto")}
function SystemCommandCancelSkip(){Host.skip=false;SystemCommandHost.skip=false;SystemCommandHost.actions.append("cancel_skip")}
function SystemCommandDelay(){SystemCommandHost.actions.append("enable_delay\t"+DelayTime)}
function SystemCommandTransBegin(){SystemCommandHost.trans_begin=clone SystemCommandHost.current}
function SystemCommandTransEnd(){SystemCommandHost.trans_end=clone SystemCommandHost.current}
function SystemCommandTransSet(duration,mask="",mode=""){
    if((typeof duration!="integer" && typeof duration!="float") || duration<0)throw "invalid original transition duration"
    if(typeof mask!="string" || typeof mode!="string")throw "invalid original transition mask parameters"
    duration=duration.tointeger()
    SystemCommandHost.trans_duration=Config.Effect==0 ? 0 : duration
    SystemCommandHost.trans_start=Host.clock
    SystemCommandHost.trans_mode=1
    if(mask.len()!=0){
        SystemCommandHost.trans_mode=mode.tolower()=="fade" ? 3 : 2
        SystemCommandHost.trans_mask_id++
        SystemCommandHost.trans_loads.append([SystemCommandHost.trans_mask_id,mask])
    }
}
function SystemCommandRenderTrans(alpha=null){
    if(SystemCommandHost.trans_begin==null || SystemCommandHost.trans_end==null)throw "original transition snapshots are not installed"
    if(alpha==null)alpha=SystemCommandHost.trans_duration==0 ? 256 : (Host.clock-SystemCommandHost.trans_start)*256/SystemCommandHost.trans_duration
    if(typeof alpha!="integer")throw "invalid original transition alpha"
    SystemCommandHost.current=SystemCommandHost.trans_mode==1 ?
        [["X",alpha,SystemCommandHost.trans_begin,SystemCommandHost.trans_end]] :
        [["Y",alpha,SystemCommandHost.trans_mode,SystemCommandHost.trans_mask_id,SystemCommandHost.trans_begin,SystemCommandHost.trans_end]]
    if(SystemCommandHost.screen!=null)SystemCommandHost.screen.layers=SystemCommandHost.current
}
function SystemCommandLines(layers){
    local out=""
    foreach(item in layers){
        if(item[0]=="C" || item[0]=="Q")out+=item[0]+"\n"
        else if(item[0]=="A")out+=format("A\t%s\t%d\t%d\t%d\n",item[1],item[2],item[3],SystemCommandAlpha(item[4]))
        else if(item[0]=="Z")out+=format("Z\t%d\n",item[1])
        else if(item[0]=="V")out+=format("Z\t%d\n",item[1].id)
        else if(item[0]=="K")out+=format("K\t%s\t%d\t%d\t%d\t%d\t%d\n",item[1],item[2],item[3],item[4],item[5],SystemCommandAlpha(item[6]))
        else if(item[0]=="T")out+=format("T\t%d\t%d\t%d\t%d\t%s\n",item[2],item[3],item[4],item[5],item[1])
        else if(item[0]=="D")out+=format("D\t%d\t%d\t%s\t%d\t%d\t%s\t%s\n",item[1],item[2],item[3],item[4],item[5],SystemCommandHex(SystemCommandLines(item[6])),SystemCommandHex(SystemCommandLines(item[7])))
        else if(item[0]=="G")out+=format("G\t%d\t%d\t%d\t%d\t%d\t%d\t%s\n",item[2],item[3],item[4],item[5],item[6],item[7],SystemCommandHex(item[1]))
        else if(item[0]=="J")out+=format("J\t%d\t%d\t%d\t%d\t%d\t%s\n",item[1],item[2],item[3],item[4],SystemCommandAlpha(item[5]),SystemCommandHex(SystemCommandLines(item[6])))
        else if(item[0]=="O")out+=format("O\t%d\t%d\n",item[1],item[2])
        else if(item[0]=="X")out+=format("X\t%d\t%s\t%s\n",item[1],SystemCommandHex(SystemCommandLines(item[2])),SystemCommandHex(SystemCommandLines(item[3])))
        else if(item[0]=="Y")out+=format("Y\t%d\t%d\t%d\t%s\t%s\n",item[1],item[2],item[3],SystemCommandHex(SystemCommandLines(item[4])),SystemCommandHex(SystemCommandLines(item[5])))
        else throw "unsupported system command drawing primitive"
    }
    return out
}
function SystemCommandInstall(name,args,rows){
    SystemCommandHost.name=name;SystemCommandHost.args=args;SystemCommandHost.rows=rows
    SystemCommandHost.thread=newthread(function(){
      try{
        seterrorhandler(function(error){
            if(error==null && SystemCommandHost.terminal_action=="quit")return
            SystemCommandHost.failure=error==null?"unexpected null in original system coroutine":error.tostring()
            for(local level=1;level<12;++level){
                local info=getstackinfos(level)
                if(info==null)break
                SystemCommandHost.failure+=format(" [%s:%d %s]",info.src,info.line,info.func)
            }
        })
        if("CheckNewestData" in getroottable())CheckNewestData()
        local handler=com[SystemCommandHost.name]
        if(SystemCommandHost.rows==null)SystemCommandHost.result=handler(SystemCommandHost.args)
        else {
            local value=handler(SystemCommandHost.args,SystemCommandHost.rows)
            SystemCommandHost.result=(typeof value=="integer" || typeof value=="float")?value.tointeger():0
        }
      }catch(error){
          if(!("SystemLoadHandleExit" in getroottable()) || !SystemLoadHandleExit(error))throw error
      }
    })
}
function SystemCommandAlpha(value){return value<0?0:value>255?255:value}
function SystemCommandStep(event,skip,elapsed,pointer=null){
    if(SystemCommandHost.host_pending){
        if(event!=2000000006)throw "native completion acknowledgement required"
        SystemCommandHost.host_pending=false;event=0
    }else if(event==2000000006)throw "unexpected native completion acknowledgement"
    if(SystemCommandHost.save_pending && event!=2000000004)throw "configuration persistence acknowledgement required"
    if(!SystemCommandHost.save_pending && event==2000000004)throw "unexpected configuration persistence acknowledgement"
    local root=getroottable()
    local bindings={Sprite=SystemCommandSprite,Screen=SystemCommandScreen,Images=SystemCommandImages,
        file=SystemCommandFile,Save=SystemCommandSave,RestoreVolume=SystemCommandRestoreVolume,RestoreScreen=SystemCommandRestoreScreen,
        wait=SystemCommandWait,event=SystemCommandEvent,InSkip=SystemCommandInSkip,sound=SystemCommandSound,
        ClearImage=SystemCommandClear,SwitchAuto=SystemCommandAuto,CancelAuto=SystemCommandCancelAuto,CancelSkip=SystemCommandCancelSkip,EnableDelay=SystemCommandDelay,
        TransBegin=SystemCommandTransBegin,TransEnd=SystemCommandTransEnd,
        TransSet=SystemCommandTransSet,RenderTrans=SystemCommandRenderTrans,
        ID_OK=2000000001,ID_CANCEL=2000000002,ID_DRAG=2000000003}
    if("SystemGuiBindings" in root)foreach(key,value in SystemGuiBindings())bindings.rawset(key,value)
    if("SystemLoadBindings" in root)foreach(key,value in SystemLoadBindings())bindings.rawset(key,value)
    if("SystemRoomBindings" in root)foreach(key,value in SystemRoomBindings())bindings.rawset(key,value)
    if("SystemExitBindings" in root)foreach(key,value in SystemExitBindings())bindings.rawset(key,value)
    local saved={}
    foreach(key,value in bindings){saved.rawset(key,[key in root,key in root?root[key]:null]);root.rawset(key,value)}
    if(pointer!=null){SystemCommandHost.x=pointer[0];SystemCommandHost.y=pointer[1]}
    SystemCommandHost.event=event;SystemCommandHost.skip=skip
    SystemCommandHost.buttons=[]
    SystemCommandHost.frame=null
    Host.clock+=elapsed
    local failure=null,caught=false
    try{
        if(!SystemCommandHost.started){SystemCommandHost.started=true;SystemCommandHost.thread.call()}
        else if(SystemCommandHost.thread.getstatus()!="idle")SystemCommandHost.thread.wakeup()
    }catch(error){failure=error;caught=true}
    foreach(key,pair in saved){if(pair[0])root.rawset(key,pair[1]);else root.rawdelete(key)}
    if(caught && !(failure==null && SystemCommandHost.terminal_action=="quit"))throw (SystemCommandHost.failure!=null?SystemCommandHost.failure:failure==null?"unexpected null in original system coroutine":failure)
    local out=SystemCommandHost.host_pending?"H\tnative\n":""
    foreach(load in SystemCommandHost.trans_loads)out+=format("M\t%d\t%s\n",load[0],SystemCommandHex(load[1]))
    SystemCommandHost.trans_loads=[]
    foreach(snapshot in SystemCommandHost.snapshots)out+=format("N\t%d\t%s\n",snapshot[0],SystemCommandHex(SystemCommandLines(snapshot[1])))
    SystemCommandHost.snapshots=[]
    foreach(action in SystemCommandHost.actions)out+="U\t"+action+"\n"
    SystemCommandHost.actions=[]
    foreach(sound in SystemCommandHost.sounds)out+=format("S\t%s\t%s\n",SystemCommandHex(sound[0]),SystemCommandHex(sound[1]))
    SystemCommandHost.sounds=[]
    if(SystemCommandHost.thread.getstatus()=="idle"){
        local value=SystemCommandHost.result,kind=typeof value
        if(kind=="null")out+="R\tnull\n"
        else if(kind=="integer" || kind=="bool")out+="R\t"+kind+"\t"+value+"\n"
        else if(kind=="string")out+="R\tstring\t"+SystemCommandHex(value)+"\n"
        else out+="R\tnull\n"
    }else out+="W\t"+SystemCommandHost.delay+"\n"
    if(SystemCommandHost.frame!=null){
        out+="P\t1\t0\t"+SystemCommandHost.delay+"\n"
        foreach(button in SystemCommandHost.frame_buttons)if(button[2])out+=format("B\t%s:%s:%d\t%d\t%d\t%d\t%d\n",button[0],button[7],button[1],button[3],button[4],button[5],button[6])
        out+=SystemCommandLines(SystemCommandHost.frame)
    }
    return out
}
