// Native room operations for the existing SystemCommand coroutine. The original
// com["room"] and GUI classes retain their own calls, loops, and return stack.
SystemRoomHost <- {music_playing=false,replay_pending=false}

function SystemRoomMusicState(playing){
    if(typeof playing!="bool")throw "invalid original room music state"
    SystemRoomHost.music_playing=playing
}
function SystemRoomPlayMusic(name,looped=true,volume=100){
    if(typeof name!="string" || !name.len() || name.find("/")!=null || name.find("\\")!=null || name.find("\r")!=null || name.find("\n")!=null || name.find("\t")!=null)throw "invalid original room music name"
    if(typeof looped!="bool" || typeof volume!="integer" || volume<0 || volume>100)throw "invalid original room music parameters"
    SystemCommandHost.actions.append(format("room_music_play\t%s\t%d\t%d",SystemCommandHex(name),looped?1:0,volume))
    SystemRoomHost.music_playing=true
}
function SystemRoomStopMusic(){
    SystemCommandHost.actions.append("room_music_stop")
    SystemRoomHost.music_playing=false
}
function SystemRoomVolumeMusic(volume){
    if(typeof volume!="integer" || volume<0 || volume>100)throw "invalid original room music volume"
    SystemCommandHost.actions.append(format("room_music_volume\t%d",volume))
}
function SystemRoomIsPlayMusic(){return SystemRoomHost.music_playing}

// Use the same font contract as every other original system page.
class SystemRoomGlyph extends SystemGuiGlyph {}

function SystemRoomImageAction(words){
    SystemCommandHost.actions.append("room_image\t"+SystemCommandHex(words))
    SystemRoomNativeBoundary()
}
function SystemRoomNativeBoundary(){
    SystemCommandHost.host_pending=true
    SystemCommandHost.delay=-1
    suspend()
}
class SystemRoomScreen extends SystemCommandScreen {
    function Commit(){
        ::SystemCommandHost.actions.append("room_commit")
        ::SystemRoomNativeBoundary()
    }
    function Render(){
        ::SystemCommandHost.current=[["C"]]
        layers=::SystemCommandHost.current
        return false
    }
}
function SystemRoomAssignImage(name){
    SystemCommandHost.actions.append("room_assign_image\t"+SystemCommandHex(name))
}
function SystemRoomScriptReset(){SystemCommandHost.actions.append("room_script_reset")}
function SystemRoomScriptEval(name){
    // Metadata inspection stops here. Production explicitly enters the existing
    // replay runner and later restores this same coroutine's host table.
    SystemCommandHost.actions.append("room_replay\t"+SystemCommandHex(name))
    SystemRoomHost.replay_pending=true
    SystemCommandHost.host_pending=true
    SystemCommandHost.delay=-1
    do{suspend()}while(SystemRoomHost.replay_pending)
}
function SystemRoomReplayComplete(){
    if(!SystemRoomHost.replay_pending)throw "no original room replay to complete"
    SystemRoomHost.replay_pending=false
    SystemCommandHost.host_pending=true
}
function SystemRoomBindings(){
    return {
        PlayMusic=SystemRoomPlayMusic,StopMusic=SystemRoomStopMusic,
        VolumeMusic=SystemRoomVolumeMusic,IsPlayMusic=SystemRoomIsPlayMusic,
        Glyph=SystemRoomGlyph,Screen=SystemRoomScreen,ImageAction=SystemRoomImageAction,
        AssignImage=SystemRoomAssignImage,ScriptReset=SystemRoomScriptReset,
        ScriptEval=SystemRoomScriptEval
    }
}
