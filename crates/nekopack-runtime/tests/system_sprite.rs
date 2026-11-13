//! Synthetic sprite snapshots, independent of mounted game images.
use nekopack_script::system::SystemVm;

fn host()->SystemVm {
    let mut vm=SystemVm::new();
    vm.eval("command",include_str!("../host/system_command.nut")).unwrap();
    vm.eval("synthetic file",r#"
        class SystemLoadFileHandle {at=0}
        Thumbnail <- SystemCommandSprite(3,2)
        Thumbnail.layers=[["Z",1000]]
        function SystemLoadThumbnail(file){
            if(!(file instanceof SystemLoadFileHandle) || file.at!=0)throw "thumbnail read order"
            file.at=1
            return SystemCommandSprite(Thumbnail)
        }
    "#).unwrap();
    vm
}

#[test]
fn file_constructor_and_loadfile_keep_real_dimensions_and_independent_storage(){
    let mut vm=host();
    vm.eval("load",r#"
        local a=SystemCommandSprite(SystemLoadFileHandle())
        local b=SystemCommandSprite(99,77);b.LoadFile(SystemLoadFileHandle())
        a.Fill(0xff0000);b.Fill(0x00ff00)
        if(a.width!=3 || a.height!=2 || b.width!=3 || b.height!=2)throw "file dimensions"
        return SystemCommandLines(Thumbnail.layers)
    "#).unwrap();
    assert_eq!(vm.result(),"Z\t1000\n");
}

#[test]
fn masked_thumbnail_draw_keeps_a_snapshot_before_hover_fill(){
    let mut vm=host();
    vm.eval("mask and hover",r#"
        local image=SystemCommandSprite(SystemLoadFileHandle()),mask=SystemCommandSprite(3,2)
        mask.asset="synthetic:0";mask.x=50;mask.y=60
        image.Alpha(mask)
        local normal=SystemCommandSprite(8,8)
        normal.Draw(image,2,3)
        local before=SystemCommandLines(normal.layers)
        image.Fill(0x8888ff,64)
        mask.Fill(0)
        if(before!=SystemCommandLines(normal.layers))throw "hover mutated normal"
        if(image.layers[0][6][0][0]!="Z" || image.layers[0][7][0][2]!=0)throw "nonlocal pixel source"
        return before
    "#).unwrap();
    assert!(vm.result().starts_with("J\t3\t2\t2\t3\t255\t"));
}

#[test]
fn reduction_captures_source_plan_and_accepts_implicit_screen_dimensions(){
    let mut vm=host();
    vm.eval("reduction",r#"
        local screen=SystemCommandSprite();screen.Snap()
        local target=SystemCommandSprite(2,1)
        target.Reduction(screen)
        local before=SystemCommandLines(target.layers)
        screen.Fill(0)
        if(before!=SystemCommandLines(target.layers))throw "reduction source changed"
        return target.layers[0][3]
    "#).unwrap();
    assert_eq!(vm.result(),"reduction");
}
