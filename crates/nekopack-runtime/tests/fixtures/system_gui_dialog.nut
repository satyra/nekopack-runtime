// Synthetic caller/resource probe for the original ViewDialog call contract.
// No game text or artwork. The caller resumes after the bool, not at the last
// confirmation click: the original function's final wait(0) is preserved.
function ViewDialog(name="CONFIRM"){
    if((name in Config.Dialog) && !Config.Dialog[name])return true
    sound("OK")
    local images=Images("dialog")
    local base=Sprite();base.Screen();base.Fill(0,128)
    base.Draw(images["BASE"]);base.Draw(images[name])
    local spots=Spots(images)
    spots.BindR("OK",ID_OK);spots.BindR("CANCEL",ID_CANCEL)
    if(Config.AutoSelect)spots.Forcus(ID_OK)
    local s=Screen(),old=spots.Check()
    wait(0)
    do{
        local id=spots.Check(),ev=event()
        if(old!=id){old=id;if(old)sound("ON_MOUSE")}
        if(ev==ID_CANCEL){sound("NO");wait(0);return false}
        if(ev==ID_OK){
            if(id==ID_CANCEL){sound("NO");wait(0);return false}
            if(id==ID_OK){sound("OK");wait(0);return true}
        }
        s.Draw(base);spots.Draw(s);s.Update()
    }while(wait(-1))
    wait(0)
    return false
}
AfterDialog <- 0
DialogResult <- null
com["dialog_probe"] = function(args){
    DialogResult=ViewDialog(args[0])
    AfterDialog++
}
