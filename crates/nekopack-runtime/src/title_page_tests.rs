use nekopack_script::system::SystemVm;

#[test]
fn title_coroutine_keeps_its_state_and_restores_host_between_steps() {
    let mut vm = SystemVm::new();
    vm.eval(
        "host",
        include_str!("../../nekopack-script/host/headless.nut"),
    )
    .unwrap();
    vm.eval(
        "synthetic-system",
        r#"
        class GUI {}
        Config <- {VolSYS=60}
        ID_OK <- 1
        com["title"] = function(args,rows){
            local f=Images("title")
            local spots=Spots(f)
            spots.BindR("START",1)
            local screen=Screen()
            local timer=Timer()
            wait(0)
            do {
                local ev=event()
                local id=spots.Check()
                screen.Draw(f.BG,0,0,(timer.Get()-500)/4)
                spots.Draw(screen)
                screen.Snap()
                screen.Update()
                if(ev==ID_OK && id)return id
            }while(wait(timer.Get()<3000?1:-1))
        }
    "#,
    )
    .unwrap();
    vm.eval("config-page-host", include_str!("../host/config_page.nut"))
        .unwrap();
    vm.eval("title-page-host", include_str!("../host/title_page.nut"))
        .unwrap();
    vm.eval(
        "install",
        r#"
        SavedConfigPage <- ConfigPageHost
        SavedScreen <- Screen
        TitlePageInstall({title={BG=["system/title.mng:0",0,0,100,100],
            ["= START"]=["system/title.mng:1",10,20,30,40]}},[],["START"])
        return TitlePageStep(0,-1,-1,0)
    "#,
    )
    .unwrap();
    assert!(vm.result().contains("B\t1:1\t10\t20\t30\t40"));
    assert!(vm.result().contains("A\tsystem/title.mng:0\t0\t0\t0"));
    vm.eval(
        "other-coroutine-state",
        r#"
        ConfigPageHost <- {other=true}
        return TitlePageStep(0,-1,-1,3000)
    "#,
    )
    .unwrap();
    assert!(vm.result().contains("P\t1\t0\t-1"));
    assert!(vm.result().contains("A\tsystem/title.mng:0\t0\t0\t255"));
    vm.eval("click", "return TitlePageStep(ID_OK,15,25,0)")
        .unwrap();
    assert_eq!(vm.result(), "closed:1");
    vm.eval(
        "restored",
        "return (ConfigPageHost.other && Screen==SavedScreen).tostring()",
    )
    .unwrap();
    assert_eq!(vm.result(), "true");
}
