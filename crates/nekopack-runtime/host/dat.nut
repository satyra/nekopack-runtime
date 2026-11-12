// Headless input identifiers preserve the existing fixture host policy.
// These values are not a verified original-engine ABI.
class DatResources {
 kind=null
 constructor(k){kind=k}
 function _get(name){return ::compilestring("return "+::NekoResource(kind,name))()}
}
Assets <- DatResources("Assets")
Data <- DatResources("Data")
Ini <- DatResources("Ini")
ID_AUTO <- 100
ID_BACK <- 101
ID_CANCEL <- 2
ID_CONFIG <- 103
ID_COS_ADD <- 104
ID_COS_SUB <- 105
ID_DOWN <- 106
ID_DRAG <- 107
ID_ELOAD <- 108
ID_EMO_ADD <- 109
ID_EMO_SUB <- 110
ID_ESAVE <- 111
ID_FUNC <- 2000
ID_HIDE <- 113
ID_LOAD <- 114
ID_LOG <- 115
ID_MENU <- 116
ID_NEXT <- 117
ID_OK <- 1
ID_POS_ADD <- 119
ID_POS_SUB <- 120
ID_QLOAD <- 121
ID_QSAVE <- 122
ID_QUIT <- 123
ID_RESET <- 124
ID_ROOM_ACTOR <- 125
ID_ROOM_CG <- 126
ID_ROOM_MUSIC <- 127
ID_ROOM_SCENE <- 128
ID_SAVE <- 129
ID_SKIP <- 130
ID_SKIP2 <- 131
ID_SPACE <- 132
ID_USER <- 1000
