// ViewMenu binding/enable rules from the original shortcut.nut; no event loop.
local images=Images("menu")
local spots=Spots(images)
foreach(name,id in SHORTCUTS)spots.Bind(name,id)
local route=InRoute()
spots.Enable(ID_LOG,route);spots.Enable(ID_SAVE,route);spots.Enable(ID_QSAVE,route)
local ordinal=function(sprite){
    if(sprite==null)return -1
    local p=split(sprite.asset,":");return p[p.len()-1].tointeger()
}
local result="base\t"+ordinal(images["BASE"])+"\n"
foreach(name,spot in spots.items){
    if(!spot.id)continue
    result+="button\t"+name+"\t"+(spot.enabled?1:0)+"\t"+ordinal(spot.normal)+"\t"+ordinal(spot.hover)+"\t"+ordinal(spot.mask!=null?spot.mask:spot.normal)+"\n"
}
return result
