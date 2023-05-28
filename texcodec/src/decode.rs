use crate::native_codec_get_bin_file;
use hw_common::{
    inner::DecodeCalls,
    AdapterDesc,
    DataFormat::*,
    DecodeContext, DecodeDriver,
    SurfaceFormat::{self, *},
};
use log::{error, trace};
use std::{
    ffi::c_void,
    os::raw::c_int,
    slice::from_raw_parts,
    sync::{Arc, Mutex},
    thread,
};
use DecodeDriver::*;

pub struct Decoder {
    calls: DecodeCalls,
    codec: Box<c_void>,
    frames: *mut Vec<DecodeFrame>,
    pub ctx: DecodeContext,
}

unsafe impl Send for Decoder {}
unsafe impl Sync for Decoder {}

impl Decoder {
    pub fn new(ctx: DecodeContext) -> Result<Self, ()> {
        let calls = match ctx.driver {
            CUVID => nvidia::decode_calls(),
            AMF => amf::decode_calls(),
            MFX => intel::decode_calls(),
        };
        unsafe {
            let codec = (calls.new)(
                ctx.luid,
                ctx.api as i32,
                ctx.data_format as i32,
                ctx.output_surface_format as i32,
            );
            if codec.is_null() {
                return Err(());
            }
            Ok(Self {
                calls,
                codec: Box::from_raw(codec as *mut c_void),
                frames: Box::into_raw(Box::new(Vec::<DecodeFrame>::new())),
                ctx,
            })
        }
    }

    pub fn decode(&mut self, packet: &[u8]) -> Result<&mut Vec<DecodeFrame>, i32> {
        unsafe {
            (&mut *self.frames).clear();
            let ret = (self.calls.decode)(
                &mut *self.codec,
                packet.as_ptr() as _,
                packet.len() as _,
                Some(Self::callback),
                self.frames as *mut _ as *mut c_void,
            );

            if ret < 0 {
                error!("Error decode: {}", ret);
                Err(ret)
            } else {
                Ok(&mut *self.frames)
            }
        }
    }

    unsafe extern "C" fn callback(
        texture: *mut c_void,
        format: c_int,
        width: c_int,
        height: c_int,
        obj: *const c_void,
        key: c_int,
    ) {
        let frames = &mut *(obj as *mut Vec<DecodeFrame>);

        let frame = DecodeFrame {
            surface_format: std::mem::transmute(format),
            width,
            height,
            texture,
            key: key != 0,
        };
        frames.push(frame);
    }
}

impl Drop for Decoder {
    fn drop(&mut self) {
        unsafe {
            (self.calls.destroy)(self.codec.as_mut());
            let _ = Box::from_raw(self.frames);
            trace!("Decoder dropped");
        }
    }
}

pub struct DecodeFrame {
    pub surface_format: SurfaceFormat,
    pub width: i32,
    pub height: i32,
    pub texture: *mut c_void,
    pub key: bool,
}

impl std::fmt::Display for DecodeFrame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "surface_format:{:?}, width:{}, height:{},key:{}",
            self.surface_format, self.width, self.height, self.key,
        )
    }
}

pub fn available() -> Vec<DecodeContext> {
    available_()
}

fn available_() -> Vec<DecodeContext> {
    // to-do: log control
    let mut natives: Vec<_> = vec![];
    natives.append(
        &mut nvidia::possible_support_decoders()
            .drain(..)
            .map(|n| (CUVID, n))
            .collect(),
    );
    natives.append(
        &mut amf::possible_support_decoders()
            .drain(..)
            .map(|n| (AMF, n))
            .collect(),
    );
    natives.append(
        &mut intel::possible_support_decoders()
            .drain(..)
            .map(|n| (MFX, n))
            .collect(),
    );
    let inputs = natives.drain(..).map(|(driver, n)| DecodeContext {
        driver,
        data_format: n.dataFormat,
        api: n.api,
        output_surface_format: SURFACE_FORMAT_BGRA,
        luid: 0,
    });
    let outputs = Arc::new(Mutex::new(Vec::<DecodeContext>::new()));
    let mut p_bin_264: *mut u8 = std::ptr::null_mut();
    let mut len_bin_264: c_int = 0;
    let buf264;
    let mut p_bin_265: *mut u8 = std::ptr::null_mut();
    let mut len_bin_265: c_int = 0;
    let buf265;
    unsafe {
        native_codec_get_bin_file(0, &mut p_bin_264 as _, &mut len_bin_264 as _);
        native_codec_get_bin_file(1, &mut p_bin_265 as _, &mut len_bin_265 as _);
        buf264 = from_raw_parts(p_bin_264, len_bin_264 as _);
        buf265 = from_raw_parts(p_bin_265, len_bin_265 as _);
    }
    let buf264 = Arc::new(buf264);
    let buf265 = Arc::new(buf265);
    let mut handles = vec![];
    for input in inputs {
        let outputs = outputs.clone();
        let buf264 = buf264.clone();
        let buf265 = buf265.clone();
        let handle = thread::spawn(move || {
            let test = match input.driver {
                CUVID => nvidia::decode_calls().test,
                AMF => amf::decode_calls().test,
                MFX => intel::decode_calls().test,
            };
            let mut descs: Vec<AdapterDesc> = vec![];
            descs.resize(crate::MAX_ADATER_NUM_ONE_VENDER, unsafe {
                std::mem::zeroed()
            });
            let mut desc_count: i32 = 0;
            let data = match input.data_format {
                H264 => &buf264[..],
                H265 => &buf265[..],
                _ => return,
            };
            if 0 == unsafe {
                test(
                    descs.as_mut_ptr() as _,
                    descs.len() as _,
                    &mut desc_count,
                    input.api as _,
                    input.data_format as i32,
                    input.output_surface_format as i32,
                    data.as_ptr() as *mut u8,
                    data.len() as _,
                )
            } {
                if desc_count as usize <= descs.len() {}
                for i in 0..desc_count as usize {
                    let mut input = input.clone();
                    input.luid = descs[i].luid;
                    outputs.lock().unwrap().push(input);
                }
            }
        });

        handles.push(handle);
    }
    for handle in handles {
        handle.join().ok();
    }
    let x = outputs.lock().unwrap().clone();
    x
}
