//! Video filter.

use crate::{
    codec::{CodecError, Keyer},
    time::TimeBase,
    Error,
};
use std::{
    os::raw::{c_int, c_void},
    ptr,
};

use super::{VideoCodecParameters, VideoFrame};

extern "C" {
    fn ffw_filtergraph_new() -> *mut c_void;
    fn ffw_filtersource_new(
        source: *mut *mut c_void,
        graph: *mut c_void,
        codec: *mut c_void,
        tb_num: c_int,
        tb_den: c_int,
    ) -> c_int;
    fn ffw_filteroverlay_new(overlay: *mut *mut c_void, graph: *mut c_void) -> c_int;
    fn ffw_filtersink_new(sink: *mut *mut c_void, graph: *mut c_void) -> c_int;
    fn ffw_filtergraph_keyer_init(
        graph: *mut c_void,
        source: *mut c_void,
        overlay_source: *mut c_void,
        overlay: *mut c_void,
        sink: *mut c_void,
    ) -> c_int;
    fn ffw_filtergraph_push_frame(context: *mut c_void, frame: *const c_void) -> c_int;
    fn ffw_filtergraph_take_frame(context: *mut c_void, frame: *mut *mut c_void) -> c_int;
    fn ffw_filtergraph_free(context: *mut c_void);
}

/// A builder for video filters.
pub struct VideoKeyerBuilder {
    ptr: *mut c_void,
    input_time_base: Option<TimeBase>,
    output_time_base: Option<TimeBase>,
    codec_parameters: Option<VideoCodecParameters>,
}

impl VideoKeyerBuilder {
    /// Create a video filter builder with the given description.
    fn new() -> Self {
        let graph = unsafe { ffw_filtergraph_new() };
        if graph.is_null() {
            panic!("unable to allocate a filtergraph");
        }
        Self {
            ptr: graph,
            input_time_base: None,
            output_time_base: None,
            codec_parameters: None,
        }
    }

    /// Set input codec parameters.
    pub fn input_codec_parameters(mut self, codec_parameters: &VideoCodecParameters) -> Self {
        self.codec_parameters = Some(codec_parameters.to_owned());
        self
    }

    /// Set input time base.
    pub fn input_time_base(mut self, time_base: TimeBase) -> Self {
        self.input_time_base = Some(time_base);
        self
    }

    /// Set output time base.
    pub fn output_time_base(mut self, time_base: TimeBase) -> Self {
        self.output_time_base = Some(time_base);
        self
    }

    /// Build the filtergraph.
    pub fn build(mut self) -> Result<VideoKeyer, Error> {
        // vaidate params
        let codec_parameters = self
            .codec_parameters
            .take()
            .ok_or_else(|| Error::new("codec parameters not set"))?;
        let input_time_base = self
            .input_time_base
            .ok_or_else(|| Error::new("input time base not set"))?;

        // fallback on input timebase if not supplied
        let output_time_base = self.output_time_base.unwrap_or(input_time_base);

        // init filters
        let mut source = ptr::null_mut();
        let ret = unsafe {
            ffw_filtersource_new(
                &mut source,
                self.ptr,
                codec_parameters.as_ptr() as _,
                input_time_base.num() as _,
                input_time_base.den() as _,
            )
        };
        if ret < 0 {
            return Err(Error::from_raw_error_code(ret));
        } else if source.is_null() {
            return Err(Error::new("unable to allocate a source"));
        }

        let mut overlay_source = ptr::null_mut();
        let ret = unsafe {
            ffw_filtersource_new(
                &mut overlay_source,
                self.ptr,
                codec_parameters.as_ptr() as _,
                input_time_base.num() as _,
                input_time_base.den() as _,
            )
        };
        if ret < 0 {
            return Err(Error::from_raw_error_code(ret));
        } else if source.is_null() {
            return Err(Error::new("unable to allocate a source"));
        }

        let mut overlay = ptr::null_mut();
        let ret = unsafe { ffw_filteroverlay_new(&mut overlay, self.ptr) };
        if ret < 0 {
            return Err(Error::from_raw_error_code(ret));
        } else if source.is_null() {
            return Err(Error::new("unable to allocate a source"));
        }

        let mut sink = ptr::null_mut();
        let ret = unsafe { ffw_filtersink_new(&mut sink, self.ptr) };
        if ret < 0 {
            return Err(Error::from_raw_error_code(ret));
        } else if sink.is_null() {
            return Err(Error::new("unable to allocate a source"));
        }

        // init the filtergraph
        let ret = unsafe {
            ffw_filtergraph_keyer_init(
                self.ptr,
                source as _,
                overlay_source as _,
                overlay as _,
                sink as _,
            )
        };
        if ret < 0 {
            return Err(Error::from_raw_error_code(ret));
        }

        let ptr = self.ptr;
        self.ptr = ptr::null_mut();

        Ok(VideoKeyer {
            ptr,
            source,
            overlay: overlay_source,
            sink,
            input_time_base: input_time_base,
            output_time_base: output_time_base,
        })
    }
}

