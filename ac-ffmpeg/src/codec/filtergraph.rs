//! AV filter.

use crate::{time::TimeBase, Error};
use std::{
    ffi::CString,
    os::raw::{c_char, c_int, c_void},
    ptr,
};

use super::{video::VideoFrame, VideoCodecParameters};

extern "C" {
    fn ffw_filtergraph_new() -> *mut c_void;
    fn ffw_filtersource_new(
        graph: *mut c_void,
        codec: *mut c_void,
        tb_num: c_int,
        tb_den: c_int,
    ) -> *mut c_void;
    fn ffw_filtersink_new(graph: *mut c_void) -> *mut c_void;
    fn ffw_filtergraph_init(
        graph: *mut c_void,
        source: *mut c_void,
        sink: *mut c_void,
        filters_descr: *const c_char,
    ) -> c_int;
    fn ffw_filtergraph_push_frame(context: *mut c_void, frame: *const c_void) -> c_int;
    fn ffw_filtergraph_flush(context: *mut c_void) -> c_int;
    fn ffw_filtergraph_take_frame(context: *mut c_void, frame: *mut *mut c_void) -> c_int;
    fn ffw_filtergraph_free(context: *mut c_void);
}

/// A builder for video filters.
pub struct VideoFilterBuilder {
    ptr: *mut c_void,
    source: *mut c_void,
    sink: *mut c_void,
    time_base: TimeBase,
}

impl VideoFilterBuilder {
    /// Create a video filter builder with the given description.
    fn new(
        codec_parameters: &VideoCodecParameters,
        filters_description: &str,
        tb: TimeBase,
    ) -> Result<Self, Error> {
        let filters_descr = CString::new(filters_description).expect("invalid filter description");

        let graph = unsafe { ffw_filtergraph_new() };
        let source = unsafe {
            ffw_filtersource_new(
                graph,
                codec_parameters.as_ptr() as _,
                tb.num() as _,
                tb.den() as _,
            )
        };
        if source.is_null() {
            panic!("unable to allocate a source");
        }
        let sink = unsafe { ffw_filtersink_new(graph) };
        if sink.is_null() {
            panic!("unable to allocate a sink");
        }

        let ret = unsafe {
            ffw_filtergraph_init(graph, source as _, sink as _, filters_descr.as_ptr() as _)
        };
        if ret < 0 {
            return Err(Error::from_raw_error_code(ret));
        } else if graph.is_null() {
            panic!("unable to allocate a filtergraph");
        }

        let res = VideoFilterBuilder {
            ptr: graph,
            source,
            sink,
            time_base: tb,
        };

        Ok(res)
    }

    // TODO: set stuff

    /// Build the filtergraph.
    pub fn build(mut self) -> Result<VideoFilter, Error> {
        // just pass the pointer for now

        let ptr = self.ptr;
        self.ptr = ptr::null_mut();

        let source = self.source;
        self.source = ptr::null_mut();

        let sink = self.sink;
        self.sink = ptr::null_mut();

        let res = VideoFilter {
            ptr,
            source,
            sink,
            time_base: self.time_base,
        };

        Ok(res)
    }
}

pub struct VideoFilter {
    ptr: *mut c_void,
    source: *mut c_void,
    sink: *mut c_void,
    time_base: TimeBase,
}

impl VideoFilter {
    pub fn builder(
        codec_parameters: &VideoCodecParameters,
        filters_description: &str,
        time_base: TimeBase,
    ) -> Result<VideoFilterBuilder, Error> {
        VideoFilterBuilder::new(codec_parameters, filters_description, time_base)
    }

    /// Push a given frame to the filter.
    pub fn push(&mut self, frame: VideoFrame) -> Result<(), Error> {
        let frame = frame.with_time_base(self.time_base);
        let ret = unsafe { ffw_filtergraph_push_frame(self.source, frame.as_ptr()) };

        if ret < 0 {
            return Err(Error::from_raw_error_code(ret));
        }

        Ok(())
    }

    /// Flush the filter.
    pub fn flush(&mut self) -> Result<(), Error> {
        let ret = unsafe { ffw_filtergraph_flush(self.ptr) };

        if ret < 0 {
            return Err(Error::from_raw_error_code(ret));
        }

        Ok(())
    }

    /// Take the next packet from the filter.
    pub fn take(&mut self) -> Result<Option<VideoFrame>, Error> {
        let mut fptr = ptr::null_mut();

        unsafe {
            match ffw_filtergraph_take_frame(self.sink, &mut fptr) {
                1 => {
                    if fptr.is_null() {
                        panic!("no frame received")
                    } else {
                        Ok(Some(VideoFrame::from_raw_ptr(fptr, self.time_base)))
                    }
                }
                0 => Ok(None),
                e => Err(Error::from_raw_error_code(e)),
            }
        }
    }
}

impl Drop for VideoFilter {
    fn drop(&mut self) {
        unsafe { ffw_filtergraph_free(self.ptr) }
    }
}

unsafe impl Send for VideoFilter {}
unsafe impl Sync for VideoFilter {}
