// NekoPack test host: retained draw plans, explicit queued input, virtual clock.
// Assets/Data/Ini and InputChoice are supplied by the resource adapter.
class Registry {
    entries = null
    constructor() { entries = {} }
    function _set(k,v) { entries.rawset(k,v) }
    function _get(k) { return entries[k] }
}
com <- Registry()
Host <- {caption="",clock=0,frames=[],sounds=[],effects=[],volumes=null,screen=null,missing_spots=[],skip=false,auto_mode=false}
// Retained transition commands only; device interpolation belongs to the renderer.
function TransBegin() { Host.effects.append(["transition-begin"]) }
function TransEnd() { Host.effects.append(["transition-end"]) }
function TransSet(duration) { Host.effects.append(["transition-duration",duration]) }
function RenderTrans() { Host.effects.append(["transition-render",Host.clock]) }
function title(text) { Host.caption=text }
function ini(name) { return Ini[name] }
function data(name) { return Data[name] }
function SavePath(name) { return name }
function file(name,mode) { throw "headless profile is empty; file IO is unavailable" }
function RestoreVolume() { Host.volumes <- [Config.VolBGM,Config.VolVO,Config.VolSE,Config.VolENV,Config.VolSYS] }
function RestoreScreen() { Host.screen=Config.Screen }
function CancelSkip() { Host.skip=false }
function CancelAuto() { Host.auto_mode=false }
function InSkip() { return Host.skip }
function InAuto() { return Host.auto_mode }
function InRestore() { return false }
function sound(name) { Host.sounds.append(name) }
function wait(time) {
    if(time<0)throw "headless input exhausted"
    Host.clock+=time
    return true
}
function event() { return ID_OK }
function NativeCommand(name,args) {
    if(name=="img")Host.image <- clone args
    else if(name=="bgm")Host.bgm <- clone args
    else if(name=="wait" || name=="fade") {
        if(args.len()!=1)throw "timing command requires one duration"
        Host.clock+=args[0].tointeger()
    } else throw "unsupported native command"
    Host.effects.append([name,clone args])
}
class Timer {
    start=0
    constructor() { start=Host.clock }
    function Reset() { start=Host.clock }
    function Get() { return Host.clock-start }
}
class Sprite {
    x=0
    y=0
    width=0
    height=0
    asset=null
    layers=null
    constructor(w=0,h=0) { layers=[]
        width=w
        height=h }
    function Base() { layers=[] }
    function Fill(color,opacity=255) { layers.append([["fill",color,width,height],0,0,opacity]) }
    function Draw(source,dx=0,dy=0,opacity=255) {
        if(source.asset!=null)layers.append([source.asset,source.x+dx,source.y+dy,opacity])
        foreach(layer in source.layers)layers.append([layer[0],layer[1]+dx,layer[2]+dy,layer[3]*opacity/255])
    }
}
function Images(name) {
    local result={}
    foreach(key,meta in Assets[name]) {
        local sprite=Sprite()
        sprite.asset=meta[0]
        sprite.x=meta[1]
        sprite.y=meta[2]
        sprite.width=meta[3]
        sprite.height=meta[4]
        result.rawset(key,sprite)
    }
    return result
}
class Screen extends Sprite {
    function Snap() { Host.snapshot <- clone layers }
    function Update() {
        Host.frames.append(clone layers)
        layers=[]
    }
}
class Spot {
    name=""
    normal=null
    hover=null
    active=null
    mask=null
    popup=null
    enabled=true
    selected=false
    id=0
    function Move(x,y) {
        foreach(sprite in [normal,hover,active,mask]) {
            if(sprite!=null) { sprite.x+=x
                sprite.y+=y }
        }
    }
}
class Spots {
    items=null
    constructor(images=null) {
        items={}
        if(images==null)return
        foreach(name,sprite in images) {
            if(name.len()<3 || name[1]!=32)continue
            local prefix=name.slice(0,1)
            if(prefix!="=" && prefix!="+" && prefix!="@" && prefix!="*")continue
            local key=name.slice(2)
            if(!(key in items)) { local spot=Spot()
                spot.name=key
                items.rawset(key,spot) }
            local spot=items[key]
            switch(prefix) {
                case "=":spot.normal=sprite;break
                case "+":spot.hover=sprite;break
                case "@":spot.active=sprite;break
                case "*":spot.mask=sprite;break
            }
        }
    }
    function _get(name) { return items[name] }
    function Add(spot) { items.rawset(spot.name,spot) }
    function Bind(name,id) {
        if(!(name in items)) { Host.missing_spots.append(name)
            return false }
        items[name].id=id
        return true
    }
    function Enable(id,value) { foreach(spot in items)if(spot.id==id)spot.enabled=value }
    function Active(id,value=true) { foreach(spot in items)if(spot.id==id)spot.selected=value }
    function ActiveAll(value) { foreach(spot in items)spot.selected=value }
    function Check() {
        foreach(spot in items)if(spot.id==InputChoice && spot.enabled)return InputChoice
        throw "requested input does not identify an enabled spot"
    }
    function Draw(screen,dx=0,dy=0,popup=false) {
        foreach(spot in items) {
            if(!spot.enabled)continue
            local sprite=spot.selected ? spot.active : spot.normal
            if(sprite!=null)screen.Draw(sprite,dx,dy)
        }
    }
}
