// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Jareer and Concat contributors

//! What crosses the API boundary: a [`Request`] in, a [`Response`] out,
//! and [`Event`]s along the way.
//!
//! All three are serde types with a fixed JSON shape, and that shape is the
//! contract: a transport is free to carry it over stdin, a socket or a
//! function call, but never to reinterpret it. Requests are tagged by
//! `method`, events by `event`, and a response is `{"result": ...}` or
//! `{"error": {"code": ..., "message": ...}}`. Fields are camelCase,
//! matching the document and the command layer, so a caller learns one
//! spelling. On a line or a socket the three travel inside the JSON-RPC 2.0
//! envelope [`crate::rpc`] describes, which adds an id to tie a response to
//! its request and nothing else.
//!
//! A method that takes longer than a caller should wait - an export - is a
//! job: the response names it at once, and everything that happens to it
//! comes back as [`Event`]s carrying that name, ending in `export.done` or
//! `export.failed`. A refusal is an [`ApiError`]: a [`ErrorCode`] a program
//! can branch on and the sentence a person would be shown.
//!
//! The edit vocabulary is not redefined here. [`Request::EditApply`] carries
//! a `concat_project` [`Command`] as it is, so every operation the window
//! can perform is one an API caller can perform, with the same clamps and
//! the same refusals, and a new command needs nothing added here.

use concat_host::export::Progress;
use concat_host::media::MediaSummary;
use concat_host::session::EditorView;
use concat_host::templates::TemplateInfo;
use concat_host::{AppDirs, ProjectInfo};
use concat_project::Command;
use concat_project::model::VideoSettings;
use concat_speech::transcribe::{Segment, TranscribeRequest, TranscriberStatus};
use concat_speech::tts::{SpeakRequest, SpeakResult, TtsStatus};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The version of this contract. Bumped when a method's shape changes in a
/// way a caller written against the previous one would misread; adding a
/// method or an optional field does not bump it.
pub const API_VERSION: &str = "0.2";

/// What a caller asks for. One variant per method, named `area.verb`.
///
/// Every method that touches a project names it by its folder, the same
/// path [`Request::ProjectOpen`] took; the API keeps one session per folder
/// and a method on a folder that is not open is an error, not a silent
/// open, so a caller always knows what it is editing.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(tag = "method", rename_all_fields = "camelCase")]
pub enum Request {
    /// The API's version, the build behind it, and what that build
    /// serves. The one method to call first: its reply's `capabilities`
    /// names the transports and optional features this server has, so a
    /// caller knows what to ask for before it asks.
    #[serde(rename = "version")]
    Version,

    /// Creates a project folder under `location`, named `name`, and opens
    /// it. Refuses a folder that already holds a project.
    #[serde(rename = "project.create")]
    ProjectCreate {
        /// The directory the project folder is made in.
        location: String,
        /// The project's name; the folder is named after it.
        name: String,
        /// Frame and rate; 1080p at 30 when absent.
        #[serde(default)]
        video: Option<VideoSettings>,
    },
    /// Opens a project folder. Opening one already open returns its state
    /// as it stands, edits and all.
    #[serde(rename = "project.open")]
    ProjectOpen {
        /// The project folder.
        path: String,
    },
    /// Closes an open project, saving first when asked. Unsaved edits are
    /// dropped otherwise.
    #[serde(rename = "project.close")]
    ProjectClose {
        /// The project folder.
        path: String,
        /// Whether to write the document before closing.
        #[serde(default)]
        save: bool,
    },
    /// The projects this machine opened most recently, newest first.
    #[serde(rename = "project.list")]
    ProjectList,
    /// The state of an open project.
    #[serde(rename = "project.get")]
    ProjectGet {
        /// The project folder.
        path: String,
    },
    /// The document exactly as a save would write it.
    #[serde(rename = "project.document")]
    ProjectDocument {
        /// The project folder.
        path: String,
    },
    /// Writes the document to the project folder.
    #[serde(rename = "project.save")]
    ProjectSave {
        /// The project folder.
        path: String,
        /// A new name for the project, when renaming.
        #[serde(default)]
        name: Option<String>,
    },
    /// Sets the active timeline's frame and rate, as an undoable edit.
    #[serde(rename = "project.setVideo")]
    ProjectSetVideo {
        /// The project folder.
        path: String,
        /// The frame and rate.
        video: VideoSettings,
    },

