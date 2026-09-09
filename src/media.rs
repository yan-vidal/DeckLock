//! Muted, looping local video through the official GTK4 GStreamer sink.
use gstreamer::{self as gst, prelude::*};
use gtk::{gdk, gio, prelude::*};
use std::path::Path;

/// Re-registering the static plugin swaps it in the global registry and frees
/// factories a running pipeline's typefind thread still walks; once per process.
fn start() -> Result<(), String> {
    static REGISTER: std::sync::OnceLock<Result<(), String>> = std::sync::OnceLock::new();
    REGISTER
        .get_or_init(|| {
            gst::init().map_err(|e| e.to_string())?;
            gstgtk4::plugin_register_static().map_err(|e| e.to_string())
        })
        .clone()
}

/// One decoded frame for a library thumbnail. Decodes nothing but the preroll and
/// gives up on the deadline, so an unreadable or slow file cannot stall settings.
pub fn frame(
    path: &Path,
    width: i32,
    deadline: std::time::Duration,
) -> Result<gdk::Texture, String> {
    decode_frame(path, width, deadline).map(|frame| frame.texture())
}

// Only owned pixel data crosses from the decoder to GTK's main thread.
pub(crate) struct Frame {
    width: i32,
    height: i32,
    stride: usize,
    bytes: Vec<u8>,
}
impl Frame {
    pub(crate) fn texture(self) -> gdk::Texture {
        gdk::MemoryTexture::new(
            self.width,
            self.height,
            gdk::MemoryFormat::R8g8b8,
            &glib::Bytes::from_owned(self.bytes),
            self.stride,
        )
        .upcast()
    }
}

pub(crate) fn decode_frame(
    path: &Path,
    width: i32,
    deadline: std::time::Duration,
) -> Result<Frame, String> {
    start()?;
    let sink = gst::ElementFactory::make("fakesink")
        .property("sync", false)
        .build()
        .map_err(|e| e.to_string())?;
    let pipeline = gst::ElementFactory::make("playbin")
        .property("uri", gio::File::for_path(path).uri())
        .property("video-sink", &sink)
        .property(
            "audio-sink",
            gst::ElementFactory::make("fakesink")
                .build()
                .map_err(|e| e.to_string())?,
        )
        .build()
        .map_err(|e| e.to_string())?;
    let result = prerolled_frame(&pipeline, width, deadline);
    let _ = pipeline.set_state(gst::State::Null);
    result
}

fn prerolled_frame(
    pipeline: &gst::Element,
    width: i32,
    deadline: std::time::Duration,
) -> Result<Frame, String> {
    let timeout = gst::ClockTime::from_nseconds(deadline.as_nanos().min(u64::MAX as u128) as u64);
    pipeline
        .set_state(gst::State::Paused)
        .map_err(|e| e.to_string())?;
    pipeline
        .state(Some(timeout))
        .0
        .map_err(|_| "Video did not preroll in time".to_string())?;
    // A frame from the very start is often a black lead-in; skip ahead when the
    // clip is long enough, then wait for that seek to preroll as well.
    if pipeline
        .query_duration::<gst::ClockTime>()
        .is_some_and(|d| d > gst::ClockTime::from_seconds(3))
        && pipeline
            .seek_simple(
                gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT,
                gst::ClockTime::from_seconds(1),
            )
            .is_ok()
    {
        pipeline
            .state(Some(timeout))
            .0
            .map_err(|_| "Video did not seek in time".to_string())?;
    }
    let caps = gst::Caps::builder("video/x-raw")
        .field("format", "RGB")
        .field("width", width)
        .field("pixel-aspect-ratio", gst::Fraction::new(1, 1))
        .build();
    let sample = pipeline.emit_by_name::<Option<gst::Sample>>("convert-sample", &[&caps]);
    let sample = sample.ok_or("Video has no convertible frame")?;
    let info = sample
        .caps()
        .and_then(|c| c.structure(0).map(|s| s.to_owned()))
        .ok_or("Converted frame has no caps")?;
    let height: i32 = info.get("height").map_err(|e| e.to_string())?;
    let width: i32 = info.get("width").map_err(|e| e.to_string())?;
    let buffer = sample.buffer().ok_or("Converted frame has no buffer")?;
    let map = buffer.map_readable().map_err(|e| e.to_string())?;
    let stride = map.len() / height.max(1) as usize;
    Ok(Frame {
        width,
        height,
        stride,
        bytes: map.as_slice().to_vec(),
    })
}

pub struct Playback {
    pipeline: gst::Element,
    _watch: gst::bus::BusWatchGuard,
}

impl Playback {
    pub fn new(path: &Path, picture: &gtk::Picture) -> Result<Self, String> {
        start()?;
        let gtk_sink = gst::ElementFactory::make("gtk4paintablesink")
            .build()
            .map_err(|e| e.to_string())?;
        let paintable = gtk_sink.property::<gdk::Paintable>("paintable");
        let sink = if paintable
            .property::<Option<gdk::GLContext>>("gl-context")
            .is_some()
        {
            gst::ElementFactory::make("glsinkbin")
                .property("sink", &gtk_sink)
                .build()
                .map_err(|e| e.to_string())?
        } else {
            gtk_sink
        };
        let audio = gst::ElementFactory::make("fakesink")
            .build()
            .map_err(|e| e.to_string())?;
        let pipeline = gst::ElementFactory::make("playbin")
            .property("uri", gio::File::for_path(path).uri())
            .property("video-sink", &sink)
            .property("audio-sink", &audio)
            .property("mute", true)
            .build()
            .map_err(|e| e.to_string())?;
        let weak = pipeline.downgrade();
        let bus = pipeline.bus().ok_or("Video pipeline has no bus")?;
        let watch = bus
            .add_watch_local(move |_, message| {
                let Some(pipeline) = weak.upgrade() else {
                    return glib::ControlFlow::Break;
                };
                match message.view() {
                    gst::MessageView::Eos(_) => {
                        if pipeline
                            .seek_simple(
                                gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT,
                                gst::ClockTime::ZERO,
                            )
                            .is_err()
                        {
                            let _ = pipeline.set_state(gst::State::Null);
                        }
                    }
                    gst::MessageView::Error(_) => {
                        eprintln!("Background video failed; keeping opaque background");
                        let _ = pipeline.set_state(gst::State::Null);
                    }
                    _ => {}
                }
                glib::ControlFlow::Continue
            })
            .map_err(|e| e.to_string())?;
        picture.set_paintable(Some(&paintable));
        let playback = Self {
            pipeline,
            _watch: watch,
        };
        playback
            .pipeline
            .set_state(gst::State::Playing)
            .map_err(|e| e.to_string())?;
        Ok(playback)
    }
}

impl Drop for Playback {
    fn drop(&mut self) {
        let _ = self.pipeline.set_state(gst::State::Null);
    }
}

#[cfg(test)]
mod tests {
    use gtk::prelude::*;
    use std::{path::PathBuf, time::Duration};
    /// The library thumbnails depend on this, and a silent failure only shows up as
    /// a generic icon, which is easy to mistake for a design choice.
    #[test]
    fn a_bundled_video_yields_one_decoded_frame_at_the_requested_width() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("assets/media/videos/osaka_dotombori.mp4");
        let texture = super::frame(&path, 176, Duration::from_secs(10)).unwrap();
        assert_eq!(texture.width(), 176);
        assert!(texture.height() > 0 && texture.height() < 176);
        assert!(
            super::frame(
                &path.with_file_name("absent.mp4"),
                176,
                Duration::from_secs(2)
            )
            .is_err()
        );
    }
}
