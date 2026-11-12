// Host primitives for the original settings coroutine.
ConfigPageHost <- {
    thread=null, assets=null, frame=null, previous=[], audio=[],
    event=0, x=-1, y=-1, page=1, reason=0, failure=null, gui=[], index=0,
    buttons=[], frame_buttons=[], frame_hit=false, delay=-1, cancel=1999,
}
class ConfigPageSprite {
    x=0; y=0; width=0; height=0; avail=false; asset=null; layers=null
    constructor(){layers=[]}
    function Base(){layers=[]}
    function Screen(){layers=clone ::ConfigPageHost.previous}
    function Fill(color, opacity=255){layers.append(["F",color,opacity])}
    function Draw(sprite, dx=0, dy=0, opacity=255) {
        if(sprite==null)return
        if(sprite.asset!=null)layers.append(["A",sprite.asset,sprite.x+dx,sprite.y+dy,opacity])
        foreach(item in sprite.layers){
            if(item[0]=="F"){layers.append(item);continue}
            local copy=clone item
            copy[2]+=dx;copy[3]+=dy
            if(copy[0]=="A")copy[4]=copy[4]*opacity/255
            layers.append(copy)
        }
    }
}
class ConfigPageImageset {
    sprites=null;folded=null
    constructor(){sprites={};folded={}}
    function _get(name){
        if(name in sprites)return sprites[name]
        local key=name.tolower()
        return key in folded ? folded[key] : ::ConfigPageSprite()
    }
}
function ConfigPageImages(name) {
    local result=::ConfigPageImageset()
    foreach(key, meta in ::ConfigPageHost.assets[name]){
        local sprite=::ConfigPageSprite()
        sprite.asset=meta[0];sprite.x=meta[1];sprite.y=meta[2]
        sprite.width=meta[3];sprite.height=meta[4];sprite.avail=true
        result.sprites.rawset(key,sprite)
        result.folded.rawset(key.tolower(),sprite)
    }
    return result
}
class ConfigPageSpot {
    normal=null;hover=null;active=null;mask=null;disable=null
    id=0;enabled=true;selected=false;visible=true
    x=0;y=0
    constructor(){normal=::ConfigPageSprite();hover=::ConfigPageSprite();active=::ConfigPageSprite();mask=::ConfigPageSprite();disable=::ConfigPageSprite()}
    function Area(){return mask.avail ? mask : normal}
}
class ConfigPageSpots {
    items=null;images=null;current=null;dx=0;dy=0
    constructor(images=null) {
        items={}
        this.images=images
    }
    function Part(prefix,name){
        if(images==null)return ::ConfigPageSprite()
        local sprites=images.sprites, spaced=prefix+" "+name, compact=prefix+name
        if(spaced in sprites)return sprites[spaced]
        if(compact in sprites)return sprites[compact]
        spaced=spaced.tolower();compact=compact.tolower()
        if(spaced in images.folded)return images.folded[spaced]
        if(compact in images.folded)return images.folded[compact]
        return ::ConfigPageSprite()
    }
    function _get(name){
        if(name in items)return items[name]
        local spot=::ConfigPageSpot()
        spot.normal=Part("=",name);spot.hover=Part("+",name);spot.active=Part("@",name)
        spot.mask=Part("*",name);spot.disable=Part("-",name)
        local area=spot.Area()
        spot.x=area.x;spot.y=area.y
        items.rawset(name,spot)
        return spot
    }
    function Bind(name,id){this[name].id=id}
    function BindR(name,id){
        if(images==null)return
        local lower=name.tolower()
        foreach(raw,image in images.sprites){
            if(raw.len()<2 || "=+@*-".find(raw.slice(0,1))==null)continue
            local key=raw.slice(raw[1]==32 ? 2 : 1)
            if(key.tolower()==lower){Bind(key,id);return}
        }
    }
    function Enable(id,value=true){foreach(spot in items)if(spot.id==id)spot.enabled=value}
    function Active(id,value=true){foreach(spot in items)if(spot.id==id)spot.selected=value}
    function ActiveAll(value){foreach(spot in items)spot.selected=value}
    function Visible(id,value){foreach(spot in items)if(spot.id==id)spot.visible=value}
    function VisibleAll(value){foreach(spot in items)spot.visible=value}
    function Check(x=0,y=0){
        current=null;dx=x;dy=y
        local smallest=null
        foreach(name,spot in items){
            if(!spot.id||!spot.enabled||!spot.visible)continue
            if(name=="FRAME"&&!::ConfigPageHost.frame_hit)continue
            local area=spot.Area()
            local px=::ConfigPageHost.x-x-area.x, py=::ConfigPageHost.y-y-area.y
            local size=area.width*area.height
            if(px>=0&&py>=0&&px<area.width&&py<area.height&&(smallest==null||size<smallest)){
                current=spot;smallest=size
            }
        }
        return current==null ? 0 : current.id
    }
    function GetPosH(count){
        local area=current.Area()
        return (::ConfigPageHost.x-dx-area.x)*count/area.width
    }
    function GetPosV(count){
        local area=current.Area()
        return (::ConfigPageHost.y-dy-area.y)*count/area.height
    }
    function Draw(screen,x=0,y=0,popup=false){
        foreach(name,spot in items){
            if(!spot.id||!spot.visible)continue
            local area=spot.Area()
            if(spot.enabled&&area.avail){
                ::ConfigPageHost.buttons.append([name,area.x+x,area.y+y,area.width,area.height])
            }
            local image=spot.normal
            if(!spot.enabled&&spot.disable.avail)image=spot.disable
            else if(spot==current&&spot.hover.avail)image=spot.hover
            else if(spot.selected&&spot.active.avail)image=spot.active
            screen.Draw(image,x,y)
        }
    }
}
class ConfigPageScreen extends ConfigPageSprite {
    function Update(){
        foreach(gui in ::ConfigPageHost.gui){
            if("PAGE" in gui.m_list)::ConfigPageHost.index=gui.Get("PAGE")
            gui.Get(::Config)
        }
        ::ConfigPageHost.previous=clone layers
        ::ConfigPageHost.frame=layers
        ::ConfigPageHost.frame_buttons=::ConfigPageHost.buttons
        ::ConfigPageHost.buttons=[]
        layers=[]
    }
}
class ConfigPageGUI extends GUI {
    constructor(){m_list={};::ConfigPageHost.gui.append(this)}
}
class ConfigPageGlyph {
    Face=null;Color=0xffffff;Size=22;Bold=false
    function Print(screen,text,x,y,border=null){
        screen.layers.append(["T",text,x,y,Bold?1:0,border!=null?1:0])
    }
}
function ConfigPageWait(time){ConfigPageHost.delay=time;suspend();return true}
function ConfigPageEvent(){return ConfigPageHost.event}
function ConfigPageSound(name){ConfigPageHost.audio.append([name,Config.VolSYS])}
function ConfigPageInstall(assets,page){
    ConfigPageHost.assets=assets
    ConfigPageHost.page=page
    ConfigPageHost.previous=[]
    ConfigPageHost.audio=[]
    ConfigPageHost.reason=0
    ConfigPageHost.failure=null
    ConfigPageHost.gui=[]
    ConfigPageHost.index=0
    ConfigPageHost.buttons=[]
    ConfigPageHost.frame_buttons=[]
    ConfigPageHost.thread=newthread(function(){
        seterrorhandler(function(error){
            ConfigPageHost.failure=error
            for(local level=1;level<12;++level){
                local info=getstackinfos(level)
                if(info==null)break
                ConfigPageHost.failure+=format(" [%s:%d %s]",info.src,info.line,info.func)
            }
        })
        if(ViewConfigPage.getinfos().parameters.len()==1){ViewConfigPage();return}
        while(ConfigPageHost.page){
            ConfigPageHost.gui=[]
            ConfigPageHost.page=ViewConfigPage(ConfigPageHost.page)
        }
    })
}
function ConfigPageStep(event,x,y,elapsed,frame_hit){
    local root=getroottable()
    local host={
        Sprite=ConfigPageSprite, Screen=ConfigPageScreen, Spots=ConfigPageSpots,
        Glyph=ConfigPageGlyph, Images=ConfigPageImages, GUI=ConfigPageGUI,
        wait=ConfigPageWait, event=ConfigPageEvent, sound=ConfigPageSound,
        ID_CANCEL=ConfigPageHost.cancel, ID_DEFAULT=200, ID_SYSTEM=201,
        ID_NEXT_END=202, ID_BACK_END=203
    }
    local saved={}
    foreach(key,value in host){
        saved.rawset(key,[key in root,key in root?root[key]:null])
        root.rawset(key,value)
    }
    local failure=null
    local stage="starting original settings"
    ConfigPageHost.x=x;ConfigPageHost.y=y
    ConfigPageHost.frame_hit=frame_hit
    Host.clock+=elapsed
    try {
        if(ConfigPageHost.reason==0){
            ConfigPageHost.reason=1
            ConfigPageHost.thread.call()
        }
        for(local i=0;i<12;++i){
            if(ConfigPageHost.thread.getstatus()=="idle")break
            ConfigPageHost.frame=null
            ConfigPageHost.event=i==0 ? event : 0
            stage="resuming original settings"
            ConfigPageHost.thread.wakeup()
            if(ConfigPageHost.frame!=null && (i>0||event==0))break
        }
    }catch(error){failure=error}
    foreach(key,entry in saved){
        if(entry[0])root.rawset(key,entry[1])
        else root.rawdelete(key)
    }
    if(failure!=null)throw (stage+": "+(ConfigPageHost.failure!=null ? ConfigPageHost.failure : failure))
    local result=""
    foreach(cue in ConfigPageHost.audio)result+="S\t"+cue[0]+"\t"+cue[1]+"\n"
    ConfigPageHost.audio=[]
    if(ConfigPageHost.thread.getstatus()=="idle"){
        ConfigPageHost.thread=null
        ConfigPageHost.reason=0
        return result+"closed"
    }
    if(ConfigPageHost.frame==null)throw "settings script did not submit a frame"
    result+="P\t"+ConfigPageHost.page+"\t"+ConfigPageHost.index+"\t"+ConfigPageHost.delay+"\n"
    foreach(button in ConfigPageHost.frame_buttons)
        result+=format("B\t%s\t%d\t%d\t%d\t%d\n",button[0],button[1],button[2],button[3],button[4])
    foreach(item in ConfigPageHost.frame){
        switch(item[0]){
            case "A": result+=format("A\t%s\t%d\t%d\t%d\n",item[1],item[2],item[3],item[4]);break
            case "F": result+=format("F\t%d\t%d\n",item[1],item[2]);break
            case "T": result+=format("T\t%d\t%d\t%d\t%d\t%s\n",item[2],item[3],item[4],item[5],item[1]);break
        }
    }
    return result
}