    /// Applies one edit command. The command's own notes say what each
    /// does; a refusal comes back as the sentence the window would show.
    #[serde(rename = "edit.apply")]
    EditApply {
        /// The project folder.
        path: String,
        /// The edit. Boxed because a command is the largest thing a
        /// request carries; the JSON is the command as it is.
        command: Box<Command>,
    },
    /// Steps the history back one edit.
    #[serde(rename = "edit.undo")]
    EditUndo {
        /// The project folder.
        path: String,
    },
    /// Steps the history forward one edit.
    #[serde(rename = "edit.redo")]
    EditRedo {
        /// The project folder.
        path: String,
    },

    /// Reports what is inside a media file without touching any project.
    #[serde(rename = "media.probe")]
    MediaProbe {
        /// The file.
        path: String,
    },
    /// Probes a file and adds it to a project's bin: what dropping a file
    /// on the window does. A path already in the bin is a no-op.
    #[serde(rename = "media.import")]
    MediaImport {
        /// The project folder.
        path: String,
        /// The file to import.
        file: String,
    },
    /// The audio waveform of a file: the min and max level in each time
    /// bucket, for a caller to draw a waveform or find silences and onsets
    /// itself (silence-trim, beat-sync). Reads the file; needs no open
    /// project.
    #[serde(rename = "media.peaks")]
    MediaPeaks {
        /// The media file.
        path: String,
        /// Which audio stream, by index; the first when absent.
        #[serde(default)]
        stream: Option<u32>,
    },

    /// Transcribes a clip's audio to timed caption segments with one of the
    /// whisper models Settings has downloaded. Reads only; it places nothing.
    /// The segments are seconds from the window's start, for the caller to
    /// turn into caption clips with [`Request::EditApply`], the way the
    /// window's Captions tool does. Refused when the server shares no
    /// transcriber - its [`Request::Version`] capabilities then omit
    /// "speech".
    #[serde(rename = "speech.transcribe")]
    SpeechTranscribe {
        /// The clip and the model; see [`TranscribeRequest`].
        #[serde(flatten)]
        request: TranscribeRequest,
    },
    /// Reads text in a chosen voice to a WAV in the project folder, with one
    /// of the TTS models Settings has downloaded. Returns the file's path,
    /// ready for [`Request::MediaImport`]. Refused when the server shares no
    /// voices - its [`Request::Version`] capabilities then omit "speech".
    #[serde(rename = "speech.tts")]
    SpeechTts {
        /// The text, the voice and where the file goes; see [`SpeakRequest`].
        #[serde(flatten)]
        request: SpeakRequest,
    },
    /// Lists the whisper models for `speech.transcribe` and the TTS models
    /// and voices for `speech.tts`, and which are downloaded, so a caller
    /// picks a valid `modelId` or `voice` without guessing. Reads disk only;
    /// answers whether or not the server shares the engines.
    #[serde(rename = "speech.models")]
    SpeechModels,
    /// Downloads a speech model `speech.models` offers, so a pipeline can
    /// fetch a whisper or TTS model without the window. Blocks until the
    /// download and unpack finish - tens to hundreds of megabytes - through
    /// the shared engine's one-at-a-time gate, so it never races the window's
    /// own download. A model already on disk is a no-op; refused when the
    /// matching engine is not shared.
    #[serde(rename = "speech.download")]
    SpeechDownload {
        /// "transcriber" for a whisper model, "tts" for a voice model.
        kind: String,
        /// The model id, exactly as `speech.models` lists it.
        model_id: String,
    },
    /// Removes a downloaded speech model to reclaim disk. `kind` is
    /// "transcriber" or "tts". Refused when the matching engine is not shared.
    #[serde(rename = "speech.deleteModel")]
    SpeechDeleteModel {
        /// "transcriber" or "tts".
        kind: String,
        /// The model id to remove.
        model_id: String,
    },
    /// Stops a running `speech.download`. A no-op when nothing is downloading.
    #[serde(rename = "speech.cancelDownload")]
    SpeechCancelDownload {
        /// "transcriber" or "tts".
        kind: String,
    },

