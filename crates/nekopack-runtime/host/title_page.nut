// Runs the mounted title handler in its own coroutine and restores host globals
// after each step, allowing the settings coroutine to use the same primitives.
TitlePageHost <- {state=null,thread=null,result=null,failure=null,buttons=[],frame_buttons=[],args=null,rows=null,snapshot=null}
class TitlePageScreen extends ConfigPageScreen {
    function Snap(){
        ::TitlePageHost.snapshot=clone layers
        ::Host.snapshot <- clone layers
    }
    function Update(){
        ::ConfigPageHost.previous=clone layers
        ::ConfigPageHost.frame=layers
        ::ConfigPageHost.buttons=[]
        layers=[]
        ::TitlePageHost.frame_buttons=::TitlePageHost.buttons
        ::TitlePageHost.buttons=[]
    }
}
class TitlePageSpots extends ConfigPageSpots {
    focus=0
    function Forcus(id){focus=id}
    function Check(x=0,y=0){
        current=null
        local smallest=null
        foreach(name,spot in items){
            if(!spot.id || !spot.enabled || !spot.visible)continue
            local area=spot.Area()
            local px=::ConfigPageHost.x-x-area.x,py=::ConfigPageHost.y-y-area.y
            local size=area.width*area.height
            if(px>=0&&py>=0&&px<area.width&&py<area.height&&(smallest==null||size<smallest)){
                current=spot;smallest=size
            }
        }
        if(::ConfigPageHost.x>=0 || ::ConfigPageHost.y>=0)return current==null?0:current.id
        foreach(name,spot in items){
            if(spot.id==focus && spot.enabled && spot.visible){current=spot;return spot.id}
        }
        return 0
    }
    function Draw(screen,x=0,y=0,popup=false){
        foreach(name,spot in items){
            if(!spot.id || !spot.visible)continue
            local area=spot.Area()
            if(area.avail)::TitlePageHost.buttons.append([spot.id,spot.enabled?1:0,area.x+x,area.y+y,area.width,area.height])
            local image=spot.normal
            if(!spot.enabled&&spot.disable.avail)image=spot.disable
            else if(spot==current&&spot.hover.avail)image=spot.hover
            else if(spot.selected&&spot.active.avail)image=spot.active
            screen.Draw(image,x,y)
        }
    }
}
function TitlePageInstall(assets,args,rows){
    TitlePageHost.state={
        assets=assets,frame=null,previous=[],audio=[],event=0,x=-1,y=-1,
        page=1,reason=0,failure=null,gui=[],index=0,buttons=[],frame_buttons=[],
        frame_hit=false,delay=-1,cancel=1999
    }
    TitlePageHost.result=null
    TitlePageHost.args=args
    TitlePageHost.rows=rows
    TitlePageHost.failure=null
    TitlePageHost.buttons=[]
    TitlePageHost.frame_buttons=[]
    TitlePageHost.thread=newthread(function(){
        seterrorhandler(function(error){
            TitlePageHost.failure=error
            for(local level=1;level<12;++level){
                local info=getstackinfos(level)
                if(info==null)break
                TitlePageHost.failure+=format(" [%s:%d %s]",info.src,info.line,info.func)
            }
        })
        local handler=com["title"]
        TitlePageHost.result=handler(TitlePageHost.args,TitlePageHost.rows)
    })
}
function TitlePageAlpha(alpha){return alpha<0?0:alpha>255?255:alpha}
function TitlePageStep(event,x,y,elapsed){
    local root=getroottable()
    local host={
        ConfigPageHost=TitlePageHost.state,
        Sprite=ConfigPageSprite,Screen=TitlePageScreen,Spots=TitlePageSpots,
        Images=ConfigPageImages,wait=ConfigPageWait,event=ConfigPageEvent,sound=ConfigPageSound
    }
    local saved={}
    foreach(key,value in host){saved.rawset(key,root[key]);root.rawset(key,value)}
    local failure=null
    ConfigPageHost.x=x;ConfigPageHost.y=y;ConfigPageHost.event=event
    Host.clock+=elapsed
    try {
        if(ConfigPageHost.reason==0){
            ConfigPageHost.reason=1
            TitlePageHost.thread.call()
        }
        if(TitlePageHost.thread.getstatus()!="idle"){
            ConfigPageHost.frame=null
            TitlePageHost.thread.wakeup()
        }
    }catch(error){failure=error}
    local state=ConfigPageHost
    foreach(key,value in saved)root.rawset(key,value)
    if(failure!=null)throw (TitlePageHost.failure!=null?TitlePageHost.failure:failure)
    local result=""
    foreach(cue in state.audio)result+="S\t"+cue[0]+"\t"+cue[1]+"\n"
    state.audio=[]
    if(TitlePageHost.thread.getstatus()=="idle")return result+"closed:"+TitlePageHost.result
    if(state.frame==null)throw "title script did not submit a frame"
    result+="P\t1\t0\t"+state.delay+"\n"
    foreach(button in TitlePageHost.frame_buttons)
        result+=format("B\t%d:%d\t%d\t%d\t%d\t%d\n",button[0],button[1],button[2],button[3],button[4],button[5])
    foreach(item in state.frame){
        switch(item[0]){
            case "A":result+=format("A\t%s\t%d\t%d\t%d\n",item[1],item[2],item[3],TitlePageAlpha(item[4]));break
            case "F":result+=format("F\t%d\t%d\n",item[1],TitlePageAlpha(item[2]));break
            default:throw "unsupported title drawing primitive"
        }
    }
    return result
}
