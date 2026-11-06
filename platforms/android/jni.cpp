#include <jni.h>
#include <cstddef>
#include <cstdint>
#include <climits>

extern "C" uint8_t* neko_probe_run(const uint8_t*, size_t, size_t*);
extern "C" void neko_probe_free(uint8_t*, size_t);

extern "C" JNIEXPORT jbyteArray JNICALL
Java_org_nekopack_probe_MainActivity_runChecks(JNIEnv* env, jobject, jbyteArray root) {
    if (!root) return nullptr;
    jsize length=env->GetArrayLength(root);
    jbyte* input=env->GetByteArrayElements(root,nullptr);
    if (!input) return nullptr; // Pending Java exception is preserved.
    size_t output_length=0;
    uint8_t* output=neko_probe_run(reinterpret_cast<uint8_t*>(input),length,&output_length);
    env->ReleaseByteArrayElements(root,input,JNI_ABORT);
    if (output_length>INT_MAX) {
        neko_probe_free(output,output_length);
        env->ThrowNew(env->FindClass("java/lang/IllegalStateException"),"probe report too large");
        return nullptr;
    }
    jbyteArray result=env->NewByteArray(static_cast<jsize>(output_length));
    if(result) env->SetByteArrayRegion(result,0,static_cast<jsize>(output_length),reinterpret_cast<jbyte*>(output));
    neko_probe_free(output,output_length);
    return result;
}

extern "C" uint8_t* neko_probe_dat(const int32_t*,size_t,int32_t,size_t*);
extern "C" JNIEXPORT jbyteArray JNICALL
Java_org_nekopack_probe_MainActivity_runDat(JNIEnv* env,jobject,jintArray descriptors,jint choice) {
 if(!descriptors)return nullptr;
 jsize count=env->GetArrayLength(descriptors);
 jint* fds=env->GetIntArrayElements(descriptors,nullptr);
 if(!fds)return nullptr;
 size_t length=0;
 uint8_t* output=neko_probe_dat(fds,count,choice,&length);
 env->ReleaseIntArrayElements(descriptors,fds,JNI_ABORT);
 if(length>INT_MAX){neko_probe_free(output,length);env->ThrowNew(env->FindClass("java/lang/IllegalStateException"),"DAT report too large");return nullptr;}
 jbyteArray result=env->NewByteArray(static_cast<jsize>(length));
 if(result)env->SetByteArrayRegion(result,0,static_cast<jsize>(length),reinterpret_cast<jbyte*>(output));
 neko_probe_free(output,length);return result;
}

extern "C" uint8_t* neko_probe_story(const int32_t*,size_t,int32_t,uint64_t,size_t*);
extern "C" JNIEXPORT jbyteArray JNICALL
Java_org_nekopack_probe_MainActivity_story(JNIEnv* env,jobject,jintArray descriptors,jint operation,jlong sequence){
 if(!descriptors)return nullptr;
 jsize count=env->GetArrayLength(descriptors);
 jint* fds=env->GetIntArrayElements(descriptors,nullptr);if(!fds)return nullptr;
 size_t length=0;uint8_t* output=neko_probe_story(fds,count,operation,static_cast<uint64_t>(sequence),&length);
 env->ReleaseIntArrayElements(descriptors,fds,JNI_ABORT);
 if(length>INT_MAX){neko_probe_free(output,length);env->ThrowNew(env->FindClass("java/lang/IllegalStateException"),"story frame too large");return nullptr;}
 jbyteArray result=env->NewByteArray(static_cast<jsize>(length));
 if(result)env->SetByteArrayRegion(result,0,static_cast<jsize>(length),reinterpret_cast<jbyte*>(output));
 neko_probe_free(output,length);return result;
}
