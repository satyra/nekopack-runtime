#include <cstdlib>
#include <cstring>
#include <cstdio>
#include <new>
#include <squirrel.h>
#include <sqstdstring.h>
#include <cstdint>
#include <climits>

#include <neko_limits.h>

typedef const char* (*VariableCallback)(void*,int,const char*,size_t,const char*,size_t,int32_t*);
typedef const char* (*ResourceCallback)(void*,const char*,size_t,const char*,size_t,const char**,size_t*);
struct Context { HSQUIRRELVM vm; char error[2048]; char* result; void* variables; VariableCallback variable; bool failed; NekoArena arena; void* resources; ResourceCallback resource; };
// The registry is shared by the root VM and its coroutines. Context outlives
// sq_close (including every child VM); it never owns a persistent Rust borrow.
static char context_key;
static Context* context(HSQUIRRELVM v) {
    sq_pushregistrytable(v);
    sq_pushuserpointer(v,&context_key);
    sq_rawget(v,-2);
    SQUserPointer pointer;
    sq_getuserpointer(v,-1,&pointer);
    sq_pop(v,2);
    return static_cast<Context*>(pointer);
}
static SQInteger variable_call(HSQUIRRELVM v, int operation) {
    auto* c=context(v);
    if(!c->variable) return sq_throwerror(v,"variable state is not bound");
    const SQChar *name=nullptr,*alias=nullptr;
    if(sq_gettop(v)!=(operation==0?2:3) || sq_gettype(v,2)!=OT_STRING)
        return sq_throwerror(v,"invalid variable arguments");
    sq_getstring(v,2,&name); size_t length=(size_t)sq_getsize(v,2), alias_length=0;
    int32_t value=0;
    if(operation==1) {
        SQInteger n;
        if(sq_gettype(v,3)!=OT_INTEGER || SQ_FAILED(sq_getinteger(v,3,&n)) || n<INT32_MIN || n>INT32_MAX)
            return sq_throwerror(v,"variable value must be an i32 integer");
        value=(int32_t)n;
    } else if(operation==2) {
        if(sq_gettype(v,3)!=OT_STRING) return sq_throwerror(v,"alias must be a string");
        sq_getstring(v,3,&alias);alias_length=(size_t)sq_getsize(v,3);
    }
    const char* error=c->variable(c->variables,operation,name,length,alias,alias_length,&value);
    if(error) return sq_throwerror(v,error);
    if(operation==0) {sq_pushinteger(v,value);return 1;}
    return 0;
}
static SQInteger variable_get(HSQUIRRELVM v) {return variable_call(v,0);}
static SQInteger variable_set(HSQUIRRELVM v) {return variable_call(v,1);}
static SQInteger variable_def(HSQUIRRELVM v) {return variable_call(v,2);}
static SQInteger resource_get(HSQUIRRELVM v) {
    auto* c=context(v);
    if(!c->resource) return sq_throwerror(v,"resource state is not bound");
    if(sq_gettop(v)!=3 || sq_gettype(v,2)!=OT_STRING || sq_gettype(v,3)!=OT_STRING)
        return sq_throwerror(v,"invalid resource arguments");
    const char *kind,*name,*data=nullptr; size_t length=0;
    sq_getstring(v,2,&kind);sq_getstring(v,3,&name);
    const char* error=c->resource(c->resources,kind,(size_t)sq_getsize(v,2),name,(size_t)sq_getsize(v,3),&data,&length);
    if(error) return sq_throwerror(v,error);
    sq_pushstring(v,data,(SQInteger)length);return 1;
}
static void compiler_error(HSQUIRRELVM v, const SQChar* desc, const SQChar*, SQInteger line, SQInteger column) {
    auto* c = context(v);
    snprintf(c->error,sizeof(c->error),"line %lld col %lld: %s",(long long)line,(long long)column,desc);
}
extern "C" {
void* neko_sq_open() {
    auto* c = new(std::nothrow) Context{}; if(!c) return nullptr;
    try {
    NekoScope scope(&c->arena); c->vm=sq_open(1024);
    sq_pushregistrytable(c->vm);
    sq_pushuserpointer(c->vm,&context_key);sq_pushuserpointer(c->vm,c);
    sq_rawset(c->vm,-3);sq_pop(c->vm,1);
    sq_setcompilererrorhandler(c->vm,compiler_error);
    sq_pushroottable(c->vm); sqstd_register_stringlib(c->vm); sq_pop(c->vm,1);
    sq_pushroottable(c->vm);
    const char* names[]={"NekoVariableGet","NekoVariableSet","def"};
    SQFUNCTION functions[]={variable_get,variable_set,variable_def};
    for(int i=0;i<3;++i) {sq_pushstring(c->vm,names[i],-1);sq_newclosure(c->vm,functions[i],0);sq_newslot(c->vm,-3,SQFalse);}
    sq_pushstring(c->vm,"NekoResource",-1);sq_newclosure(c->vm,resource_get,0);sq_newslot(c->vm,-3,SQFalse);
    sq_pop(c->vm,1);
    return c;
    } catch(...) { delete c; return nullptr; }
}
void neko_sq_close(Context* c) {
    { NekoScope scope(&c->arena); c->arena.metered=false;
      if(!c->arena.failed) { try { sq_close(c->vm); } catch(...) { c->arena.failed=true; } }
    }
    delete c;
}
const char* neko_sq_result(Context* c) { return c->result ? c->result : ""; }
const char* neko_sq_error(Context* c) { return c->error; }
void neko_sq_bind(Context* c, void* variables, VariableCallback callback) {c->variables=variables;c->variable=callback;}
void neko_sq_bind_resources(Context* c, void* resources, ResourceCallback callback) {c->resources=resources;c->resource=callback;}
static int eval_impl(Context* c, const char* source, size_t length, const char* name, size_t budget) {
    if(c->failed) {strcpy(c->error,"VM stopped after execution limit; create a new VM");return -1;}
    auto v=c->vm; auto top=sq_gettop(v); c->error[0]=0;
    sq_free(c->result,0); c->result=nullptr;
    if(SQ_FAILED(sq_compilebuffer(v,source,static_cast<SQInteger>(length),name,SQTrue))) {
        sq_settop(v,top); return -1;
    }
    sq_pushroottable(v);
    sq_setexecutionbudget(v,static_cast<SQUnsignedInteger>(budget));
    SQRESULT status=sq_call(v,1,SQTrue,SQTrue);
    if(sq_executionexhausted(v) || sq_getvmstate(v)==SQ_VMSTATE_SUSPENDED) {
        c->failed=true;
        strcpy(c->error,sq_executionexhausted(v) ? "Squirrel instruction budget exceeded" : "unsupported root VM suspension");
        return -1;
    }
    if(SQ_FAILED(status)) {
        sq_getlasterror(v); const SQChar* text=nullptr;
        if(SQ_SUCCEEDED(sq_getstring(v,-1,&text))) {
            strncpy(c->error,text,sizeof(c->error)-1); c->error[sizeof(c->error)-1]=0;
        } else strcpy(c->error,"non-string Squirrel exception");
        sq_settop(v,top); return -1;
    }
    sq_free(c->result,0); c->result=nullptr;
    if(sq_gettype(v,-1)==OT_STRING) {
        const SQChar* text; sq_getstring(v,-1,&text); size_t len=(size_t)sq_getsize(v,-1);
        c->result=(char*)sq_malloc(len+1);
        memcpy(c->result,text,len); c->result[len]=0;
    }
    sq_settop(v,top); return 0;
}
size_t neko_sq_memory(Context* c) { return c->arena.used; }
size_t neko_sq_live_bytes() { return neko_live_bytes; }
void neko_sq_fail_after(Context* c, size_t count) { c->arena.allocations_left=count; }
int neko_sq_eval(Context* c, const char* source, size_t length, const char* name,
                 size_t budget, size_t memory, size_t work) {
    if(c->failed) {strcpy(c->error,"VM stopped after execution limit; create a new VM");return -1;}
    NekoScope scope(&c->arena);
    c->arena.limit=memory; c->arena.work=work; c->arena.metered=true;
    try {
        if(c->arena.used>memory) neko_fail(NekoMemoryLimit);
        neko_work(length);
        return eval_impl(c,source,length,name,budget);
    } catch(NekoLimitError error) {
        const char* message = error==NekoMemoryLimit ? "native memory budget exceeded" :
            error==NekoWorkLimit ? "native work budget exceeded" :
            error==NekoDepthLimit ? "native recursion limit exceeded" : "native allocation failed";
        strcpy(c->error,message);
    } catch(...) {
        c->arena.failed=true;
        strcpy(c->error,"unexpected native exception");
    }
    c->failed=true; c->result=nullptr;
    return -1;
}
}