    /// Writes an upscaled/restored copy of a clip's media and points the clip
    /// at it, as the window's Enhance does: the original stays in the bin, and
    /// `clip.enhance` can be undone. Blocks on the model (fetched on first
    /// use). Refused when the server shares no enhancer (its capabilities omit
    /// "enhance").
    #[serde(rename = "clip.enhance")]
    ClipEnhance {
        /// The project folder.
        path: String,
        /// The clip to enhance.
        clip_id: String,
        /// How many times larger; 2 when absent. The model clamps it to what
        /// it supports.
        #[serde(default)]
        factor: Option<u32>,
    },
    /// Writes a reversed copy of the span a clip shows and points the clip at
    /// it, as the window's Reverse does. Blocks. Refused when the server
    /// shares no reverser (its capabilities omit "reverse").
    #[serde(rename = "clip.reverse")]
    ClipReverse {
        /// The project folder.
        path: String,
        /// The clip to reverse.
        clip_id: String,
    },

    /// Turns timed segments into a run of styled caption clips on the open
    /// project's active timeline, in one batch - the one-call form of what the
    /// window's Captions tool does, so a pipeline gets a finished caption track
    /// without placing each clip itself. Give it `segments` (from
    /// `speech.transcribe`) or a `transcribe` request to run first. The
    /// `preset` chooses the look and how words are grouped. Refused when no
    /// project is open at `path`, and - when it must transcribe - when the
    /// server shares no transcriber.
    #[serde(rename = "caption.build")]
    CaptionBuild {
        /// The caption job; see [`CaptionRequest`].
        #[serde(flatten)]
        request: CaptionRequest,
    },

    /// Every effect package the build knows, with its parameters, so a
    /// caller can build a valid chain without reading a manifest.
    #[serde(rename = "catalogue.list")]
    CatalogueList {
        /// Restrict to one kind: "effect", "filter", "audio", "transition"
        /// or "generator".
        #[serde(default)]
        kind: Option<String>,
    },

    /// The template library.
    #[serde(rename = "template.list")]
    TemplateList,
    /// Makes a project from a template with every slot filled, and opens
    /// it. Every slot must be filled; a set that leaves one empty makes
    /// nothing.
    #[serde(rename = "template.instantiate")]
    TemplateInstantiate {
        /// The template bundle folder.
        template: String,
        /// The directory the project folder is made in.
        location: String,
        /// The project's name.
        name: String,
        /// The file for each slot.
        fills: Vec<Fill>,
    },
    /// Packs an open project into a new template bundle.
    #[serde(rename = "template.save")]
    TemplateSave {
        /// The project folder.
        path: String,
        /// The template's name.
        name: String,
    },

    /// Renders an open project's active timeline to a file, exactly as
    /// the window's Export does: cutouts analysed, titles painted, then the
    /// frame loop and the mix. Returns at once with the job's name; the
    /// render runs on its own thread and reports through [`Event`]s, ending
    /// in [`Event::ExportDone`] or [`Event::ExportFailed`]. One export runs
    /// at a time; a second is refused as [`ErrorCode::Busy`].
    #[serde(rename = "export.run")]
    ExportRun {
        /// The project folder.
        path: String,
        /// How to render it.
        #[serde(flatten)]
        spec: ExportSpec,
    },
    /// Stops a running export at its next frame. The job then ends with
    /// [`Event::ExportFailed`] carrying [`ErrorCode::Cancelled`].
    #[serde(rename = "export.cancel")]
    ExportCancel {
        /// The job [`Started`] named.
        job: String,
    },

