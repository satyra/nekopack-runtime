// pl_mpeg is compiled as C++ so bounded allocation/I/O failure unwinds to this C ABI.
// No exception escapes these entry points; the arena also cleans partial construction.
#include <cstddef>
#include <cstdint>
#include <cstdlib>
#include <cstring>
#include <cmath>
#include <new>
struct Arena;
struct alignas(std::max_align_t) Block { Block *prev,*next; Arena *owner; size_t size; };
struct Arena { Block *head=nullptr; size_t live=0,peak=0,limit=64*1024*1024; };
static thread_local Arena *current=nullptr;
static thread_local size_t live_bytes=0;
struct Guard { Arena *old; Guard(Arena *a):old(current){current=a;} ~Guard(){current=old;} };
static void *allocate(size_t n) {
 if(n>current->limit-current->live)throw 1;
 void *memory=std::malloc(sizeof(Block)+n);if(!memory)throw 1;
 auto *b=new(memory) Block{};live_bytes+=n;
 b->owner=current;b->size=n;b->prev=nullptr;b->next=current->head;
 if(b->next)b->next->prev=b;current->head=b;current->live+=n;
 if(current->live>current->peak)current->peak=current->live;
 return b+1;
}
static void release(void *p) {
 if(!p)return;auto*b=static_cast<Block*>(p)-1;auto*a=b->owner;
 if(b->prev)b->prev->next=b->next;else a->head=b->next;
 if(b->next)b->next->prev=b->prev;a->live-=b->size;live_bytes-=b->size;b->~Block();std::free(b);
}
static void *resize(void *p,size_t n) {
 if(!p)return allocate(n);auto*b=static_cast<Block*>(p)-1;
 void*q=allocate(n);std::memcpy(q,p,n<b->size?n:b->size);release(p);return q;
}
static void clear(Arena &a){while(a.head)release(a.head+1);}
#define PLM_NO_STDIO
#define PLM_MALLOC allocate
#define PLM_REALLOC resize
#define PLM_FREE release
#define PL_MPEG_IMPLEMENTATION
#include "../../../vendor/pl_mpeg/pl_mpeg.h"

