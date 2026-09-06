//! Muted, looping local video through the official GTK4 GStreamer sink.
use gstreamer::{self as gst, prelude::*};
use gtk::{gdk, gio, prelude::*};
use std::path::Path;

pub struct Playback {
    pipeline: gst::Element,
    _watch: gst::bus::BusWatchGuard,
}

impl Playback {
    pub fn new(path: &Path, picture: &gtk::Picture) -> Result<Self, String> {
        gst::init().map_err(|e| e.to_string())?;
        gstgtk4::plugin_register_static().map_err(|e| e.to_string())?;
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