    /// Composites the true frame at one instant. Written as a PNG to
    /// `output` when there is one, and handed back inline as a
    /// [`Picture`] otherwise, for a caller on the far side of a socket.
    #[serde(rename = "preview.frame")]
    PreviewFrame {
        /// The project folder.
        path: String,
        /// The timeline instant, in seconds.
        time: f64,
        /// The file to write; absent means the picture comes back inline.
        #[serde(default)]
        output: Option<String>,
        /// Frame width; the timeline's when absent.
        #[serde(default)]
        width: Option<u32>,
        /// Frame height; the timeline's when absent.
        #[serde(default)]
        height: Option<u32>,
    },
}

/// One template slot and the file that takes it.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Fill {
    /// The placeholder's media id in the template.
    pub media_id: String,
    /// The file to put there; probed on the way in.
    pub file: String,
}

/// How an export is rendered. Everything but the output is optional and
/// defaults to what the window's sheet defaults to.
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportSpec {
    /// The file to write.
    pub output: String,
    /// Constant rate factor; lower is better and bigger. 20 when absent.
    #[serde(default)]
    pub crf: Option<u8>,
    /// The x264 preset name. "medium" when absent.
    #[serde(default)]
    pub preset: Option<String>,
    /// Output width; the timeline's when absent.
    #[serde(default)]
    pub width: Option<u32>,
    /// Output height; the timeline's when absent.
    #[serde(default)]
    pub height: Option<u32>,
    /// Frame rate numerator; the timeline's when absent.
    #[serde(default)]
    pub rate_num: Option<i64>,
    /// Frame rate denominator; the timeline's when absent.
    #[serde(default)]
    pub rate_den: Option<i64>,
    /// "h264", "hevc" or "av1". H.264 when absent - HEVC for an HDR file,
    /// which H.264 does not carry.
    #[serde(default)]
    pub codec: Option<String>,
    /// Ten bits a channel. Eight when absent.
    #[serde(default)]
    pub ten_bit: Option<bool>,
    /// "limited" (16-235, what every player and YouTube expect) or "full"
    /// (0-255, for screen content bound for a PC player that reads the
    /// tag). Limited when absent.
    /// https://github.com/jub0t/Concat/issues/103
    #[serde(default)]
    pub color_range: Option<String>,
    /// For an HDR timeline: written as HDR, its own HLG or PQ in ten bits
    /// (true, the default), or tone-mapped to SDR (false). Nothing for an
    /// SDR timeline.
    #[serde(default)]
    pub hdr: Option<bool>,
}

/// How captions are grouped and styled for [`Request::CaptionBuild`].
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CaptionPreset {
    /// One caption per transcript segment - a whole spoken line at a time.
    /// The most readable default, and the only one that needs no word timings.
    #[default]
    Phrase,
    /// A few words at a time (see `maxWords`), the big centred look short-form
    /// video uses. Falls back to the whole segment when it has no word timings.
    PopWord,
}

/// Overrides on a preset's built-in look, for [`CaptionRequest`]. Every field
/// is optional; an absent one keeps the preset's value, so a caller tweaks one
/// thing without restating the rest.
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CaptionStyle {
    /// CSS family name, quotes included where the name needs them.
    pub font_family: Option<String>,
    /// Cap height as a fraction of frame height.
    pub font_size: Option<f64>,
    /// CSS weight, 100..=900.
    pub font_weight: Option<f64>,
    /// Fill colour, "#rrggbb".
    pub color: Option<String>,
    /// Outline thickness as a fraction of frame height; 0 for none.
    pub stroke_width: Option<f64>,
    /// Outline colour.
    pub stroke_color: Option<String>,
    /// Plate colour "#rrggbb[aa]"; empty string for none.
    pub background: Option<String>,
    /// Upper-case every caption's text.
    pub uppercase: Option<bool>,
    /// Widest a line runs before it wraps, as a fraction of frame width; 0 for
    /// no limit.
    pub max_width: Option<f64>,
    /// Vertical placement as a frame-height fraction, like `AddTextClip`.
    pub offset_y: Option<f64>,
}

