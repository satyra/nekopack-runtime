// Current message-box objects and the original message handler's draw hooks.
function NativeMessageLayerHex(value){
    local out=""
    for(local i=0;i<value.len();++i)out+=format("%02x",value[i])
    return out
}
function NativeMessageSpriteSnapshot(label,sprite){
    local out=format("S\t%s\t%s\t%d\t%d\t%d\t%d\n",label,
        sprite.asset==null ? "" : NativeMessageLayerHex(sprite.asset),sprite.x,sprite.y,sprite.width,sprite.height)
    foreach(item in sprite.layers){
        if(typeof item[0]!="string")throw "unsupported message sprite fill layer"
        if(item[0]=="C" || item[0]=="Q" || item[0]=="O" || item[0]=="X")throw "unsupported message snapshot composite primitive"
        local asset=item[0],x=item[1],y=item[2],opacity=item[3]
        if(item[0]=="A"){asset=item[1];x=item[2];y=item[3];opacity=item[4]}
        opacity=opacity<0 ? 0 : opacity>255 ? 255 : opacity
        out+=format("L\t%s\t%s\t%d\t%d\t%d\n",label,NativeMessageLayerHex(asset),x,y,opacity)
    }
    return out
}
function NativeMessageBoxSnapshot(){
    local box=::MsgBox
    local classic="name_x" in box
    local out=classic ? "M\tclassic\n" : "M\tshared\n"
    out+=format("P\t%d\t%d\n",NativeMessageFrameAfterName?1:0,NativeMessageFrameRequiresName?1:0)
    out+=NativeMessageSpriteSnapshot("name",box.name)
    out+=NativeMessageSpriteSnapshot("text",box.text)
    if(classic){
        out+=format("G\tname\t%d\t%d\t%d\n",box.name_x,box.name_y,box.name_w)
        out+=format("G\ttext\t%d\t%d\t%d\n",box.text_x,box.text_y,box.text_w)
    }else{
        out+=NativeMessageSpriteSnapshot("base",box.base)
        out+=NativeMessageSpriteSnapshot("frame",box.frame)
    }
    return out
}

class NativeMessageLayerSprite {
    x=0;y=0;width=0;height=0;asset=null;layers=null
    constructor(source=null){
        layers=[]
        if(source!=null){x=source.x;y=source.y;width=source.width;height=source.height;asset=source.asset;layers=clone source.layers}
    }
    function Draw(source,dx=0,dy=0,opacity=255){
        if(source.asset!=null)layers.append(["A",source.asset,source.x+dx,source.y+dy,opacity])
        foreach(item in source.layers){
            if(item[0]!="A")throw "unsupported message overlay drawing primitive"
            local copy=clone item
            copy[2]+=source.x+dx;copy[3]+=source.y+dy;copy[4]=copy[4]*opacity/255
            layers.append(copy)
        }
    }
}
function NativeMessageLayerRun(begin,rest,elapsed){
    local root=getroottable(),saved={}
    local bindings={Sprite=NativeMessageLayerSprite,InRestore=function(){return ::NativeMessageLayerRest}}
    foreach(key,value in bindings){saved.rawset(key,[key in root,key in root?root[key]:null]);root.rawset(key,value)}
    NativeMessageLayerRest <- rest
    Host.clock+=elapsed
    local screen=NativeMessageLayerSprite(),active=false,failure=null
    try{
        if(begin)NativeMessageLayerBegin()
        active=NativeMessageLayerDraw(screen)
    }catch(error){failure=error}
    foreach(key,pair in saved){if(pair[0])root.rawset(key,pair[1]);else root.rawdelete(key)}
    root.rawdelete("NativeMessageLayerRest")
    if(failure!=null)throw failure
    return (active ? "D\t1\n" : "D\t0\n")+NativeMessageSpriteSnapshot("overlay",screen)
}
