// Virtual read-only save files for mounted SaveInfo/LoadData handlers.
// The host supplies decoded runtime records, not original .sav byte streams.
SystemLoadHost <- {records=[],catalog_ready=false,catalog_pending=false,pending=null,acknowledged=false,accepted=false,exit_state=null}
class SystemLoadFileHandle {
    record=null;at=-1
    constructor(value){record=value}
}
function SystemLoadInstall(records){
    foreach(record in records)if("metadata" in record){
        record.values <- SystemLoadProject(record.metadata)
        record.rawdelete("metadata")
    }
    SystemLoadHost.records=records
    SystemLoadHost.catalog_ready=true;SystemLoadHost.catalog_pending=false
    SystemLoadHost.pending=null;SystemLoadHost.acknowledged=false
    SystemLoadHost.accepted=false;SystemLoadHost.exit_state=null
}
class SystemLoadCollector {
    values=null;thumbnail=false
    constructor(){values=[]}
}
class SystemLoadCollectorThumbnail {
    function SaveFile(file){
        if(!(file instanceof SystemLoadCollector) || file.thumbnail || file.values.len())throw "invalid original thumbnail save order"
        file.thumbnail=true
    }
}
function SystemLoadCollect(file,value){
    if(!(file instanceof SystemLoadCollector) || !file.thumbnail)throw "original metadata must follow thumbnail"
    file.values.append(SystemLoadCopyValue(value))
}
function SystemLoadDimensions(){
    local root=getroottable(),previous=root.Sprite
    root.Sprite=SystemCommandSprite
    local size=null,failure=null,caught=false
    try{local info=SaveInfo();size=info.thumb.width+","+info.thumb.height}catch(error){caught=true;failure=error}
    root.Sprite=previous
    if(caught)throw failure
    return size
}
// The mounted writer defines its own sequence; the adapter supplies semantics,
// never a game-id branch or a guessed number of Load calls.
function SystemLoadProject(metadata){
    local root=getroottable(),bindings={Sprite=SystemCommandSprite,Save=SystemLoadCollect},saved={}
    foreach(key,value in metadata.globals)bindings.rawset(key,value)
    foreach(key,value in bindings){saved.rawset(key,[key in root,key in root?root[key]:null]);root.rawset(key,value)}
    local file=SystemLoadCollector(),failure=null,caught=false
    try{
        local info=SaveInfo()
        foreach(key,value in metadata.fields)if(key in info)info[key]=value
        info.thumb=SystemLoadCollectorThumbnail()
        info.Save(file)
        if(!file.thumbnail)throw "original SaveInfo did not write a thumbnail"
    }catch(error){caught=true;failure=error}
    foreach(key,pair in saved){if(pair[0])root.rawset(key,pair[1]);else root.rawdelete(key)}
    if(caught)throw failure
    return file.values
}
function SystemLoadReset(){
    SystemLoadHost.records=[];SystemLoadHost.catalog_ready=false;SystemLoadHost.catalog_pending=false
    SystemLoadHost.pending=null;SystemLoadHost.exit_state=null
}
function SystemLoadEnsureCatalog(){
    if(SystemLoadHost.catalog_ready)return
    if(SystemLoadHost.catalog_pending)throw "save catalog already pending"
    SystemLoadHost.catalog_pending=true
    SystemCommandHost.host_pending=true
    SystemCommandHost.actions.append("load_catalog")
    SystemCommandHost.delay=-1;suspend()
    if(!SystemLoadHost.catalog_ready)throw "save catalog acknowledgement required"
}
function SystemLoadFile(path,mode){
    if(mode!="rb")return SystemCommandFile(path,mode)
    SystemLoadEnsureCatalog()
    foreach(record in SystemLoadHost.records)if(record.path==path){
        if("error" in record)throw record.error
        return SystemLoadFileHandle(record)
    }
    throw "save file does not exist"
}
function SystemLoadEnumFiles(path,pattern){
    SystemLoadEnsureCatalog()
    if(path!=SavePath("") || pattern!="save*.sav")throw "unsupported original save enumeration"
    local records=[]
    foreach(record in SystemLoadHost.records){
        if(record.path.slice(0,path.len())!=path)continue
        local name=record.path.slice(path.len())
        if(name.len()<8 || name.slice(0,4)!="save" || name.slice(name.len()-4)!=".sav")continue
        if(name.find("/")!=null || name.find("\\")!=null)continue
        records.append(record)
    }
    records.sort(function(a,b){return a.modified>b.modified ? -1 : a.modified<b.modified ? 1 : 0})
    local result=[]
    foreach(record in records)result.append(record.path.slice(path.len()))
    return result
}
function SystemLoadThumbnail(file){
    if(!(file instanceof SystemLoadFileHandle) || file.at!=-1)throw "save thumbnail must be read first"
    file.at=0
    return file.record.thumbnail
}
function SystemLoadValue(file){
    if(!(file instanceof SystemLoadFileHandle) || file.at<0 || file.at>=file.record.values.len())throw "invalid save metadata read order"
    return SystemLoadCopyValue(file.record.values[file.at++])
}
// Native deserialization produces independent values; live MessageLog changes
// must not mutate the mounted record subsequently used by another file handle.
function SystemLoadCopyValue(value){
    switch(typeof value){
    case "array":
        local result=[];foreach(item in value)result.append(SystemLoadCopyValue(item));return result
    case "table":
        local result={};foreach(key,item in value)result.rawset(key,SystemLoadCopyValue(item));return result
    case "null":case "string":case "integer":case "float":case "bool":return value
    }
    throw "unsupported virtual save metadata value"
}
function SystemLoadState(file){
    if(!(file instanceof SystemLoadFileHandle) || file.at!=file.record.values.len())throw "save metadata not fully consumed"
    if(SystemLoadHost.pending!=null || SystemLoadHost.exit_state!=null)throw "original load already pending"
    SystemLoadHost.pending=file.record.state_id
    SystemLoadHost.acknowledged=false;SystemLoadHost.accepted=false
    SystemCommandHost.host_pending=true
    SystemCommandHost.actions.append("load_state\t"+SystemCommandHex(SystemLoadHost.pending))
    SystemCommandHost.delay=-1
    suspend()
    if(!SystemLoadHost.acknowledged)throw "load preparation acknowledgement required"
    local success=SystemLoadHost.accepted
    if(success)SystemLoadHost.exit_state=SystemLoadHost.pending
    SystemLoadHost.pending=null;SystemLoadHost.acknowledged=false
    return success
}
function SystemLoadAcknowledge(state_id,accepted){
    if(SystemLoadHost.pending==null || SystemLoadHost.pending!=state_id || SystemLoadHost.acknowledged)throw "unexpected load preparation acknowledgement"
    if(typeof accepted!="bool")throw "invalid load preparation result"
    SystemLoadHost.accepted=accepted;SystemLoadHost.acknowledged=true
}
function SystemLoadSleep(milliseconds){return SystemCommandWait(milliseconds)}
// Call only in the coroutine's handler boundary catch. Preserve all other errors.
function SystemLoadHandleExit(error){
    if(error!=null || SystemLoadHost.exit_state==null)return false
    SystemCommandHost.actions.append("load_commit\t"+SystemCommandHex(SystemLoadHost.exit_state))
    SystemLoadHost.exit_state=null
    return true
}
function SystemLoadBindings(){
    return {file=SystemLoadFile,Load=SystemLoadValue,LoadState=SystemLoadState,
        EnumFiles=SystemLoadEnumFiles,Sleep=SystemLoadSleep}
}