using Io=int64_t(*)(void*,int,uint8_t*,size_t,uint64_t);
struct State {
 Arena arena;Io io;void*user;plm_t*plm=nullptr;bool failed=false;
 size_t io_left=0,calls=0,steps=0;double duration=0,video_time=0;
 uint8_t *rgba=nullptr;size_t rgba_len=0;int width=0,height=0;int16_t pcm[4608];size_t pcm_len=0;bool frame=false;
 uint64_t audio_samples=0,video_frames=0;double video_end=0;
};
static int64_t input(State*s,int op,uint8_t*p,size_t n,uint64_t offset=0){
 if(++s->calls>4096||n>s->io_left)throw 2;
 s->io_left-=n;int64_t result=s->io(s->user,op,p,n,offset);if(result<0)throw 2;return result;
}
static void load(plm_buffer_t*b,void*u){
 auto*s=static_cast<State*>(u);uint8_t bytes[65536];
 auto n=input(s,0,bytes,sizeof(bytes));if(n>static_cast<int64_t>(sizeof(bytes)))throw 2;
 if(n)plm_buffer_write(b,bytes,static_cast<size_t>(n));else b->has_ended=TRUE;
}
static void seek(plm_buffer_t*,size_t offset,void*u){input(static_cast<State*>(u),1,nullptr,0,offset);}
static size_t tell(plm_buffer_t*,void*u){return static_cast<size_t>(input(static_cast<State*>(u),2,nullptr,0));}
static void video(plm_t*,plm_frame_t*f,void*u){
 auto*s=static_cast<State*>(u);if(s->frame||f->width!=s->width||f->height!=s->height)throw 3;
 plm_frame_to_rgba(f,s->rgba,f->width*4);s->video_time=f->time;s->frame=true;
 ++s->video_frames;s->video_end=f->time+1.0/plm_get_framerate(s->plm);
}
static void audio(plm_t*,plm_samples_t*f,void*u){
 auto*s=static_cast<State*>(u);size_t n=f->count*2;if(s->pcm_len+n>4608)throw 3;
 for(size_t i=0;i<n;i++){float v=f->interleaved[i];if(!std::isfinite(v))throw 3;
  s->pcm[s->pcm_len++]=static_cast<int16_t>(v>=1?32767:v<=-1?-32768:std::lrint(v*32767));}
 s->audio_samples+=f->count;
}
extern "C" void* neko_video_open(Io io,void*user,size_t length,size_t budget) noexcept {
 if(budget<sizeof(State))return nullptr;
 void*memory=std::malloc(sizeof(State));if(!memory)return nullptr;auto*s=new(memory) State{};live_bytes+=sizeof(State);
 s->io=io;s->user=user;s->arena.limit=budget-sizeof(State);s->io_left=32*1024*1024;
 try {Guard guard(&s->arena);
  s->plm=plm_create_with_buffer(plm_buffer_create_with_callbacks(load,seek,tell,length,s),TRUE);
  if(!plm_has_headers(s->plm))throw 3;
  int w=plm_get_width(s->plm),h=plm_get_height(s->plm),rate=plm_get_samplerate(s->plm);
  double fps=plm_get_framerate(s->plm);
  if(w<=0||h<=0||w%2||h%2||w>1920||h>1080||fps<1||fps>60||plm_get_num_audio_streams(s->plm)!=1||plm_get_num_video_streams(s->plm)!=1||(rate!=32000&&rate!=44100&&rate!=48000))throw 3;
  s->width=w;s->height=h;s->rgba_len=static_cast<size_t>(w)*h*4;s->rgba=static_cast<uint8_t*>(allocate(s->rgba_len));std::memset(s->rgba,255,s->rgba_len);
  s->duration=plm_get_duration(s->plm);if(!std::isfinite(s->duration)||s->duration<=0||s->duration>600)throw 3;
  plm_set_video_decode_callback(s->plm,video,s);plm_set_audio_decode_callback(s->plm,audio,s);
  return s;
 }catch(...){clear(s->arena);s->~State();live_bytes-=sizeof(State);std::free(s);return nullptr;}
}
extern "C" int neko_video_info(void*p,double*out) noexcept {
 auto*s=static_cast<State*>(p);if(s->failed)return -1;
 try {Guard guard(&s->arena);out[0]=plm_get_width(s->plm);out[1]=plm_get_height(s->plm);out[2]=plm_get_framerate(s->plm);out[3]=plm_get_samplerate(s->plm);out[4]=s->duration;return 0;}catch(...){s->failed=true;return -1;}
}
extern "C" int neko_video_step(void*p,const uint8_t**rgba,size_t*rgba_len,const int16_t**pcm,size_t*pcm_len,double*time,size_t*peak) noexcept {
 auto*s=static_cast<State*>(p);if(s->failed)return -1;
 try {Guard guard(&s->arena);if(++s->steps>static_cast<size_t>((s->duration+3)*60))throw 3;
  s->frame=false;s->pcm_len=0;s->io_left=8*1024*1024;s->calls=0;
  plm_decode(s->plm,1.0/60.0);
  *rgba=s->rgba;*rgba_len=s->frame?s->rgba_len:0;*pcm=s->pcm;*pcm_len=s->pcm_len;*time=s->video_time;*peak=s->arena.peak+sizeof(State);
  return plm_has_ended(s->plm)?1:0;
 }catch(...){s->failed=true;return -1;}
}
extern "C" void neko_video_close(void*p) noexcept {auto*s=static_cast<State*>(p);clear(s->arena);s->~State();live_bytes-=sizeof(State);std::free(s);}

extern "C" size_t neko_video_live_bytes() noexcept {return live_bytes;}
extern "C" void neko_video_times(void*p,double*out) noexcept {
 auto*s=static_cast<State*>(p);out[0]=double(s->audio_samples)/plm_get_samplerate(s->plm);out[1]=s->video_end;out[2]=double(s->video_frames);
}
