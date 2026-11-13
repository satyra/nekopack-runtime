#include <cstdint>
#include <cstdlib>
#include <cstring>
#include <cmath>
#include <new>
#include <vector>
#include <deque>
extern "C" {
#include <libavformat/avformat.h>
#include <libavcodec/avcodec.h>
#include <libavutil/channel_layout.h>
#include <libavutil/imgutils.h>
#include <libswscale/swscale.h>
#include <libswresample/swresample.h>
}
using Io=int64_t(*)(void*,int,uint8_t*,size_t,uint64_t);
struct Asf {
 Io io;void*user;size_t length,budget;AVFormatContext*format=nullptr;AVIOContext*input=nullptr;
 AVCodecContext*video=nullptr,*audio=nullptr;AVFrame*frame=nullptr;AVPacket*packet=nullptr;
 SwsContext*sws=nullptr;SwrContext*swr=nullptr;int vi=-1,ai=-1;
 bool failed=false,demux_end=false,vflush=false,aflush=false,veof=false,aeof=false,swr_end=false;
 std::vector<uint8_t>rgba;std::vector<int16_t>pcm;double fps=0,duration=0,time=0,video_end=0;
 uint64_t samples=0,frames=0;size_t reads=0,read_bytes=0;
 bool first_audio=true;uint64_t discard_audio=0;
 std::deque<AVPacket*>vq,aq;size_t queued_bytes=0,queued_packets=0,peak_payload=0;
};
static size_t bridge_bytes(Asf*s){
 size_t now=s->rgba.capacity()+s->pcm.capacity()*sizeof(int16_t)+65536+sizeof(Asf)+s->queued_bytes+s->queued_packets*sizeof(AVPacket);
 if(now>s->peak_payload)s->peak_payload=now;return s->peak_payload;
}
static int read_input(void*opaque,uint8_t*bytes,int n){
 auto*s=static_cast<Asf*>(opaque);if(++s->reads>4096||(s->read_bytes+=n)>32*1024*1024)return AVERROR(EIO);
 auto result=s->io(s->user,0,bytes,n,0);return result<0||result>n?AVERROR(EIO):result?int(result):AVERROR_EOF;
}
static int64_t seek_input(void*opaque,int64_t offset,int whence){
 auto*s=static_cast<Asf*>(opaque);if(whence==AVSEEK_SIZE)return s->length;
 whence&=~AVSEEK_FORCE;
 int64_t base=whence==SEEK_SET?0:whence==SEEK_END?int64_t(s->length):whence==SEEK_CUR?s->io(s->user,2,nullptr,0,0):-1;
 if(base<0||offset < -base||offset>int64_t(s->length)-base)return AVERROR(EINVAL);
 return s->io(s->user,1,nullptr,0,uint64_t(base+offset));
}
extern "C" void neko_asf_close(void*p) noexcept {
 auto*s=static_cast<Asf*>(p);av_packet_free(&s->packet);av_frame_free(&s->frame);
 for(auto*packet:s->vq)av_packet_free(&packet);for(auto*packet:s->aq)av_packet_free(&packet);
 sws_freeContext(s->sws);swr_free(&s->swr);avcodec_free_context(&s->video);avcodec_free_context(&s->audio);
 avformat_close_input(&s->format);if(s->input){av_freep(&s->input->buffer);avio_context_free(&s->input);}delete s;
}
static AVCodecContext* open_codec(AVStream*stream){
 auto*codec=avcodec_find_decoder(stream->codecpar->codec_id);if(!codec)return nullptr;
 auto*c=avcodec_alloc_context3(codec);if(!c)return nullptr;
 c->thread_count=1;c->max_pixels=1920*1080;c->err_recognition=AV_EF_CAREFUL;
 c->pkt_timebase=stream->time_base;
 if(avcodec_parameters_to_context(c,stream->codecpar)<0||avcodec_open2(c,codec,nullptr)<0){avcodec_free_context(&c);return nullptr;}return c;
}
extern "C" void*neko_asf_open(Io io,void*user,size_t length,size_t budget) noexcept {
 auto*s=new(std::nothrow)Asf{};if(!s)return nullptr;s->io=io;s->user=user;s->length=length;s->budget=budget;
 try {
  // Codec dimensions and demux probe/packet limits bound the permitted media workload.
  // FFmpeg owns internal allocations: this admission floor is not a total heap cap.
  if(budget<32*1024*1024)throw 1;
  auto*buffer=static_cast<uint8_t*>(av_malloc(65536));if(!buffer)throw 1;
  s->input=avio_alloc_context(buffer,65536,0,s,read_input,nullptr,seek_input);
  if(!s->input){av_free(buffer);throw 1;}
  s->format=avformat_alloc_context();if(!s->format)throw 1;
  s->format->pb=s->input;s->format->flags|=AVFMT_FLAG_CUSTOM_IO;
  s->format->probesize=1024*1024;s->format->max_analyze_duration=5*AV_TIME_BASE;
  s->format->max_streams=8;
  if(avformat_open_input(&s->format,nullptr,av_find_input_format("asf"),nullptr)<0||avformat_find_stream_info(s->format,nullptr)<0)throw 1;
  for(unsigned i=0;i<s->format->nb_streams;i++){
   auto type=s->format->streams[i]->codecpar->codec_type;
   if(type==AVMEDIA_TYPE_VIDEO){if(s->vi>=0)throw 1;s->vi=i;}
   if(type==AVMEDIA_TYPE_AUDIO){if(s->ai>=0)throw 1;s->ai=i;}
  }
  if(s->vi<0||s->ai<0)throw 1;
  auto*vs=s->format->streams[s->vi];auto*as=s->format->streams[s->ai];
  int w=vs->codecpar->width,h=vs->codecpar->height;
  if(w<=0||h<=0||w>1920||h>1080)throw 1;
  s->video=open_codec(vs);s->audio=open_codec(as);if(!s->video||!s->audio)throw 1;
  s->fps=av_q2d(av_guess_frame_rate(s->format,vs,nullptr));s->duration=double(s->format->duration)/AV_TIME_BASE;
  if(!std::isfinite(s->fps)||s->fps<1||s->fps>60||!std::isfinite(s->duration)||s->duration<=0||s->duration>600||s->audio->sample_rate<8000||s->audio->sample_rate>96000)throw 1;
  s->rgba.resize(size_t(w)*h*4);
  s->frame=av_frame_alloc();s->packet=av_packet_alloc();if(!s->frame||!s->packet)throw 1;
  AVChannelLayout stereo=AV_CHANNEL_LAYOUT_STEREO;
  if(swr_alloc_set_opts2(&s->swr,&stereo,AV_SAMPLE_FMT_S16,s->audio->sample_rate,&s->audio->ch_layout,s->audio->sample_fmt,s->audio->sample_rate,0,nullptr)<0||swr_init(s->swr)<0)throw 1;
  return s;
 }catch(...){neko_asf_close(s);return nullptr;}
}
extern "C" int neko_asf_info(void*p,double*out) noexcept {
 auto*s=static_cast<Asf*>(p);out[0]=s->video->width;out[1]=s->video->height;out[2]=s->fps;out[3]=s->audio->sample_rate;out[4]=s->duration;return 0;
}
extern "C" void neko_asf_times(void*p,double*out) noexcept {
 auto*s=static_cast<Asf*>(p);out[0]=double(s->samples)/s->audio->sample_rate;out[1]=s->video_end;out[2]=double(s->frames);
}
// Read ahead compressed packets only. ASF file order can put video several
// seconds before the audio for that presentation interval.
static void demux_packet(Asf*s){
 int r=av_read_frame(s->format,s->packet);
 if(r==AVERROR_EOF){if(s->input->error&&s->input->error!=AVERROR_EOF)throw 1;s->demux_end=true;return;}
 if(r<0)throw 1;
 int stream=s->packet->stream_index;
 if(stream!=s->vi&&stream!=s->ai){av_packet_unref(s->packet);return;}
 if(s->packet->size<0||size_t(s->packet->size)>8*1024*1024-s->queued_bytes||s->queued_packets>=4096)throw 1;
 auto&q=stream==s->vi?s->vq:s->aq;
 q.push_back(nullptr);q.back()=av_packet_alloc();if(!q.back())throw 1;
 s->queued_bytes+=size_t(s->packet->size);++s->queued_packets;
 av_packet_move_ref(q.back(),s->packet);bridge_bytes(s);
}
// True means one decoded frame; false means decoder EOF. Each stream flushes only
// after its own compressed queue is empty, even if demux EOF was reached early.
static bool receive_stream(Asf*s,bool video){
 auto*c=video?s->video:s->audio;auto&q=video?s->vq:s->aq;bool&flushed=video?s->vflush:s->aflush;
 for(int work=0;work<4096;work++){
  int r=avcodec_receive_frame(c,s->frame);
  if(r==0)return true;
  if(r==AVERROR_EOF)return false;
  if(r!=AVERROR(EAGAIN)||flushed)throw 1;
  while(q.empty()&&!s->demux_end){if(++work>=4096)throw 1;demux_packet(s);}
  if(q.empty()){
   if(avcodec_send_packet(c,nullptr)<0)throw 1;flushed=true;
  }else{
   auto*packet=q.front();r=avcodec_send_packet(c,packet);
   s->queued_bytes-=size_t(packet->size);--s->queued_packets;
   q.pop_front();av_packet_free(&packet);if(r<0)throw 1;
  }
 }
 throw 1;
}
extern "C" int neko_asf_step(void*p,const uint8_t**rgba,size_t*rn,const int16_t**pcm,size_t*pn,double*time,size_t*peak) noexcept {
 auto*s=static_cast<Asf*>(p);if(s->failed)return -1;s->reads=0;s->read_bytes=0;
 *rgba=nullptr;*rn=0;*pcm=nullptr;*pn=0;*time=s->time;
 // Report owned output payload; FFmpeg internal allocations are not included.
 *peak=bridge_bytes(s);
 try {
  for(int work=0;work<4096;work++){
   if(!s->aeof&&(s->veof||double(s->samples)/s->audio->sample_rate<s->video_end+0.1)){
    if(receive_stream(s,false)){auto*f=s->frame;int capacity=swr_get_out_samples(s->swr,f->nb_samples);if(capacity<0||capacity>192000)throw 1;
     size_t silence=0;
     if(s->first_audio){
      // Decoded WMA can start after packet PTS zero because of codec delay.
      // Keep PCM and video on the same container-relative presentation clock.
      int64_t pts=f->best_effort_timestamp;if(pts==AV_NOPTS_VALUE)throw 1;
      AVRational samples_timebase={1,s->audio->sample_rate};
      int64_t start=av_rescale_q(pts,s->format->streams[s->ai]->time_base,samples_timebase);
      if(s->format->start_time!=AV_NOPTS_VALUE)start-=av_rescale_q(s->format->start_time,AV_TIME_BASE_Q,samples_timebase);
      if(start>0)silence=size_t(start);else s->discard_audio=uint64_t(-start);
      s->first_audio=false;
     }
     if(silence>uint64_t(s->audio->sample_rate)*603||(silence+capacity)*4>s->budget-s->rgba.size()-65536)throw 1;
     s->pcm.assign((silence+size_t(capacity))*2,0);uint8_t*dst[]={reinterpret_cast<uint8_t*>(s->pcm.data()+silence*2)};
     int count=swr_convert(s->swr,dst,capacity,const_cast<const uint8_t**>(f->extended_data),f->nb_samples);av_frame_unref(f);if(count<0)throw 1;
     size_t discard=size_t(s->discard_audio<uint64_t(count)?s->discard_audio:uint64_t(count));s->discard_audio-=discard;
     if(discard)std::memmove(s->pcm.data()+silence*2,s->pcm.data()+(silence+discard)*2,(size_t(count)-discard)*4);
     size_t produced=silence+size_t(count)-discard;s->samples+=produced;if(s->samples>uint64_t(s->audio->sample_rate)*603)throw 1;
     *pcm=s->pcm.data();*pn=produced*2;*peak=bridge_bytes(s);return 0;
    }s->aeof=true;
   }
   if(!s->veof){
    if(receive_stream(s,true)){auto*f=s->frame;if(f->width!=s->video->width||f->height!=s->video->height)throw 1;
     auto*vs=s->format->streams[s->vi];int64_t pts=f->best_effort_timestamp;
     if(pts==AV_NOPTS_VALUE)throw 1;
     double t=(pts-(s->format->start_time==AV_NOPTS_VALUE?0:av_rescale_q(s->format->start_time,AV_TIME_BASE_Q,vs->time_base)))*av_q2d(vs->time_base);
     if(t<0||!std::isfinite(t)||(s->frames&&t<s->time)||t>603)throw 1;
     s->sws=sws_getCachedContext(s->sws,f->width,f->height,AVPixelFormat(f->format),f->width,f->height,AV_PIX_FMT_RGBA,SWS_BILINEAR,nullptr,nullptr,nullptr);if(!s->sws)throw 1;
     uint8_t*dst[]={s->rgba.data()};int stride[]={f->width*4};
     if(sws_scale(s->sws,f->data,f->linesize,0,f->height,dst,stride)!=f->height)throw 1;
     s->time=t;s->video_end=t+(f->duration>0?f->duration*av_q2d(vs->time_base):1.0/s->fps);++s->frames;
     av_frame_unref(f);*rgba=s->rgba.data();*rn=s->rgba.size();*time=t;*peak=bridge_bytes(s);return 0;
    }s->veof=true;
   }
   if(s->veof&&s->aeof){
    if(!s->swr_end){s->pcm.resize(8192);uint8_t*dst[]={reinterpret_cast<uint8_t*>(s->pcm.data())};int n=swr_convert(s->swr,dst,4096,nullptr,0);if(n<0)throw 1;
     if(n){s->samples+=n;*pcm=s->pcm.data();*pn=size_t(n)*2;*peak=bridge_bytes(s);return 0;}s->swr_end=true;}
    return 1;
   }
  }throw 1;
 }catch(...){s->failed=true;return -1;}
}
