#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use std::{
    ffi::{c_int, c_void},
    fmt::Display,
    slice::from_raw_parts,
};

use log::error;

include!(concat!(env!("OUT_DIR"), "/common_ffi.rs"));

#[derive(Debug, Clone, PartialEq)]
pub struct EncodeContext {
    pub device: HWDeviceType,
    pub format: PixelFormat,
    pub codec: CodecID,
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Clone)]
pub struct DecodeContext {
    pub device: HWDeviceType,
    pub format: PixelFormat,
    pub codec: CodecID,
}

pub struct EncodeFrame {
    pub data: Vec<u8>,
    pub pts: i64,
    pub key: i32,
}

impl Display for EncodeFrame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "encode len:{}, pts:{}", self.data.len(), self.pts)
    }
}

pub struct DecodeFrame {
    pub format: PixelFormat,
    pub width: i32,
    pub height: i32,
    pub data: Vec<Vec<u8>>,
    pub linesize: Vec<i32>,
    pub key: bool,
}

impl std::fmt::Display for DecodeFrame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut s = String::from("data:");
        for data in self.data.iter() {
            s.push_str(format!("{} ", data.len()).as_str());
        }
        s.push_str(", linesize:");
        for linesize in self.linesize.iter() {
            s.push_str(format!("{} ", linesize).as_str());
        }

        write!(
            f,
            "surface_format:{:?}, width:{}, height:{},key:{}, {}",
            self.format, self.width, self.height, self.key, s,
        )
    }
}

pub extern "C" fn encode_callback(
    data: *const u8,
    size: c_int,
    pts: i64,
    key: i32,
    obj: *const c_void,
) {
    unsafe {
        let frames = &mut *(obj as *mut Vec<EncodeFrame>);
        frames.push(EncodeFrame {
            data: from_raw_parts(data, size as usize).to_vec(),
            pts,
            key,
        });
    }
}

pub unsafe extern "C" fn decode_callback(
    datas: *mut *mut u8,
    linesizes: *mut i32,
    format: c_int,
    width: c_int,
    height: c_int,
    obj: *const c_void,
    key: c_int,
) {
    let frames = &mut *(obj as *mut Vec<DecodeFrame>);
    let datas = from_raw_parts(datas, MAX_DATA_NUM as _);
    let linesizes = from_raw_parts(linesizes, MAX_DATA_NUM as _);

    let mut frame = DecodeFrame {
        format: std::mem::transmute(format),
        width,
        height,
        data: vec![],
        linesize: vec![],
        key: key != 0,
    };

    if format == PixelFormat::YUV420P as c_int {
        let y = from_raw_parts(datas[0], (linesizes[0] * height) as usize).to_vec();
        let u = from_raw_parts(datas[1], (linesizes[1] * height / 2) as usize).to_vec();
        let v = from_raw_parts(datas[2], (linesizes[2] * height / 2) as usize).to_vec();

        frame.data.push(y);
        frame.data.push(u);
        frame.data.push(v);

        frame.linesize.push(linesizes[0]);
        frame.linesize.push(linesizes[1]);
        frame.linesize.push(linesizes[2]);

        frames.push(frame);
    } else if format == PixelFormat::NV12 as c_int {
        let y = from_raw_parts(datas[0], (linesizes[0] * height) as usize).to_vec();
        let uv = from_raw_parts(datas[1], (linesizes[1] * height / 2) as usize).to_vec();

        frame.data.push(y);
        frame.data.push(uv);

        frame.linesize.push(linesizes[0]);
        frame.linesize.push(linesizes[1]);

        frames.push(frame);
    } else {
        error!("unsupported pixfmt {}", format);
    }
}
