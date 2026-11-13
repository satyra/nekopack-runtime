//! Reuse the mounted message handler's own local parser and voice predicate.
//! Presentation remains native; parameter semantics remain owned by system code.

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SpriteLayer {
    pub asset: String,
    pub position: [i32; 2],
    pub opacity: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SpriteSnapshot {
    pub asset: Option<String>,
    pub position: [i32; 2],
    pub size: [u32; 2],
    pub layers: Vec<SpriteLayer>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum MessageBoxSnapshot {
    Classic {name: SpriteSnapshot, text: SpriteSnapshot, name_origin: [i32; 2], name_width: i32,
             text_origin: [i32; 2], text_width: i32},
    Shared {base: SpriteSnapshot, frame: SpriteSnapshot, name: SpriteSnapshot, text: SpriteSnapshot,
            frame_after_name: bool, frame_requires_name: bool},
}

pub(crate) struct MessageLayers {
    pub overlay: SpriteSnapshot,
    pub keep_animating: bool,
}

/// Preserve offsets and literals while excluding both Squirrel comment forms.
fn effective_code(source: &str) -> String {
    let mut bytes=source.as_bytes().to_vec();
    let mut i=0;
    while i<bytes.len() {
        if bytes[i]==b'"' || bytes[i]==b'\'' {
            let quote=bytes[i];i+=1;
            while i<bytes.len() {
                if bytes[i]==b'\\' {i+=2;continue;}
                if bytes[i]==quote {i+=1;break;}
                i+=1;
            }
        } else if bytes[i..].starts_with(b"//") {
            while i<bytes.len() && bytes[i]!=b'\n' {bytes[i]=b' ';i+=1;}
        } else if bytes[i..].starts_with(b"/*") {
            bytes[i]=b' ';bytes[i+1]=b' ';i+=2;
            while i<bytes.len() {
                if bytes[i..].starts_with(b"*/") {bytes[i]=b' ';bytes[i+1]=b' ';i+=2;break;}
                if bytes[i]!=b'\n' {bytes[i]=b' ';}
                i+=1;
            }
        } else {i+=1;}
    }
    String::from_utf8(bytes).expect("comment removal preserves UTF-8 literals")
}

pub(crate) fn layer_adapter(source: &str) -> Result<String, String> {
    let code=effective_code(source);
    let (_,handler)=code.split_once("com[\"\"] = function(args){").ok_or("missing message handler for layers")?;
    let before=handler.lines().filter(|line|line.trim()=="UpdateClimax()" || line.trim()=="UpdateClimax();")
        .collect::<Vec<_>>().join("\n");
    let draws=handler.lines().filter(|line| {
        let line=line.trim();
        line=="if(Config.Climax)s.Draw(ClimaxImage)" || line=="if(Config.Climax)s.Draw(ClimaxImage);"
            || line=="if(DrawDate(s))delay=0" || line=="if(DrawDate(s))delay=0;"
    }).collect::<Vec<_>>().join("\n");
    if handler.contains("UpdateClimax(") && before.is_empty() {return Err("unsupported message pre-draw hook statement".into());}
    if handler.contains("DrawDate(s)") && !draws.contains("DrawDate(s)") {return Err("unsupported message date draw statement".into());}
    if handler.contains("s.Draw(ClimaxImage)") && !draws.contains("s.Draw(ClimaxImage)") {return Err("unsupported message climax draw statement".into());}
    let name=handler.find("s.Draw(name)");let frame=handler.find("s.Draw(frame)");
    let after=matches!((name,frame),(Some(n),Some(f)) if n<f);
    let gated=handler.lines().any(|line|line.trim().starts_with("if(m.name.len())s.Draw(frame)"));
    Ok(format!("{}\nNativeMessageFrameAfterName <- {after}\nNativeMessageFrameRequiresName <- {gated}\nfunction NativeMessageLayerBegin(){{\n{before}\n}}\nfunction NativeMessageLayerDraw(s){{\nlocal delay=-1\n{draws}\nreturn delay==0\n}}",
               include_str!("../host/message_layers.nut")))
}

fn snapshot_hex(value: &str) -> Result<String, String> {
    if value.len()%2!=0 {return Err("invalid sprite resource encoding".into());}
    let bytes=(0..value.len()).step_by(2).map(|i|u8::from_str_radix(&value[i..i+2],16))
        .collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?;
    String::from_utf8(bytes).map_err(|e|e.to_string())
}

fn sprite_snapshots(plan: &str) -> Result<std::collections::BTreeMap<String,SpriteSnapshot>,String> {
    let mut sprites=std::collections::BTreeMap::new();
    for line in plan.lines() {
        let f:Vec<_>=line.split('\t').collect();
        let integer=|v:&str|v.parse::<i32>().map_err(|e|e.to_string());
        match f.as_slice() {
            ["S",name,asset,x,y,w,h] => {
                let asset=snapshot_hex(asset)?;
                sprites.insert((*name).into(),SpriteSnapshot {asset:(!asset.is_empty()).then_some(asset),
                    position:[integer(x)?,integer(y)?],size:[w.parse::<u32>().map_err(|e|e.to_string())?,h.parse::<u32>().map_err(|e|e.to_string())?],layers:Vec::new()});
            }
            ["L",name,asset,x,y,opacity] => sprites.get_mut(*name).ok_or("sprite layer without header")?.layers.push(SpriteLayer {
                asset:snapshot_hex(asset)?,position:[integer(x)?,integer(y)?],opacity:opacity.parse::<u8>().map_err(|e|e.to_string())?}),
            ["M",_] | ["G",..] | ["D",_] | ["P",_,_] => {}
            _=>return Err("invalid message sprite snapshot".into()),
        }
    }
    Ok(sprites)
}

pub(crate) fn message_box(vm: &mut nekopack_script::system::SystemVm) -> Result<MessageBoxSnapshot,String> {
    vm.eval("message-box-snapshot","return NativeMessageBoxSnapshot()")?;
    let plan=vm.result();
    let mut sprites=sprite_snapshots(&plan)?;
    let mut take=|name|sprites.remove(name).ok_or_else(||format!("message box missing {name}"));
    let name=take("name")?;let text=take("text")?;
    if plan.starts_with("M\tclassic\n") {
        let geometry=|name:&str|->Result<([i32;2],i32),String> {
            let line=plan.lines().find(|line|line.starts_with(&format!("G\t{name}\t"))).ok_or("missing message geometry")?;
            let values=line.split('\t').skip(2).map(str::parse::<i32>).collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?;
            let [x,y,w]=values.as_slice() else {return Err("invalid message geometry".into())};
            Ok(([*x,*y],*w))
        };
        let (name_origin,name_width)=geometry("name")?;let (text_origin,text_width)=geometry("text")?;
        Ok(MessageBoxSnapshot::Classic {name,text,name_origin,name_width,text_origin,text_width})
    }else if plan.starts_with("M\tshared\n") {
        let policy=plan.lines().find(|line|line.starts_with("P\t")).ok_or("missing message frame policy")?;
        let values:Vec<_>=policy.split('\t').collect();
        let ["P",after,gated]=values.as_slice() else {return Err("invalid message frame policy".into())};
        Ok(MessageBoxSnapshot::Shared {base:take("base")?,frame:take("frame")?,name,text,
            frame_after_name:*after=="1",frame_requires_name:*gated=="1"})
    }else {Err("unknown message box contract".into())}
}

pub(crate) fn message_layers<R: std::io::Read+std::io::Seek>(
    vm:&mut nekopack_script::system::SystemVm,store:&mut crate::dat::DatGame<R>,
    variables:&mut nekopack_script::runtime::Variables,begin:bool,rest:bool,elapsed_delta:u32,
) -> Result<MessageLayers,String> {
    vm.eval_with_resources("message-layers",&format!("return NativeMessageLayerRun({begin},{rest},{elapsed_delta})"),variables,store)?;
    let plan=vm.result();
    let mut sprites=sprite_snapshots(&plan)?;
    Ok(MessageLayers {overlay:sprites.remove("overlay").ok_or("missing message overlay")?,keep_animating:plan.starts_with("D\t1\n")})
}

#[cfg(test)]
mod layer_tests {
    use super::*;
    use nekopack_script::system::SystemVm;

    #[test]
    fn box_snapshot_tracks_live_sprite_handles_and_geometry() {
        let mut vm=SystemVm::new();
        vm.eval("host",include_str!("../../nekopack-script/host/headless.nut")).unwrap();
        vm.eval("layers",&layer_adapter("com[\"\"] = function(args){\n}\n").unwrap()).unwrap();
        vm.eval("box",r#"
            local name=Sprite(70,20),text=Sprite(200,60)
            name.asset="system/first.mng:1";name.x=3;name.y=4
            text.asset="system/first.mng:2";text.x=5;text.y=30
            MsgBox <- {name=name,text=text,name_x=2,name_y=3,name_w=60,text_x=7,text_y=8,text_w=180}
        "#).unwrap();
        let before=message_box(&mut vm).unwrap();
        let MessageBoxSnapshot::Classic {name,text_origin,text_width,..}=&before else {panic!("classic box")};
        assert_eq!(name.asset.as_deref(),Some("system/first.mng:1"));
        assert_eq!(name.position,[3,4]);assert_eq!(*text_origin,[7,8]);assert_eq!(*text_width,180);
        vm.eval("change","MsgBox.name.asset=\"system/second.mng:7\";MsgBox.text_w=140").unwrap();
        let after=message_box(&mut vm).unwrap();
        assert_ne!(before,after);
        let MessageBoxSnapshot::Classic {name,text_width,..}=after else {panic!("classic box")};
        assert_eq!(name.asset.as_deref(),Some("system/second.mng:7"));assert_eq!(text_width,140);
    }

    #[test]
    fn hooks_exclude_comments_preserve_disabled_date_and_update_once() {
        let source=r#"
            com[""] = function(args){
                UpdateClimax()
                if(Config.Climax)s.Draw(ClimaxImage)
                // if(DrawDate(s))delay=0
                /* if(DrawDate(s))delay=0 */
            }
        "#;
        let mut vm=SystemVm::new();
        vm.eval("host",include_str!("../../nekopack-script/host/headless.nut")).unwrap();
        vm.eval("fixture",r#"
            Config <- {Climax=1}
            Count <- 0;DateCalls <- 0
            function UpdateClimax(){
                if(!InRestore())Count++
                ClimaxImage <- Sprite()
                ClimaxImage.asset="system/climax.mng:0"
                ClimaxImage.x=12;ClimaxImage.y=15
            }
            function DrawDate(s){DateCalls++;return}
        "#).unwrap();
        vm.eval("layers",&layer_adapter(source).unwrap()).unwrap();
        vm.eval("begin","return NativeMessageLayerRun(true,false,0)").unwrap();
        let overlay=sprite_snapshots(&vm.result()).unwrap().remove("overlay").unwrap();
        assert_eq!(overlay.layers.len(),1);assert_eq!(overlay.layers[0].position,[12,15]);
        vm.eval("next","return NativeMessageLayerRun(false,false,100)").unwrap();
        vm.eval("state","return format(\"%d %d\",Count,DateCalls)").unwrap();
        assert_eq!(vm.result(),"1 0");
        vm.eval("restored","return NativeMessageLayerRun(true,true,0)").unwrap();
        vm.eval("state","return Count.tostring()").unwrap();assert_eq!(vm.result(),"1");
        let mut active=source.replace("// if(DrawDate(s))delay=0","if(DrawDate(s))delay=0");
        active=active.replace("UpdateClimax()","");
        vm.eval("active layers",&layer_adapter(&active).unwrap()).unwrap();
        vm.eval("draw","return NativeMessageLayerRun(false,false,0)").unwrap();
        assert!(vm.result().starts_with("D\t0\n"));
        vm.eval("date","return DateCalls.tostring()").unwrap();assert_eq!(vm.result(),"1");
    }
}

pub(crate) fn voice_adapter(source: &str) -> Result<String, String> {
    const HANDLER: &str = "com[\"\"] = function(args){";
    const PARSED: &str = "local is_readed =";
    fn unique<'a>(source: &'a str, marker: &str) -> Result<usize, String> {
        let mut offsets = source.match_indices(marker).map(|(offset, _)| offset);
        let first = offsets
            .next()
            .ok_or_else(|| format!("message adapter missing {marker}"))?;
        if offsets.next().is_some() {
            return Err(format!("message adapter ambiguous {marker}"));
        }
        Ok(first)
    }
    let start = unique(source, HANDLER)? + HANDLER.len();
    let parsed = unique(source, PARSED)?;
    let voices: Vec<_> = ["local voice = IsAvailVoice(", "local voice = VoiceVolume("]
        .into_iter()
        .filter(|marker| source.contains(marker))
        .collect();
    if voices.len() != 1 {
        return Err("message adapter requires one supported voice declaration".into());
    }
    let voice = unique(source, voices[0])?;
    if !(start < parsed && parsed < voice) {
        return Err("message adapter boundaries are out of order".into());
    }
    // In the verified source contract this prefix only declares m and parses
    // args into local fields; MessageLog/global/UI writes follow PARSED.
    let parser = &source[start..parsed];
    let predicate = source[voice..].lines().next().unwrap();
    if !predicate
        .trim()
        .trim_end_matches(';')
        .trim_end()
        .ends_with(')')
    {
        return Err("message adapter voice declaration is not a complete statement".into());
    }
    Ok(format!(
        "function NativeMessageVoice(args){{\n{parser}\n{predicate}\nreturn voice ? true : false\n}}"
    ))
}

/// Capture parameters by executing the mounted parser and Glyph setup, rather
/// than duplicating size controls, name decoration or centering in Rust.
pub(crate) fn layout_adapter(source: &str) -> Result<String, String> {
    fn unique(source: &str, marker: &str) -> Result<usize, String> {
        let hits: Vec<_> = source.match_indices(marker).collect();
        if hits.len() != 1 {
            return Err(format!("message layout requires one {marker}"));
        }
        Ok(hits[0].0)
    }
    let start =
        unique(source, "com[\"\"] = function(args){")? + "com[\"\"] = function(args){".len();
    let parsed = unique(source, "local is_readed =")?;
    let glyph = unique(source, "local comp = Glyph()")?;
    let build = unique(source, "local list1  = comp.Build(m.text)")?
        + "local list1  = comp.Build(m.text)".len();
    if !(start < parsed && parsed < glyph && glyph < build) {
        return Err("message layout source boundaries out of order".into());
    }
    let parser = &source[start..parsed];
    let colors = source[parsed..glyph]
        .lines()
        .filter(|line| {
            let line = line.trim_start();
            [
                "local readed_color",
                "local text_color",
                "local border_color",
            ]
            .iter()
            .any(|declaration| {
                line.strip_prefix(declaration)
                    .is_some_and(|tail| tail.trim_start().starts_with('='))
            })
        })
        .collect::<Vec<_>>()
        .join("\n");
    let body_border = source[build..]
        .lines()
        .find(|line| line.trim_start().starts_with("local list2"))
        .filter(|line| line.contains("comp.Build2("))
        .unwrap_or("");
    // Squirrel strings here are UTF-8; the original engine used CP932 bytes.
    // Preserve its length calculation using the caller's original byte count.
    let setup = source[glyph..build].replace("m.name.len()", "name_byte_len");
    let width_setup = if setup.contains("= text_w") {
        let width = unique(source, "local text_w")?;
        source[width..]
            .lines()
            .next()
            .ok_or("missing message width declaration")?
    } else {
        ""
    };
    let layers=layer_adapter(source)?;
    Ok(format!(
        "{}\nfunction NativeMessageParse(args){{\n{parser}\nreturn NativeMessageFields(m)\n}}\n\
         function NativeMessageLayout(args,name_byte_len,is_readed){{\n{parser}\n\
         local box=::MsgBox; local name=NativeMessageLayoutSprite();\n\
         {width_setup}\n\
         {colors}\n\
         local Glyph=NativeMessageLayoutGlyph;\n{setup}\n{body_border}\n\
         local c=comp.capture;\n\
         return c.name_size+\",\"+c.name_x+\",\"+c.name_y+\",\"+c.text_size+\",\"+c.line_width+\",\"+c.line_span+\",\"+c.name_color+\",\"+c.name_border+\",\"+c.text_color+\",\"+c.text_border+\"\\n\"+NativeMessagePack(c.name_text)+NativeMessageFields(m)\n}}",
        format!("{}\n{layers}", include_str!("../host/message_layout.nut"))
    ))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ParsedMessage {
    pub name: String,
    pub voice: String,
    pub mask: String,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MessageLayout {
    pub message: ParsedMessage,
    pub name_text: String,
    pub name_size: i32,
    /// Coordinates local to the name sprite, exactly as passed to Glyph.Print.
    pub name_position: [i32; 2],
    pub text_size: i32,
    pub line_width: i32,
    pub line_span: i32,
    pub name_color: u32,
    pub name_border: Option<u32>,
    pub text_color: u32,
    pub text_border: Option<u32>,
}

fn take_field<'a>(input: &mut &'a str) -> Result<&'a str, String> {
    let (length, tail) = input
        .split_once(':')
        .ok_or("invalid message field length")?;
    let length: usize = length.parse().map_err(|_| "invalid message field length")?;
    let (field, tail) = tail
        .split_at_checked(length)
        .ok_or("invalid message field extent")?;
    *input = tail;
    Ok(field)
}
fn take_message(input: &mut &str) -> Result<ParsedMessage, String> {
    Ok(ParsedMessage {
        name: take_field(input)?.into(),
        voice: take_field(input)?.into(),
        mask: take_field(input)?.into(),
        text: take_field(input)?.into(),
    })
}
fn args_literal(args: &[String]) -> Result<String, String> {
    Ok(format!(
        "[{}]",
        args.iter()
            .map(|arg| crate::quote(arg.as_bytes()))
            .collect::<Result<Vec<_>, _>>()?
            .join(",")
    ))
}
pub(crate) fn parse_message(
    vm: &mut nekopack_script::system::SystemVm,
    args: &[String],
) -> Result<ParsedMessage, String> {
    vm.eval(
        "message-parse",
        &format!("return NativeMessageParse({})", args_literal(args)?),
    )
    .map_err(|e| e.to_string())?;
    let result = vm.result();
    let mut result = result.as_str();
    let message = take_message(&mut result)?;
    if !result.is_empty() {
        return Err("unexpected message parser fields".into());
    }
    Ok(message)
}
pub(crate) fn evaluate_layout(
    vm: &mut nekopack_script::system::SystemVm,
    args: &[String],
    name_byte_len: usize,
    is_readed: bool,
) -> Result<MessageLayout, String> {
    vm.eval(
        "message-layout",
        &format!(
            "return NativeMessageLayout({},{name_byte_len},{is_readed})",
            args_literal(args)?
        ),
    )
    .map_err(|e| e.to_string())?;
    let result = vm.result();
    let (header, mut fields) = result
        .split_once('\n')
        .ok_or("missing message layout header")?;
    let values = header
        .split(',')
        .map(str::parse::<i32>)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "invalid message layout header")?;
    let [
        name_size,
        x,
        y,
        text_size,
        line_width,
        line_span,
        name_color,
        name_border,
        text_color,
        text_border,
    ] = values.as_slice()
    else {
        return Err("invalid message layout field count".into());
    };
    let name_text: String = take_field(&mut fields)?.into();
    let message = take_message(&mut fields)?;
    if !fields.is_empty() {
        return Err("unexpected message layout fields".into());
    }
    if *text_size <= 0
        || *line_width <= 0
        || text_size
            .checked_add(*line_span)
            .is_none_or(|height| height <= 0)
        || (!name_text.is_empty() && *name_size <= 0)
    {
        return Err("invalid script message layout extent".into());
    }
    let color = |value: i32| -> Result<u32, String> {
        if !(0..=0xffffff).contains(&value) {
            return Err("invalid script glyph color".into());
        }
        Ok(value as u32)
    };
    let border = |value: i32| -> Result<Option<u32>, String> {
        if value == -1 {
            Ok(None)
        } else {
            color(value).map(Some)
        }
    };
    Ok(MessageLayout {
        name_color: color(*name_color)?,
        name_border: border(*name_border)?,
        text_color: color(*text_color)?,
        text_border: border(*text_border)?,
        message,
        name_text,
        name_size: *name_size,
        name_position: [*x, *y],
        text_size: *text_size,
        line_width: *line_width,
        line_span: *line_span,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use nekopack_script::system::SystemVm;

    fn layout_vm(source: &str) -> SystemVm {
        layout_vm_with_constants(source, "")
    }
    fn layout_vm_with_constants(source: &str, constants: &str) -> SystemVm {
        let mut vm = SystemVm::new();
        vm.eval(
            "synthetic-layout-host",
            r#"
            Config <- {Font="synthetic",Bold=false,Border=true}
            MsgBox <- {name_w=200,name_x=7,name_y=9,text_w=500,text={width=600}}
        "#,
        )
        .unwrap();
        vm.eval("original-color-constants", constants).unwrap();
        vm.eval("original-layout-adapter", &layout_adapter(source).unwrap())
            .unwrap();
        vm
    }

    #[test]
    fn original_color_expressions_follow_read_state_and_option_for_name_and_body() {
        let source = r#"com[""] = function(args){
local m={size=24,name="Unit",voice="",mask="",text="probe"}
local is_readed = IsReaded()
local readed_color = Config.ReadedColor && is_readed
local text_color = readed_color ? READED_TEXT_COLOR : NORMAL_TEXT_COLOR
local border_color = readed_color ? READED_BORDER_COLOR : NORMAL_BORDER_COLOR
local comp = Glyph()
comp.Color=text_color
if(m.name.len()){comp.Size=24;comp.Print(name,m.name,0,0,border_color)}
comp.Size=m.size;comp.LineWidth=500;comp.LineSpan=8
local list1  = comp.Build(m.text)
local list2 = Config.Border ? comp.Build2(border_color) : []
}"#;
        let mut vm = layout_vm(source);
        vm.eval(
            "synthetic-colors",
            r#"
            Config.ReadedColor <- 1;
            NORMAL_TEXT_COLOR <- 0x123456; NORMAL_BORDER_COLOR <- 0x654321;
            READED_TEXT_COLOR <- 0xabcdef; READED_BORDER_COLOR <- 0xfedcba;
        "#,
        )
        .unwrap();
        for (read, enabled, fill, border) in [
            (false, 1, 0x123456, 0x654321),
            (true, 1, 0xabcdef, 0xfedcba),
            (true, 0, 0x123456, 0x654321),
        ] {
            vm.eval(
                "read-color-option",
                &format!("Config.ReadedColor={enabled}"),
            )
            .unwrap();
            let layout = evaluate_layout(&mut vm, &[], 4, read).unwrap();
            assert_eq!((layout.name_color, layout.text_color), (fill, fill));
            assert_eq!(
                (layout.name_border, layout.text_border),
                (Some(border), Some(border))
            );
        }
        vm.eval("no-border", "Config.Border=false").unwrap();
        assert_eq!(
            evaluate_layout(&mut vm, &[], 4, false).unwrap().text_border,
            None
        );
        vm.eval("missing-original-constant", "delete NORMAL_TEXT_COLOR")
            .unwrap();
        assert!(evaluate_layout(&mut vm, &[], 4, false).is_err());
    }

    #[test]
    fn layout_executes_parser_sizes_name_decoration_and_spacing() {
        let source = r#"com[""] = function(args){
local m={size=30,name="",voice="",mask="",text=""}
foreach(line in args){
 if(line[0]==':'){
  local p=split(line,":");p.append("");p.append("");p.append("")
  if(p[0]=="size")m.size=p[1]=="big"?42:p[1].tointeger()
  else {m.name=p[0];m.voice=p[1];m.mask=p[2]}
 }else {if(m.text.len())m.text+="\n";m.text+=line}
}
local is_readed = IsReaded()
local text_color=0xffffff; local border_color=0;
local comp = Glyph()
comp.Face=Config.Font;comp.Bold=Config.Bold;comp.Color=text_color
if(m.name.len()){comp.Reset();comp.Size=31;local name_t="【"+m.name+"】";comp.Print(name,name_t,2,3,border_color)}
comp.Reset();comp.Size=m.size;comp.LineWidth=box.text.width;comp.LineSpan=11
local list1  = comp.Build(m.text)
}"#;
        let mut vm = layout_vm(source);
        let args = vec![
            ":Synthetic:unit:XX".into(),
            ":size:big".into(),
            "alpha".into(),
            "beta".into(),
        ];
        let parsed = parse_message(&mut vm, &args).unwrap();
        assert_eq!(parsed.name, "Synthetic");
        assert_eq!(parsed.text, "alpha\nbeta");
        let layout = evaluate_layout(&mut vm, &args, 9, false).unwrap();
        assert_eq!(layout.message, parsed);
        assert_eq!(
            (
                layout.name_text.as_str(),
                layout.name_size,
                layout.name_position
            ),
            ("【Synthetic】", 31, [2, 3])
        );
        assert_eq!(
            (layout.text_size, layout.line_width, layout.line_span),
            (42, 600, 11)
        );
        assert!(layout_adapter(&source.replace("local list1  =", "local bad =")).is_err());
    }

    #[test]
    #[ignore = "requires local original system sources; only system code and synthetic message inputs"]
    fn six_original_layouts_preserve_their_own_parameters() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.local");
        let sources = (0..4)
            .map(|i| root.join(format!("title-verified/{i}/system/message.nut")))
            .chain([
                root.join("dialogue-batch31/message.nut"),
                root.join("sorakoi-import/analysis/message.nut"),
            ]);
        for (index, path) in sources.enumerate() {
            let source = std::fs::read_to_string(&path).unwrap();
            let config = std::fs::read_to_string(path.with_file_name("config.nut")).unwrap();
            let constants = config
                .lines()
                .filter(|line| line.trim_start().starts_with("const "))
                .collect::<Vec<_>>()
                .join("\n");
            let mut vm = layout_vm_with_constants(&source, &constants);
            vm.eval("read-color-test-option", "Config.ReadedColor <- 1")
                .unwrap();
            let args = vec![":Synthetic:unit:XX".into(), "probe".into()];
            let layout = evaluate_layout(&mut vm, &args, 4, false).unwrap();
            assert_eq!(layout.text_size, if index == 5 { 30 } else { 22 });
            assert_eq!(layout.line_span, 8);
            assert_eq!(
                layout.name_position,
                if index == 5 { [0, 0] } else { [85, 9] }
            );
            assert_eq!(
                layout.name_text,
                if index == 5 {
                    "【Synthetic】"
                } else {
                    "Synthetic"
                }
            );
            for (control, expected) in [
                ("bigbig", if index == 5 { 86 } else { 60 }),
                ("big", if index == 5 { 42 } else { 36 }),
                ("small", if index == 5 { 18 } else { 16 }),
                ("27", 27),
            ] {
                let args = vec![
                    ":Synthetic:unit:XX".into(),
                    format!(":size:{control}"),
                    "probe".into(),
                ];
                let layout = evaluate_layout(&mut vm, &args, 4, false).unwrap();
                assert_eq!(layout.text_size, expected);
            }
        }
    }

    fn fixture(field: &str) -> String {
        format!(
            r#"com[""] = function(args){{
local m = {{name="",voice="",mask="",text=""}}
foreach(line in args){{
    if(line[0]==':'){{
        local p=split(line,":")
        p.append("");p.append("");p.append("")
        m.name=p[0];m.voice=p[1];m.mask=p[2]
    }}else{{m.text+=line}}
}}
local is_readed = IsReaded()
local voice = IsAvailVoice(m.{field})
Presentation()
}}"#
        )
    }

    #[test]
    fn original_parser_and_predicate_choose_their_own_input() {
        for (field, expected) in [("voice", false), ("mask", true)] {
            let mut vm = SystemVm::new();
            vm.eval(
                "config",
                r#"
Config <- {VoiceCheck={AA=false,BB=true,CC=false,XX=false}}
function IsAvailVoice(input){
    foreach(tag in split(input," \t")){
        if(!(tag in Config.VoiceCheck))tag="XX"
        if(Config.VoiceCheck[tag])return true
    }
    return false
}"#,
            )
            .unwrap();
            vm.eval("adapter", &voice_adapter(&fixture(field)).unwrap())
                .unwrap();
            vm.eval(
                "speaker",
                r#"return NativeMessageVoice([":Speaker:AA:CC BB"]).tostring()"#,
            )
            .unwrap();
            assert_eq!(vm.result(), expected.to_string());
        }
    }

    #[test]
    fn malformed_or_duplicate_contract_is_rejected() {
        assert!(voice_adapter("").is_err());
        let source = fixture("voice");
        assert!(voice_adapter(&format!("{source}\n{source}")).is_err());
        assert!(
            voice_adapter(&source.replace("IsAvailVoice(m.voice)", "IsAvailVoice(m.voice"))
                .is_err()
        );
    }

    #[test]
    fn volume_predicate_preserves_original_zero_and_positive_decisions() {
        let source = fixture("mask").replace("IsAvailVoice(m.mask)", "VoiceVolume(m.mask)");
        let mut vm = SystemVm::new();
        vm.eval(
            "volume",
            "function VoiceVolume(mask){return mask==\"ON\" ? 70 : 0}",
        )
        .unwrap();
        vm.eval("adapter", &voice_adapter(&source).unwrap())
            .unwrap();
        for (mask, expected) in [("ON", "true"), ("OFF", "false")] {
            vm.eval(
                "synthetic",
                &format!("return NativeMessageVoice([\":Synthetic:unit:{mask}\"]).tostring()"),
            )
            .unwrap();
            assert_eq!(vm.result(), expected);
        }
    }

    #[test]
    #[ignore = "requires five original game directories in NEKO_VOICE_CODE_TEST_DIRS; system and boot code only"]
    fn five_original_message_parsers_execute_their_own_voice_rules() {
        use crate::{GameResources, dat::DatGame};
        let paths = std::env::var_os("NEKO_VOICE_CODE_TEST_DIRS").unwrap();
        let paths: Vec<_> = std::env::split_paths(&paths).collect();
        assert_eq!(paths.len(), 5);
        let mut name_profiles = 0;
        let mut mask_profiles = 0;
        for root in paths {
            let mut store = DatGame::mount_named(
                std::fs::read_dir(&root)
                    .unwrap()
                    .map(|entry| entry.unwrap().path())
                    .filter(|path| {
                        matches!(
                            path.file_name().and_then(|name| name.to_str()),
                            Some("system.dat" | "script.dat")
                        )
                    })
                    .map(|path| {
                        (
                            path.file_name().unwrap().to_str().unwrap().to_owned(),
                            std::fs::File::open(path).unwrap(),
                        )
                    }),
                true,
            )
            .unwrap();
            let source = String::from_utf8(store.system(b"message.nut").unwrap()).unwrap();
            let (_, mut vm) =
                crate::bootstrap_begin(&mut store, 1, &mut std::io::sink(), true).unwrap();
            // These are source-contract assertions for the two verified original
            // variants, not production profile selection or duplicated filters.
            let mask_input = source.contains("local voice = IsAvailVoice(m.mask)");
            if mask_input {
                mask_profiles += 1;
            } else {
                assert!(source.contains("local voice = IsAvailVoice(m.voice)"));
                name_profiles += 1;
            }
            for (xx, ve) in [(false, false), (false, true), (true, false), (true, true)] {
                vm.eval("voice-controls", &format!("foreach(tag,value in Config.VoiceCheck)Config.VoiceCheck[tag]=false; Config.VoiceCheck.XX={xx}; Config.VoiceCheck.VE={ve};")).unwrap();
                vm.eval(
                    "synthetic-speaker",
                    r#"return NativeMessageVoice([":Synthetic:XX_unit:VE"]).tostring()"#,
                )
                .unwrap();
                assert_eq!(
                    vm.result(),
                    (if mask_input { ve } else { xx }).to_string(),
                    "{}",
                    root.display()
                );
            }
            // ED's mask handler must retain its any-enabled-tag behavior, and
            // the name handler must continue ignoring mask contents.
            vm.eval("multi-mask", r#"Config.VoiceCheck.VE=true;Config.VoiceCheck.XX=false;return NativeMessageVoice([":Synthetic:XX_unit:UNKNOWN VE"]).tostring()"#).unwrap();
            assert_eq!(vm.result(), mask_input.to_string());
        }
        assert_eq!((name_profiles, mask_profiles), (4, 1));
    }
}
