// Presentation adapter for the observed ViewNormalMap initialization contract.
// Only builds the draw/binding plan; does not invoke menu destinations or complete .map.
local images=Images(TitleName)
local spots=Spots(images)
local bases=[images["BASE"]]
foreach(i,item in TitleRows){
    local p=split(item," \t")
    local name=p[0]
    local enabled=p.len()>1 ? FlagCheck(p[1]) : true
    local active=p.len()>2 ? FlagCheck(p[2]) : false
    if(name[0]==64){if(enabled && name in images)bases.append(images[name]);continue}
    if(name=="デバッグ" && !(name in spots.items))continue
    if(!spots.Bind(name,i+1))throw "title button asset missing: "+name
    spots.Enable(i+1,enabled)
    spots.Active(i+1,active)
}
local ordinal=function(sprite){
    if(sprite==null)return -1
    local p=split(sprite.asset,":")
    return p[p.len()-1].tointeger()
}
local result=""
foreach(sprite in bases)result+="base\t"+ordinal(sprite)+"\n"
foreach(spot in spots.items){
    if(!spot.id)continue
    result+="button\t"+spot.id+"\t"+(spot.enabled?1:0)+"\t"+
        ordinal(spot.selected?spot.active:spot.normal)+"\t"+ordinal(spot.hover)+"\t"+
        ordinal(spot.mask!=null?spot.mask:spot.normal)+"\n"
}
return result
