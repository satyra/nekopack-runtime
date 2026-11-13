// Executes the original Glyph initialization with a recorder in place of rasterization.
class NativeMessageLayoutGlyph {
    Face=null; Bold=false; Color=0xffffff; Size=0; LineWidth=0; LineSpan=0
    capture=null
    constructor(){capture={name_text="",name_size=0,name_x=0,name_y=0,name_color=0xffffff,name_border=-1,text_border=-1}}
    function Reset(){}
    function Print(target,text,x,y,border=null){
        capture.name_color=Color;capture.name_border=border==null ? -1 : border;
        capture.name_text=text;capture.name_size=Size;capture.name_x=x;capture.name_y=y
    }
    function Build2(border){capture.text_border=border;return []}
    function Build(text){
        capture.text_color<-Color;capture.text_size<-Size;capture.line_width<-LineWidth;capture.line_span<-LineSpan
        return []
    }
}
class NativeMessageLayoutSprite {function Clear(){}}
function NativeMessagePack(value){return value.len()+":"+value}
function NativeMessageFields(m){
    local mask="mask" in m ? m.mask : ""
    return NativeMessagePack(m.name)+NativeMessagePack(m.voice)+NativeMessagePack(mask)+NativeMessagePack(m.text)
}
