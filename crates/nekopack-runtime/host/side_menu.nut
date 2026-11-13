// Query the original shortcut updater against runtime-owned save/route state.
function SideMenuPlan(quick,in_route,voice,skip,auto_mode,next,back){
    local root=getroottable()
    NativeSideInputs <- {quick=quick,in_route=in_route,voice=voice,skip=skip,auto_mode=auto_mode,next=next,back=back}
    local host={
        QuickCheck=function(){return NativeSideInputs.quick},
        InRoute=function(){return NativeSideInputs.in_route},
        HasLastVoice=function(){return NativeSideInputs.voice},
        InSkip=function(){return NativeSideInputs.skip},
        InAuto=function(){return NativeSideInputs.auto_mode}
    }
    if("IsEnableSkipNext" in root)host.rawset("IsEnableSkipNext",function(){return NativeSideInputs.next})
    if("IsEnableSkipBack" in root)host.rawset("IsEnableSkipBack",function(){return NativeSideInputs.back})
    local saved={}
    foreach(key,value in host){saved.rawset(key,root[key]);root.rawset(key,value)}
    local result="",failure=null
    try{
        ShortCut.Update()
        foreach(name,spot in ShortCut.m_spots.items){
            local action=null
            foreach(key,id in SHORTCUTS)if(id==spot.id){action=key;break}
            if(action==null)continue
            local ordinal=function(sprite){
                if(sprite==null || sprite.asset==null)return -1
                local parts=split(sprite.asset,":")
                return parts[parts.len()-1].tointeger()
            }
            local area=(spot.mask!=null && spot.mask.asset!=null) ? spot.mask : spot.normal
            result+=format("B\t%s\t%d\t%d\t%d\t%d\t%d\t%d\n",action,spot.enabled?1:0,
                ordinal(spot.normal),ordinal(spot.hover),spot.selected?ordinal(spot.active):-1,ordinal(spot.disable),ordinal(area))
        }
    }catch(error){failure=error}
    foreach(key,value in saved)root.rawset(key,value)
    root.rawdelete("NativeSideInputs")
    if(failure!=null)throw failure
    return result
}