/// The caption job for [`Request::CaptionBuild`].
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptionRequest {
    /// The open project to place captions in.
    pub path: String,
    /// The timed segments to turn into captions. Takes precedence over
    /// `transcribe` when both are given.
    #[serde(default)]
    pub segments: Option<Vec<Segment>>,
    /// Transcribe this first when `segments` is absent; see
    /// [`TranscribeRequest`].
    #[serde(default)]
    pub transcribe: Option<TranscribeRequest>,
    /// Which look and grouping; see [`CaptionPreset`].
    #[serde(default)]
    pub preset: CaptionPreset,
    /// Words per caption for the `popWord` preset; 3 when absent.
    #[serde(default)]
    pub max_words: Option<usize>,
    /// The lane to place captions on; a fresh lane above the picture when
    /// absent, the way the window's Captions tool lands them.
    #[serde(default)]
    pub track_id: Option<String>,
    /// Seconds added to every segment time, for when the transcribed audio
    /// does not begin at timeline zero. Zero when absent.
    #[serde(default)]
    pub timeline_offset: f64,
    /// Overrides on the preset look; see [`CaptionStyle`].
    #[serde(default)]
    pub style: Option<CaptionStyle>,
}

/// A file's audio envelope: `min[i]` and `max[i]` are the lowest and highest
/// sample in bucket `i`, and `bucketsPerSecond` maps a bucket back to time.
/// Low `max` over a run of buckets is silence; a sharp `max` rise is an onset.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioPeaks {
    /// The lowest sample in each bucket, `-1.0..=0.0`.
    pub min: Vec<f32>,
    /// The highest sample in each bucket, `0.0..=1.0`.
    pub max: Vec<f32>,
    /// Buckets per second, to turn a bucket index into seconds.
    pub buckets_per_second: f32,
}

/// What `speech.models` hands back: the whisper models for transcription,
/// and the TTS models and voices, each with what is downloaded.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechCatalogue {
    /// Whisper models for [`Request::SpeechTranscribe`].
    pub transcriber: TranscriberStatus,
    /// TTS models and voices for [`Request::SpeechTts`].
    pub tts: TtsStatus,
}

/// What [`Request::CaptionBuild`] hands back.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptionRun {
    /// How many caption clips were placed.
    pub clips: usize,
}

/// What a request hands back. Serialised as the payload alone: the variant
/// is implied by the method, so a caller that sent `project.get` reads an
/// editor view and nothing wraps it.
#[derive(Clone, Serialize)]
#[serde(untagged)]
pub enum Reply {
    /// [`Request::Version`].
    Version(VersionInfo),
    /// The project after a change, or as it stands.
    View(Box<EditorView>),
    /// [`Request::ProjectList`].
    Projects(Vec<ProjectInfo>),
    /// [`Request::ProjectDocument`].
    Document(Value),
    /// [`Request::MediaProbe`].
    Media(MediaSummary),
    /// [`Request::MediaPeaks`]: the file's audio envelope.
    Peaks(AudioPeaks),
    /// [`Request::SpeechTranscribe`]: the clip's words, timed from the
    /// window's start.
    Captions(Vec<Segment>),
    /// [`Request::SpeechTts`]: the WAV written, and how long it is.
    Spoken(SpeakResult),
    /// [`Request::SpeechModels`]: the speech models and voices on offer.
    SpeechModels(SpeechCatalogue),
    /// [`Request::CaptionBuild`]: how many caption clips were placed.
    Captioned(CaptionRun),
    /// [`Request::CatalogueList`].
    Packages(Vec<PackageInfo>),
    /// [`Request::TemplateList`].
    Templates(Vec<TemplateInfo>),
    /// [`Request::TemplateSave`].
    Template(TemplateInfo),
    /// [`Request::ExportRun`]: the job that is now running.
    Started(Started),
    /// [`Request::PreviewFrame`] with an output: the file written.
    Written(Written),
    /// [`Request::PreviewFrame`] without one: the picture itself.
    Picture(Picture),
    /// Methods with nothing to say beyond having worked.
    Done(Done),
}

