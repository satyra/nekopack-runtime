use super::*;

fn hex(text:&str)->String {text.bytes().map(|byte|format!("{byte:02x}")).collect()}
fn parsed(plan:&str)->ScriptView {
    ScriptView {draws:transition_draws(&hex(plan)).unwrap(),..Default::default()}
}
fn pixel(image:&RgbaImage,x:usize,y:usize)->&[u8] {
    &image.pixels()[(y*image.width() as usize+x)*4..][..4]
}

#[test]
fn composite_clips_its_sized_canvas_and_preserves_transparent_margin(){
    let mut view=parsed(&format!("O\t255\t256\nJ\t3\t2\t1\t1\t255\t{}\n",
        hex("A\tfixture\t-1\t0\t255\n")));
    view.assets.insert("fixture".into(),RgbaImage::filled(2,1,[255,0,0,255],1024).unwrap());
    let image=view.render([5,4],None).unwrap();
    assert_eq!(pixel(&image,1,1),[255,0,0,255]);
    for (x,y) in [(0,1),(2,1),(3,1),(1,2),(4,3)] {
        assert_eq!(pixel(&image,x,y),[0,0,255,255],"pixel {x},{y}");
    }
    assert_eq!(draw_assets(&view.draws),["fixture"]);
}

#[test]
fn nested_composite_uses_local_coordinates_and_native_opacity(){
    let inner=format!("J\t1\t1\t1\t0\t255\t{}\n",hex("F\t16711680\t255\n"));
    let view=parsed(&format!("O\t255\t256\nJ\t2\t1\t1\t1\t128\t{}\n",hex(&inner)));
    let image=view.render([4,3],None).unwrap();
    assert_eq!(pixel(&image,2,1),[126,0,128,255]);
    assert_eq!(pixel(&image,1,1),[0,0,255,255]);
    assert_eq!(pixel(&image,3,1),[0,0,255,255]);
}

#[test]
fn sized_fill_and_offscreen_composite_stay_inside_the_parent(){
    let view=parsed(&format!("O\t255\t256\nJ\t2\t2\t-1\t-1\t255\t{}\n",hex("O\t65280\t256\n")));
    let image=view.render([2,2],None).unwrap();
    assert_eq!(pixel(&image,0,0),[0,255,0,255]);
    assert_eq!(pixel(&image,1,0),[0,0,255,255]);
    assert_eq!(pixel(&image,0,1),[0,0,255,255]);
}

#[test]
fn glyph_protocol_accepts_positive_sizes_and_encoded_tabs_unicode(){
    for size in [1,18,22,25,28,32,45] {
        let draw=styled_text("3","4",&size.to_string(),"4482815","1","-1",&hex("字\tX")).unwrap();
        match draw {
            Draw::StyledText(text,pos,actual,color,bold,border)=>{
                assert_eq!(text,"字\tX");assert_eq!(pos,[3,4]);assert_eq!(actual,size);
                assert_eq!(color,0x4466ff);assert!(bold);assert_eq!(border,None);
            }
            _=>panic!("expected styled text"),
        }
    }
    for (size,bold,border,text) in [("0","0","-1","41"),("-1","0","-1","41"),("22","2","-1","41"),("22","0","-2","41"),("22","0","-1","字字")] {
        assert!(styled_text("0","0",size,"0",bold,border,text).is_err());
    }
}

#[test]
fn cp932_resource_text_crosses_the_existing_utf8_vm_boundary_once(){
    // DatGame's system() and Data resource adapters decode CP932 before
    // providing UTF-8 source/literals to the VM. G hex encodes those VM bytes.
    let (cp932,_,errors)=encoding_rs::SHIFT_JIS.encode("音楽");
    assert!(!errors);
    let vm_text=crate::dat::decode(&cp932).unwrap();
    let view=parsed(&format!("G\t0\t0\t25\t0\t0\t-1\t{}\n",hex(&vm_text)));
    match &view.draws[0] {
        Draw::StyledText(text,..)=>assert_eq!(text,"音楽"),
        _=>panic!("expected styled glyph"),
    }
    let raw_hex=cp932.iter().map(|byte|format!("{byte:02x}")).collect::<String>();
    assert!(styled_text("0","0","25","0","0","-1",&raw_hex).is_err());
}

#[test]
fn styled_glyph_raster_uses_size_color_weight_and_border(){
    let Some(path)=std::env::var_os("NEKO_TEST_FONT") else {return};
    let font=crate::text_render::load_font(std::fs::read(path).unwrap(),0).unwrap();
    for size in [18,22,25,28,32] {
        let render=|bold,border|{
            parsed(&format!("G\t2\t2\t{size}\t16711680\t{bold}\t{border}\t{}\n",hex("MW")))
                .render([120,60],Some(&font)).unwrap()
        };
        let normal=render(0,-1);
        let bold=render(1,-1);
        let border=render(0,65280);
        assert!(normal.pixels().chunks_exact(4).any(|p|p[0]>0 && p[1]==0 && p[2]==0));
        assert_ne!(normal.pixels(),bold.pixels());
        assert!(border.pixels().chunks_exact(4).any(|p|p[1]>0));
    }
}

#[test]
fn pixel_alpha_replaces_alpha_without_changing_rgb_or_original_assets(){
    let source="A\tcolor\t0\t0\t255\n";
    let mask="A\tmask\t0\t0\t255\n";
    let mut view=parsed(&format!("D\t2\t1\talpha\t2\t1\t{}\t{}\n",hex(source),hex(mask)));
    view.assets.insert("color".into(),RgbaImage::from_rgba(2,1,vec![120,80,40,30, 5,10,15,200],1024).unwrap());
    view.assets.insert("mask".into(),RgbaImage::from_rgba(1,1,vec![255,0,0,128],1024).unwrap());
    let image=view.render_draws_on(&view.draws,[2,1],None,None,None,[0;4]).unwrap();
    assert_eq!(image.pixels(),[120,80,40,128, 5,10,15,0]);
    assert_eq!(view.assets["color"].pixels(),[120,80,40,30,5,10,15,200]);
    assert_eq!(draw_assets(&view.draws),["color","mask"]);
}

#[test]
fn reduction_uses_native_fixed_point_rgb_weights_and_opaque_output(){
    let source=RgbaImage::from_rgba(3,1,vec![0,10,20,0, 90,100,110,128, 240,250,255,255],1024).unwrap();
    let image=native_reduction(&source,[2,1]).unwrap();
    // The middle pixel participates in both output samples, using the native
    // quantized 170/256 source pixel width rather than floating point weights.
    assert_eq!(image.pixels(),[30,40,50,255, 140,150,158,255]);
    let mut view=parsed(&format!("D\t2\t1\treduction\t3\t1\t{}\t\n",hex("Z\t1000\n")));
    view.saved_screens.insert(1000,Rc::new(source));
    assert_eq!(view.render([2,1],None).unwrap().pixels(),image.pixels());
}
