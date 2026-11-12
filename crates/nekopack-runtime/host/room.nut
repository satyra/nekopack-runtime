// Display and input host for the original room script. wait() yields its event loop.
RoomHost <- {
    saved={}, frame=null, buttons=[], nextGroup=0, thread=null,
    event=0, group=-1, hit=-1, replay=null, visual="", active="", failure=null, audio=""
}
class RoomSprite {
    x=0; y=0; width=0; height=0; asset=null; layers=null; avail=false
    constructor(source=null) {
        layers=[]
        if(source!=null){width=source.width;height=source.height;avail=source.avail}
    }
    function Fill(color,alpha=255){layers.append(["F",color,alpha])}
    function Draw(source,dx=0,dy=0){
        if(source==null)return
        local sx=source.x+dx
        local sy=source.y+dy
        if(source.asset!=null)layers.append(["A",source.asset,sx,sy])
        if(source.layers.len())layers.append(["L",source.width,source.height,sx,sy,clone source.layers])
    }
    function Load(name,folder){
        local picture=compilestring("return "+::NekoResource("Picture",folder+"/"+name+".png"))()
        if(picture==null)return false
        asset=picture[0]; x=picture[1]; y=picture[2]; avail=true
        return true
    }
}
class RoomImageSet {
    sprites=null
    constructor(){sprites={}}
    function _get(key){return key in sprites ? sprites[key] : RoomSprite()}
}
function RoomImages(name=null,folder="system"){
    local result=RoomImageSet()
    if(name==null)return result
    local resource=folder=="system" ? name : folder+"/"+name
    local assets=compilestring("return "+::NekoResource("Assets",resource))()
    foreach(key,meta in assets){
        local sprite=RoomSprite()
        sprite.asset=meta[0];sprite.x=meta[1];sprite.y=meta[2];sprite.width=meta[3];sprite.height=meta[4];sprite.avail=true
        result.sprites.rawset(key,sprite)
    }
    return result
}
function RoomData(name){return compilestring("return "+::NekoResource("Data",name))()}
class RoomSpot {
    normal=null;hover=null;active=null;mask=null;disable=null;popup=null
    id=0; enabled=true;selected=false
}
class RoomSpots {
    items=null; named=null; group=0
    constructor(images=null){
        items=[]; named={}; group=RoomHost.nextGroup++
        if(images==null)return
        foreach(name,sprite in images.sprites){
            if(name.len()<2)continue
            local prefix=name.slice(0,1)
            if(prefix!="=" && prefix!="+" && prefix!="@" && prefix!="*")continue
            local key=name.slice(name[1]==32 ? 2 : 1)
            if(!(key in named)){local spot=RoomSpot();named.rawset(key,spot);items.append(spot)}
            local spot=named[key]
            switch(prefix){
                case "=":spot.normal=sprite;break
                case "+":spot.hover=sprite;break
                case "@":spot.active=sprite;break
                case "*":spot.mask=sprite;break
            }
        }
    }
    function _get(name){return named[name]}
    function Add(spot){items.append(spot)}
    function Bind(name,id){if(!(name in named))return false;named[name].id=id;return true}
    function BindR(name,id){
        local key=name.tolower()
        foreach(label,spot in named){
            if(label.tolower()==key){spot.id=id;return true}
        }
        return false
    }
    function Enable(id,value=true){foreach(spot in items)if(spot.id==id)spot.enabled=value}
    function Active(id,value=true){foreach(spot in items)if(spot.id==id)spot.selected=value}
    function ActiveAll(value){foreach(spot in items)spot.selected=value}
    function Check(dx=0,dy=0){
        if(RoomHost.group==group)
            foreach(spot in items)if(spot.id==RoomHost.hit && spot.enabled)return spot.id
        return 0
    }
    function Draw(screen,dx=0,dy=0){
        foreach(spot in items){
            if(!spot.id)continue
            local area=spot.mask!=null ? spot.mask : spot.disable!=null ? spot.disable : spot.normal
            if(area!=null)RoomHost.buttons.append([group,spot.id,spot.enabled ? 1 : 0,area.x+dx,area.y+dy,area.width,area.height])
            local sprite=spot.normal
            if(!spot.enabled)sprite=spot.disable
            else if(Check()==spot.id && spot.hover!=null)sprite=spot.hover
            else if(spot.selected && spot.active!=null)sprite=spot.active
            screen.Draw(sprite,dx,dy)
        }
    }
}
class RoomScreen extends RoomSprite {
    function Render(){return false}
    function Update(){
        local visual=layers.len()==0 && RoomHost.visual.len()!=0
        if(visual){
            layers.append(["A","image/visual/"+RoomHost.visual+".png",0,0])
        }
        if(layers.len() && layers[0][0]=="A"){
            local base=layers[0][1]
            if(base.find("system/x_cg.mng:")==0)RoomHost.active="cg"
            if(base.find("system/x_scene.mng:")==0)RoomHost.active="scene"
            if(base.find("system/x_actor.mng:")==0)RoomHost.active="actor"
        }
        RoomHost.frame=[clone layers,clone RoomHost.buttons,visual]
        layers=[]
        RoomHost.buttons=[]
    }
}
function RoomWait(time){suspend();return true}
function RoomEvent(){return RoomHost.event}
function RoomSound(name){RoomHost.audio+="S\t"+name+"\n"}
function RoomStopMusic(){RoomHost.audio+="M\n"}
function RoomAssignImage(name){RoomHost.visual=name}
function RoomReset(){}
function RoomEval(name){RoomHost.replay=name;suspend()}
function RoomInstall(name,cancel){
    local root=getroottable()
    local bindings={
        ID_CANCEL=cancel, Sprite=RoomSprite, Images=RoomImages, Spot=RoomSpot, Spots=RoomSpots, Screen=RoomScreen,
        data=RoomData,wait=RoomWait,event=RoomEvent,sound=RoomSound,StopMusic=RoomStopMusic,
        AssignImage=RoomAssignImage,ScriptReset=RoomReset,ScriptEval=RoomEval
    }
    foreach(key,value in bindings){
        RoomHost.saved.rawset(key,[key in root,key in root ? root[key] : null])
        root.rawset(key,value)
    }
    RoomHost.active=name
    RoomHost.thread=newthread(function(){
        seterrorhandler(function(error){
            local info=getstackinfos(1)
            RoomHost.failure=info!=null ? format("%s:%d: %s",info.src,info.line,error) : error
        })
        local handler=com["room"]
        handler([RoomHost.active])
    })
    RoomHost.thread.call()
}
function RoomRestore(){
    local root=getroottable()
    RoomHost.thread=null
    foreach(key,pair in RoomHost.saved){
        if(pair[0])root.rawset(key,pair[1])
        else root.rawdelete(key)
    }
    RoomHost.saved={}
}
function RoomLines(layers){
    local result=""
    foreach(layer in layers){
        if(layer[0]=="L"){
            result+=format("L\t%d\t%d\t%d\t%d\n",layer[1],layer[2],layer[3],layer[4])
            result+=RoomLines(layer[5])+"E\n"
        }else if(layer[0]=="A")result+=format("A\t%s\t%d\t%d\n",layer[1],layer[2],layer[3])
        else result+=format("F\t%d\t%d\n",layer[1],layer[2])
    }
    return result
}
function RoomAdvance(event,group,hit){
    for(local i=0;i<12;++i){
        RoomHost.frame=null
        if(RoomHost.thread.getstatus()=="idle"){RoomRestore();return "closed"}
        RoomHost.event=i==0 ? event : 0
        RoomHost.group=i==0 ? group : -1
        RoomHost.hit=i==0 ? hit : -1
        try { RoomHost.thread.wakeup() }
        catch(error) { throw RoomHost.failure!=null ? RoomHost.failure : error }
        if(RoomHost.replay!=null){local scene=RoomHost.replay;RoomRestore();return "replay\t"+scene}
        if(RoomHost.thread.getstatus()=="idle"){RoomRestore();return "closed"}
        if(RoomHost.frame!=null && (i>0 || event==0)){
            local frame=RoomHost.frame
            local result="frame\t"+RoomHost.active+"\t"+(frame[2] ? "1" : "0")+"\n"
            foreach(button in frame[1])result+=format("B\t%d\t%d\t%d\t%d\t%d\t%d\t%d\n",button[0],button[1],button[2],button[3],button[4],button[5],button[6])
            return result+RoomLines(frame[0])
        }
    }
    throw "room did not render"
}

function RoomStep(event,group,hit){
    local result=RoomAdvance(event,group,hit)
    local audio=RoomHost.audio
    RoomHost.audio=""
    return audio+result
}
