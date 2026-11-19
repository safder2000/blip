//! Opus through the libopus that ships with Blip (opus.dll beside Blip.exe, libopus.0.dylib in
//! Blip.app/Contents/Frameworks), loaded at run time like the Python version does. Without it
//! calls through a server fall back to raw audio.

use std::ffi::c_void;
use std::os::raw::{c_int, c_uchar};
use std::sync::OnceLock;

pub const RATE: i32 = 16000;
pub const FRAME: usize = 320;
const APPLICATION_VOIP: c_int = 2048;
const MAX_PACKET: usize = 400;

type EncCreate = unsafe extern "C" fn(i32, c_int, c_int, *mut c_int) -> *mut c_void;
type Encode = unsafe extern "C" fn(*mut c_void, *const i16, c_int, *mut c_uchar, i32) -> i32;
type DecCreate = unsafe extern "C" fn(i32, c_int, *mut c_int) -> *mut c_void;
type Decode = unsafe extern "C" fn(*mut c_void, *const c_uchar, i32, *mut i16, c_int, c_int) -> c_int;
type Destroy = unsafe extern "C" fn(*mut c_void);

struct Lib {
    _lib: libloading::Library,
    enc_create: EncCreate,
    encode: Encode,
    enc_destroy: Destroy,
    dec_create: DecCreate,
    decode: Decode,
    dec_destroy: Destroy,
}

fn candidates() -> Vec<std::path::PathBuf> {
    let mut out = vec![];
    if let Some(p) = std::env::var_os("BLIP_OPUS") {
        out.push(std::path::PathBuf::from(p));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            for n in ["opus.dll", "libopus-0.dll", "libopus.0.dylib", "libopus.dylib", "../Frameworks/libopus.0.dylib", "../Resources/libopus.0.dylib"] {
                out.push(dir.join(n));
            }
            // development: the repository's vendor folder
            for up in dir.ancestors().take(6) {
                out.push(up.join("vendor").join(if cfg!(windows) { "opus.dll" } else { "libopus.0.dylib" }));
            }
        }
    }
    if cfg!(target_os = "macos") {
        out.push("/opt/homebrew/lib/libopus.0.dylib".into());
        out.push("/usr/local/lib/libopus.0.dylib".into());
    }
    out
}

fn lib() -> Option<&'static Lib> {
    static L: OnceLock<Option<Lib>> = OnceLock::new();
    L.get_or_init(|| {
        for c in candidates() {
            if !c.exists() {
                continue;
            }
            unsafe {
                let Ok(l) = libloading::Library::new(&c) else { continue };
                let got = (|| -> Result<(EncCreate, Encode, Destroy, DecCreate, Decode, Destroy), libloading::Error> {
                    Ok((
                        *l.get::<EncCreate>(b"opus_encoder_create\0")?,
                        *l.get::<Encode>(b"opus_encode\0")?,
                        *l.get::<Destroy>(b"opus_encoder_destroy\0")?,
                        *l.get::<DecCreate>(b"opus_decoder_create\0")?,
                        *l.get::<Decode>(b"opus_decode\0")?,
                        *l.get::<Destroy>(b"opus_decoder_destroy\0")?,
                    ))
                })();
                let get = || got.map(|(enc_create, encode, enc_destroy, dec_create, decode, dec_destroy)| Lib { enc_create, encode, enc_destroy, dec_create, decode, dec_destroy, _lib: l });
                if let Ok(x) = get() {
                    return Some(x);
                }
            }
        }
        None
    })
    .as_ref()
}

pub fn available() -> bool {
    lib().is_some()
}

pub struct Encoder {
    h: *mut c_void,
}

pub struct Decoder {
    h: *mut c_void,
}

// the handles are only used behind a Mutex
unsafe impl Send for Encoder {}
unsafe impl Send for Decoder {}

impl Encoder {
    pub fn new() -> Option<Encoder> {
        let l = lib()?;
        let mut err = 0;
        let h = unsafe { (l.enc_create)(RATE, 1, APPLICATION_VOIP, &mut err) };
        (err == 0 && !h.is_null()).then_some(Encoder { h })
    }

    /// One 20 ms frame (320 samples) -> an Opus packet.
    pub fn encode(&mut self, pcm: &[i16]) -> Option<Vec<u8>> {
        let l = lib()?;
        let mut out = vec![0u8; MAX_PACKET];
        let n = unsafe { (l.encode)(self.h, pcm.as_ptr(), FRAME as c_int, out.as_mut_ptr(), MAX_PACKET as i32) };
        (n >= 0).then(|| {
            out.truncate(n as usize);
            out
        })
    }
}

impl Decoder {
    pub fn new() -> Option<Decoder> {
        let l = lib()?;
        let mut err = 0;
        let h = unsafe { (l.dec_create)(RATE, 1, &mut err) };
        (err == 0 && !h.is_null()).then_some(Decoder { h })
    }

    /// An Opus packet -> PCM; None packet = fill in a lost frame.
    pub fn decode(&mut self, packet: Option<&[u8]>) -> Option<Vec<i16>> {
        let l = lib()?;
        let mut out = vec![0i16; FRAME * 6];
        let n = unsafe {
            match packet {
                Some(p) => (l.decode)(self.h, p.as_ptr(), p.len() as i32, out.as_mut_ptr(), (FRAME * 6) as c_int, 0),
                None => (l.decode)(self.h, std::ptr::null(), 0, out.as_mut_ptr(), FRAME as c_int, 0),
            }
        };
        (n > 0).then(|| {
            out.truncate(n as usize);
            out
        })
    }
}

impl Drop for Encoder {
    fn drop(&mut self) {
        if let Some(l) = lib() {
            unsafe { (l.enc_destroy)(self.h) }
        }
    }
}

impl Drop for Decoder {
    fn drop(&mut self) {
        if let Some(l) = lib() {
            unsafe { (l.dec_destroy)(self.h) }
        }
    }
}