/// The build and the contract it speaks.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionInfo {
    /// [`API_VERSION`].
    pub api_version: String,
    /// The Concat build.
    pub concat: String,
    /// Where this machine keeps recents, templates and models.
    pub dirs: Dirs,
    /// What this build serves, as names a caller tests for membership:
    /// what the API itself provides in every build, `events`, and what
    /// only some builds do, `gpu` when frames composite on one; then what
    /// the transport around it adds, `json-rpc`, `unix-socket` and `grpc`
    /// for each of the socket transports listening. A name absent from
    /// the list is not served, whatever the build; a name added later is
    /// not a version bump, so a caller ignores names it does not know.
    pub capabilities: Vec<String>,
}

/// The app's directories, as paths.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Dirs {
    /// Small state: recents, settings, the template library.
    pub config: String,
    /// Large state: downloaded models, painted titles.
    pub data: String,
}

impl From<&AppDirs> for Dirs {
    fn from(dirs: &AppDirs) -> Self {
        Dirs {
            config: dirs.config.to_string_lossy().into_owned(),
            data: dirs.data.to_string_lossy().into_owned(),
        }
    }
}

/// A file the API wrote.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Written {
    /// The file's path.
    pub path: String,
    /// Its picture size.
    pub width: u32,
    /// Its picture size.
    pub height: u32,
}

/// A job the API has begun. Every event about it carries `job`.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Started {
    /// The job's name, unique for the life of the API: "j1", "j2", ...
    pub job: String,
    /// The project it works on.
    pub path: String,
    /// The file it will write.
    pub output: String,
}

/// A frame handed back without touching the disk.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Picture {
    /// Its picture size.
    pub width: u32,
    /// Its picture size.
    pub height: u32,
    /// The PNG, base64 in the standard alphabet with padding.
    pub png: String,
}

/// The empty reply, an object so every reply is one.
#[derive(Clone, Copy, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct Done {}

/// One effect package, as a caller building a chain needs it.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageInfo {
    /// The id a clip's chain stores, `author.name`.
    pub id: String,
    /// What the catalogue card says.
    pub name: String,
    /// "effect", "filter", "audio", "transition" or "generator".
    pub kind: String,
    /// The shelf the card sits on.
    pub category: String,
    /// A sentence about it.
    pub description: String,
    /// The parameter the simple view shows as the one slider, if any.
    pub intensity: Option<String>,
    /// The knobs, in the order the inspector shows them.
    pub params: Vec<ParamInfo>,
}

/// One parameter of a package.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParamInfo {
    /// The key a chain entry's `params` stores it under.
    pub key: String,
    /// What the control is labelled.
    pub label: String,
    /// "float", "int", "bool", "enum", "color" or "point".
    #[serde(rename = "type")]
    pub kind: String,
    /// Lowest value.
    pub min: f64,
    /// Highest value.
    pub max: f64,
    /// The value an untouched control means.
    pub default: f64,
    /// Slider increment; 0 means continuous.
    pub step: f64,
    /// Displayed after the number.
    pub unit: String,
    /// Whether the control can carry keyframes.
    pub animate: bool,
    /// For "enum": the values the document may hold.
    pub values: Vec<f64>,
    /// For "enum": what each value is called, in `values` order.
    pub labels: Vec<String>,
}

/// Why a request has no reply, as a program reads it. The JSON-RPC number
/// each maps to is [`ErrorCode::number`].
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ErrorCode {
    /// The line was not JSON.
    Parse,
    /// The JSON was not a request: an unknown method, a missing or
    /// ill-typed field, a value out of range.
    Invalid,
    /// The project named is not open; open it first.
    NotOpen,
    /// A job, template or package named does not exist.
    NotFound,
    /// The edit layer said no. The message is the sentence the window
    /// would show, and the state is as it was.
    Refused,
    /// A one-at-a-time job is already running.
    Busy,
    /// The job was stopped by [`Request::ExportCancel`].
    Cancelled,
    /// The transport wanted a token and did not get the right one. Never
    /// raised by the API itself.
    Unauthorized,
    /// Everything else: a file that could not be read or written, a decode
    /// that failed, a model that did not download.
    Failed,
}

