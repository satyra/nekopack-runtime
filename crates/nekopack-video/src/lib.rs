//! Bounded MPEG-1/MP2 playback over an owned, seekable resource; no platform APIs.
use std::{
    ffi::c_void,
    io::{Read, Seek, SeekFrom},
    marker::PhantomData,
    ptr::NonNull,
    rc::Rc,
};
type Io = unsafe extern "C" fn(*mut c_void, i32, *mut u8, usize, u64) -> i64;
unsafe extern "C" {
    fn neko_video_open(io: Io, user: *mut c_void, length: usize, budget: usize) -> *mut c_void;
    fn neko_video_info(p: *mut c_void, out: *mut f64) -> i32;
    fn neko_video_step(
        p: *mut c_void,
        rgba: *mut *const u8,
        rgba_len: *mut usize,
        pcm: *mut *const i16,
        pcm_len: *mut usize,
        time: *mut f64,
        peak: *mut usize,
    ) -> i32;
    fn neko_video_close(p: *mut c_void);
    fn neko_video_live_bytes() -> usize;
}
/// Live decoder payload bytes on the calling decoder thread, excluding Rust frame copies.
pub fn live_bytes() -> usize {
    unsafe { neko_video_live_bytes() }
}
struct Input<R> {
    reader: R,
    failed: bool,
}
unsafe extern "C" fn io<R: Read + Seek>(
    user: *mut c_void,
    op: i32,
    bytes: *mut u8,
    len: usize,
    offset: u64,
) -> i64 {
    let input = unsafe { &mut *user.cast::<Input<R>>() };
    let result =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> std::io::Result<u64> {
            match op {
                0 => input
                    .reader
                    .read(unsafe { std::slice::from_raw_parts_mut(bytes, len) })
                    .map(|n| n as u64),
                1 => input.reader.seek(SeekFrom::Start(offset)),
                2 => input.reader.stream_position(),
                _ => unreachable!("private decoder I/O operation"),
            }
        }));
    match result {
        Ok(Ok(n)) => n as i64,
        Ok(Err(_)) => {
            input.failed = true;
            -1
        }
        Err(payload) => {
            std::mem::forget(payload);
            input.failed = true;
            -1
        }
    }
}
#[derive(Debug, Clone, Copy)]
pub struct Info {
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub sample_rate: u32,
    pub duration: f64,
}
pub struct Frame {
    pub rgba: Vec<u8>,
    pub pcm: Vec<i16>,
    pub video_time: f64,
    pub ended: bool,
    pub peak_bytes: usize,
}
/// Owned by one decoder worker. Its callbacks borrow the stable boxed reader only
/// during a synchronous C ABI call; neither the reader nor frame pointers escape.
pub struct Decoder<R: Read + Seek> {
    ptr: NonNull<c_void>,
    _input: Box<Input<R>>,
    pub info: Info,
    _thread: PhantomData<Rc<()>>,
}
impl<R: Read + Seek> Decoder<R> {
    pub fn open(reader: R) -> Result<Self, String> {
        Self::with_budget(reader, 64 * 1024 * 1024)
    }
    pub fn with_budget(mut reader: R, budget: usize) -> Result<Self, String> {
        let length = reader.seek(SeekFrom::End(0)).map_err(|e| e.to_string())?;
        if !(16..=512 * 1024 * 1024).contains(&length) || budget > 64 * 1024 * 1024 {
            return Err("unsupported movie length or decoder budget".into());
        }
        reader.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
        let mut magic = [0; 4];
        reader.read_exact(&mut magic).map_err(|e| e.to_string())?;
        if magic != [0, 0, 1, 0xba] {
            return Err("movie must be MPEG program stream".into());
        }
        reader.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
        let mut input = Box::new(Input {
            reader,
            failed: false,
        });
        let ptr = NonNull::new(unsafe {
            neko_video_open(
                io::<R>,
                (&mut *input as *mut Input<R>).cast(),
                length as usize,
                budget,
            )
        })
        .ok_or("invalid/unsupported movie, I/O or decoder budget failure")?;
        let mut values = [0.0; 5];
        if unsafe { neko_video_info(ptr.as_ptr(), values.as_mut_ptr()) } != 0 {
            unsafe { neko_video_close(ptr.as_ptr()) };
            return Err("movie metadata failure".into());
        }
        Ok(Self {
            ptr,
            _input: input,
            info: Info {
                width: values[0] as u32,
                height: values[1] as u32,
                fps: values[2],
                sample_rate: values[3] as u32,
                duration: values[4],
            },
            _thread: PhantomData,
        })
    }
    pub fn step(&mut self) -> Result<Frame, String> {
        let (mut rgba, mut pcm) = (std::ptr::null(), std::ptr::null());
        let (mut rn, mut pn, mut peak) = (0, 0, 0);
        let mut time = 0.0;
        let status = unsafe {
            neko_video_step(
                self.ptr.as_ptr(),
                &mut rgba,
                &mut rn,
                &mut pcm,
                &mut pn,
                &mut time,
                &mut peak,
            )
        };
        if status < 0 {
            return Err("movie decode/I/O/budget failure".into());
        }
        Ok(Frame {
            rgba: if rn == 0 {
                Vec::new()
            } else {
                unsafe { std::slice::from_raw_parts(rgba, rn) }.to_vec()
            },
            pcm: if pn == 0 {
                Vec::new()
            } else {
                unsafe { std::slice::from_raw_parts(pcm, pn) }.to_vec()
            },
            video_time: time,
            ended: status == 1,
            peak_bytes: peak,
        })
    }
}
impl<R: Read + Seek> Drop for Decoder<R> {
    fn drop(&mut self) {
        unsafe { neko_video_close(self.ptr.as_ptr()) };
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    #[test]
    fn callback_panic_and_io_error_stay_inside_rust() {
        struct Broken;
        impl Read for Broken {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                panic!("reader panic")
            }
        }
        impl Seek for Broken {
            fn seek(&mut self, _: SeekFrom) -> std::io::Result<u64> {
                Err(std::io::Error::other("seek failure"))
            }
        }
        let mut input = Box::new(Input {
            reader: Broken,
            failed: false,
        });
        let user = (&mut *input as *mut Input<Broken>).cast();
        let mut bytes = [0; 8];
        assert_eq!(
            unsafe { io::<Broken>(user, 0, bytes.as_mut_ptr(), 8, 0) },
            -1
        );
        assert!(input.failed);
        assert_eq!(
            unsafe { io::<Broken>(user, 1, std::ptr::null_mut(), 0, 0) },
            -1
        );
        assert_eq!(live_bytes(), 0);
    }
    #[test]
    fn invalid_input_and_allocation_failure_are_errors() {
        assert!(Decoder::open(Cursor::new(vec![0; 64])).is_err());
        let mut bytes = vec![0; 64];
        bytes[..4].copy_from_slice(&[0, 0, 1, 0xba]);
        for budget in [0, 1, 32, 1024, 65536, 64 * 1024 * 1024] {
            assert!(Decoder::with_budget(Cursor::new(bytes.clone()), budget).is_err());
            assert_eq!(live_bytes(), 0);
        }
    }
}