unsafe impl Send for VideoKeyer {}
unsafe impl Sync for VideoKeyer {}

impl Drop for VideoKeyerBuilder {
    fn drop(&mut self) {
        unsafe { ffw_filtergraph_free(self.ptr) }
    }
}

pub struct VideoKeyer {
    ptr: *mut c_void,
    source: *mut c_void,
    overlay: *mut c_void,
    sink: *mut c_void,
    input_time_base: TimeBase,
    output_time_base: TimeBase,
}

impl VideoKeyer {
    pub fn builder() -> VideoKeyerBuilder {
        VideoKeyerBuilder::new()
    }
}

impl Drop for VideoKeyer {
    fn drop(&mut self) {
        unsafe { ffw_filtergraph_free(self.ptr) }
    }
}

unsafe impl Send for VideoKeyerBuilder {}
unsafe impl Sync for VideoKeyerBuilder {}

impl Keyer for VideoKeyer {
    type Frame = VideoFrame;

    /// Push a given frame to the filter.
    fn try_push(&mut self, frame: VideoFrame) -> Result<(), CodecError> {
        let frame = frame.with_time_base(self.input_time_base);
        // push source frame
        unsafe {
            match ffw_filtergraph_push_frame(self.source, frame.as_ptr()) {
                1 => Ok(()),
                0 => Err(CodecError::again(
                    "all frames must be consumed before pushing a new frame",
                )),
                e => Err(CodecError::from_raw_error_code(e)),
            }
        }
    }

    fn try_push_overlay(&mut self, frame: VideoFrame) -> Result<(), CodecError> {
        let frame = frame.with_time_base(self.input_time_base);
        // push overlay frame
        unsafe {
            match ffw_filtergraph_push_frame(self.overlay, frame.as_ptr()) {
                1 => Ok(()),
                0 => Err(CodecError::again(
                    "all frames must be consumed before pushing a new frame",
                )),
                e => Err(CodecError::from_raw_error_code(e)),
            }
        }
    }

    /// Flush the filter.
    fn try_flush(&mut self) -> Result<(), CodecError> {
        unsafe {
            match ffw_filtergraph_push_frame(self.source, ptr::null()) {
                1 => Ok(()),
                0 => Err(CodecError::again(
                    "all frames must be consumed before flushing",
                )),
                e => Err(CodecError::from_raw_error_code(e)),
            }
        }
    }

    /// Take the next packet from the filter.
    fn take(&mut self) -> Result<Option<VideoFrame>, Error> {
        let mut fptr = ptr::null_mut();

        unsafe {
            match ffw_filtergraph_take_frame(self.sink, &mut fptr) {
                1 => {
                    if fptr.is_null() {
                        panic!("no frame received")
                    } else {
                        Ok(Some(VideoFrame::from_raw_ptr(fptr, self.output_time_base)))
                    }
                }
                0 => Ok(None),
                e => Err(Error::from_raw_error_code(e)),
            }
        }
    }
}