impl ErrorCode {
    /// The code's JSON-RPC 2.0 number: the standard ones for the standard
    /// meanings, and server-defined ones (-32000 downwards) for the rest.
    pub fn number(self) -> i64 {
        match self {
            ErrorCode::Parse => -32700,
            ErrorCode::Invalid => -32600,
            ErrorCode::Failed => -32000,
            ErrorCode::NotOpen => -32001,
            ErrorCode::NotFound => -32002,
            ErrorCode::Refused => -32003,
            ErrorCode::Busy => -32004,
            ErrorCode::Cancelled => -32005,
            ErrorCode::Unauthorized => -32006,
        }
    }
}

/// A refusal: what kind, and the sentence a person would be shown.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiError {
    /// What kind of refusal.
    pub code: ErrorCode,
    /// The sentence.
    pub message: String,
}

impl ApiError {
    /// An error of `code` saying `message`.
    pub fn new(code: ErrorCode, message: impl Into<String>) -> ApiError {
        ApiError {
            code,
            message: message.into(),
        }
    }

    /// [`ErrorCode::Invalid`].
    pub fn invalid(message: impl Into<String>) -> ApiError {
        ApiError::new(ErrorCode::Invalid, message)
    }

    /// [`ErrorCode::Failed`].
    pub fn failed(message: impl Into<String>) -> ApiError {
        ApiError::new(ErrorCode::Failed, message)
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ApiError {}

/// A request's outcome. `{"result": ...}` when it worked, `{"error":
/// {"code": ..., "message": ...}}` when it did not.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[allow(clippy::large_enum_variant)]
pub enum Response {
    /// The reply.
    Result(Reply),
    /// Why there is none.
    Error(ApiError),
}

impl From<Result<Reply, ApiError>> for Response {
    fn from(result: Result<Reply, ApiError>) -> Self {
        match result {
            Ok(reply) => Response::Result(reply),
            Err(error) => Response::Error(error),
        }
    }
}

/// Something that happened to a job. A transport forwards these to every
/// caller as they come; each names its job and its project, so a caller
/// keeps the ones it asked for and ignores the rest.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(tag = "event", rename_all_fields = "camelCase")]
pub enum Event {
    /// A cutout's masks are being found ahead of an export.
    #[serde(rename = "cutout.progress")]
    CutoutProgress {
        /// The job.
        job: String,
        /// The project folder.
        path: String,
        /// The media being analysed.
        media_id: String,
        /// True while a model downloads, false while it runs.
        fetching: bool,
        /// How far along, `0..=1`.
        fraction: f32,
    },
    /// The export moved on.
    #[serde(rename = "export.progress")]
    ExportProgress {
        /// The job.
        job: String,
        /// The project folder.
        path: String,
        /// Frames done.
        frame: i64,
        /// Frames in total.
        total: i64,
        /// "video", "audio" or "mux".
        stage: String,
    },
    /// The export wrote its file. The job is over.
    #[serde(rename = "export.done")]
    ExportDone {
        /// The job.
        job: String,
        /// The project folder.
        path: String,
        /// The file written.
        output: String,
        /// Its picture size.
        width: u32,
        /// Its picture size.
        height: u32,
    },
    /// The export did not write its file. The job is over.
    #[serde(rename = "export.failed")]
    ExportFailed {
        /// The job.
        job: String,
        /// The project folder.
        path: String,
        /// Why: [`ErrorCode::Cancelled`] when asked to stop, otherwise what
        /// went wrong.
        error: ApiError,
    },
}

impl Event {
    /// [`Event::ExportProgress`] for one report from the renderer.
    pub fn progress(job: &str, path: &str, progress: Progress) -> Event {
        Event::ExportProgress {
            job: job.to_owned(),
            path: path.to_owned(),
            frame: progress.frame,
            total: progress.total,
            stage: progress.stage.to_owned(),
        }
    }

    /// The job this event is about.
    pub fn job(&self) -> &str {
        match self {
            Event::CutoutProgress { job, .. }
            | Event::ExportProgress { job, .. }
            | Event::ExportDone { job, .. }
            | Event::ExportFailed { job, .. } => job,
        }
    }
}
