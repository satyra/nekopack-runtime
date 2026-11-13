// UI primitives for the existing SystemCommand coroutine. Original GUI and
// control classes keep ownership of their Update/Get/Put calls.
SystemGuiState <- {next_group=0}
class SystemGuiSpot {
    name="";id=0;enabled=true;is_rect=false;selected=false;visible=true
    x=0;y=0
    normal=null;hover=null;active=null;mask=null;disable=null;popup=null
    constructor(){
        normal=::SystemCommandSprite();hover=::SystemCommandSprite()
        active=::SystemCommandSprite();mask=::SystemCommandSprite()
        disable=::SystemCommandSprite();popup=::SystemCommandSprite()
    }
    function _get(name){
        if(name=="is_enable")return enabled
        throw "unknown original Spot property: "+name
    }
    function _set(name,value){
        if(name=="is_enable"){enabled=value;return value}
        throw "unknown original Spot property: "+name
    }
    function Area(){
        if(mask.avail)return mask
        if(normal.avail)return normal
        return disable
    }
}
class SystemGuiSpots {
    m_items=null;items=null;m_images=null;m_folded=null;m_current=null
    m_group=0;m_focus=0;m_dx=0;m_dy=0;m_switch=false
    constructor(images=null){
        m_items=[];items={};m_images=images;m_folded={}
        m_group=::SystemGuiState.next_group.tostring()
        ::SystemGuiState.next_group++
        if(images!=null)foreach(name,image in images)m_folded.rawset(name.tolower(),image)
    }
    function Part(prefix,name){
        if(m_images==null)return ::SystemCommandSprite()
        local spaced=prefix+" "+name,compact=prefix+name
        if(spaced in m_images)return m_images[spaced]
        if(compact in m_images)return m_images[compact]
        spaced=spaced.tolower();compact=compact.tolower()
        if(spaced in m_folded)return m_folded[spaced]
        if(compact in m_folded)return m_folded[compact]
        return ::SystemCommandSprite()
    }
    function Find(name){
        if(name in items)return items[name]
        local spot=::SystemGuiSpot()
        spot.name=name
        spot.normal=Part("=",name);spot.hover=Part("+",name);spot.active=Part("@",name)
        spot.mask=Part("*",name);spot.disable=Part("-",name)
        local area=spot.Area();spot.x=area.x;spot.y=area.y
        items.rawset(name,spot);m_items.append(spot)
        return spot
    }
    function _get(name){
        if(typeof name=="integer"){
            foreach(spot in m_items)if(spot.id==name)return spot
            throw "unknown system GUI spot id"
        }
        return Find(name)
    }
    function Add(spot){
        foreach(item in m_items)if(item==spot)return
        m_items.append(spot)
    }
    function Bind(name,id){Find(name).id=id}
    function BindR(name,id){
        if(m_images==null)return false
        local lower=name.tolower()
        foreach(raw,image in m_images){
            if(raw.len()<2 || "=+@*-".find(raw.slice(0,1))==null)continue
            local key=raw.slice(raw[1]==32 ? 2 : 1)
            if(key.tolower()==lower){Bind(key,id);return true}
        }
        return false
    }
    function Enable(id,value=true){foreach(spot in m_items)if(spot.id==id)spot.enabled=value}
    function Active(id,value=true){foreach(spot in m_items)if(spot.id==id)spot.selected=value}
    function ActiveAll(value){foreach(spot in m_items)spot.selected=value}
    function Visible(id,value){foreach(spot in m_items)if(spot.id==id)spot.visible=value}
    function VisibleAll(value){foreach(spot in m_items)spot.visible=value}
    function SwitchMode(enabled=true){m_switch=enabled}
    function Forcus(id){m_focus=id}
    function Record(spot,dx,dy){
        local area=spot.Area()
        if(spot.id && spot.visible && area.avail)::SystemCommandHost.buttons.append([
            m_group,spot.id,(spot.enabled && spot.is_enable),area.x+dx,area.y+dy,area.width,area.height,spot.name])
    }
    function Check(dx=0,dy=0){
        m_current=null;m_dx=dx;m_dy=dy
        foreach(spot in m_items){
            Record(spot,dx,dy)
            if(!spot.id || (!spot.enabled || !spot.is_enable) || !spot.visible || (!m_switch && spot.selected))continue
            local area=spot.Area()
            local px=::SystemCommandHost.x-dx-area.x,py=::SystemCommandHost.y-dy-area.y
            if(px>=0 && py>=0 && px<area.width && py<area.height){
                m_current=spot;break
            }
        }
        if(::SystemCommandHost.x>=0 || ::SystemCommandHost.y>=0)return m_current==null ? 0 : m_current.id
        foreach(spot in m_items){
            if(spot.id==m_focus && spot.enabled && spot.is_enable && spot.visible){m_current=spot;return spot.id}
        }
        return 0
    }
    function GetPosH(count){
        local area=m_current.Area()
        return (::SystemCommandHost.x-m_dx-area.x)*count/area.width
    }
    function GetPosV(count){
        local area=m_current.Area()
        return (::SystemCommandHost.y-m_dy-area.y)*count/area.height
    }
    function Draw(screen,dx=0,dy=0,popup=false){
        if(popup)throw "unsupported system GUI popup rendering"
        foreach(spot in m_items){
            if(!spot.id || !spot.visible)continue
            local area=spot.Area()
            Record(spot,dx,dy)
            local sprite=spot.normal
            if((!spot.enabled || !spot.is_enable) && spot.disable.avail)sprite=spot.disable
            else if(m_switch){if(spot.selected && spot.active.avail)sprite=spot.active}
            else if(spot==m_current && spot.hover.avail)sprite=spot.hover
            else if(spot.selected && spot.active.avail)sprite=spot.active
            screen.Draw(sprite,dx,dy)
        }
    }
}
class SystemGuiGlyph {
    Face=null;Color=0xffffff;Size=22;Bold=false
    function Print(target,text,x,y,border=null){
        if(Size<=0 || Size>512)throw "invalid system GUI Glyph size"
        if(Size==22 && Color==0xffffff && (border==null || border==0) && text.find("\n")==null && text.find("\r")==null)
            target.layers.append(["T",text,x,y,Bold?1:0,border!=null?1:0])
        else target.layers.append(["G",text,x,y,Size,Color,Bold?1:0,border==null?-1:border])
    }
}
function SystemGuiBindings(){
    return {Spot=SystemGuiSpot,Spots=SystemGuiSpots,Glyph=SystemGuiGlyph}
}
