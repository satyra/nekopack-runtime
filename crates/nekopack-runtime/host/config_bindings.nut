// Run the original page setup against the same VM that owns Config.
// The renderer consumes the bindings after the first wait boundary.
function ConfigPageBindings(page) {
    local root = getroottable()
    local bindings = []
    local stop = {}
    local sprite = class {
        function Base() {}
        function Draw(source) {}
    }
    local spots = class {
        constructor(images) {}
        function Bind(name, id) {}
        function BindR(name, id) {}
        function Active(id) {}
    }
    local gui = class {
        function Bind(name, control) { ::NekoConfigBindingSink.append(name + "\t" + control) }
        function Put(values) {}
        function Get(name) { return Config[name] }
    }
    local glyph = class {
        Face = null
        Color = 0
        Size = 0
    }
    local image_set = class {
        function _get(name) { return null }
    }
    local host = {
        NekoConfigBindingSink = bindings,
        NekoConfigStop = stop,
        NekoConfigImageSet = image_set,
        ID_DEFAULT = null,
        ID_SYSTEM = null,
        Sprite = sprite,
        Screen = sprite,
        Spots = spots,
        GUI = gui,
        Glyph = glyph,
        Images = function(name) { return ::NekoConfigImageSet() },
        RadioButton = function(images, name, count) { return count },
        RadioList = function(images, voices) { return 2 },
        VolumeBar = function(images, name, maximum) { return maximum },
        ScrollBar = function(images, name, last) { return last },
        wait = function(time) { throw ::NekoConfigStop }
    }
    local saved = {}
    foreach (key, value in host) {
        saved.rawset(key, [key in root, key in root ? root[key] : null])
        root.rawset(key, value)
    }
    local reached = false
    local failure = null
    try {
        if (ViewConfigPage.getinfos().parameters.len() == 1) ViewConfigPage()
        else ViewConfigPage(page)
    } catch (error) {
        if (error == stop) reached = true
        else failure = error
    }
    foreach (key, entry in saved) {
        if (entry[0]) root.rawset(key, entry[1])
        else root.rawdelete(key)
    }
    if (!reached) throw (failure == null ? "settings setup returned before wait" : failure)
    local result = ""
    foreach (binding in bindings) result += binding + "\n"
    return result
}
