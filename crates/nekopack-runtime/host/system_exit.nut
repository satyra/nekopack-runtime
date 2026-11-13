// QuitGame returns to the mounted caller, whose throw null unwinds its coroutine.
// The explicit action lets SystemCommandStep distinguish that unwind from errors.
function SystemExitQuitGame(){
    SystemCommandHost.terminal_action="quit"
    SystemCommandHost.actions.append("quit")
}
function SystemExitBindings(){return {QuitGame=SystemExitQuitGame}}
