//! Audio, video, and the streaming manifests and subtitle sidecars
//! that travel with them.

/// See [`super::MEDIA_TYPES`] for the invariants every group obeys.
pub(super) const TYPES: &[(&[&str], &str)] = &[
    // ── Audio ───────────────────────────────────────────────────────────
    (&["mp3", "mp2", "mpga"], "audio/mpeg"),
    (&["m4a", "m4b"], "audio/mp4"),
    (&["aac"], "audio/aac"),
    // ALAC normally travels inside `.m4a`; a bare `.alac` is raw and has no
    // registered type.
    (&["alac"], "audio/x-alac"),
    (&["wav"], "audio/wav"),
    (&["flac"], "audio/flac"),
    (&["ogg", "oga", "opus"], "audio/ogg"),
    (&["weba"], "audio/webm"),
    (&["mka"], "audio/x-matroska"),
    (&["wma"], "audio/x-ms-wma"),
    (&["aif", "aiff", "aifc"], "audio/x-aiff"),
    (&["amr"], "audio/amr"),
    (&["mid", "midi"], "audio/midi"),
    (&["au", "snd"], "audio/basic"),
    (&["ra"], "audio/x-realaudio"),
    (&["caf"], "audio/x-caf"),
    (&["3ga"], "audio/3gpp"),
    (&["m3u"], "audio/x-mpegurl"),
    (&["pls"], "audio/x-scpls"),
    // ── Video ───────────────────────────────────────────────────────────
    (&["mp4", "m4v"], "video/mp4"),
    (&["mov", "qt"], "video/quicktime"),
    (&["webm"], "video/webm"),
    (&["ogv"], "video/ogg"),
    (&["mkv", "mk3d"], "video/x-matroska"),
    (&["avi"], "video/x-msvideo"),
    (&["wmv"], "video/x-ms-wmv"),
    (&["asf"], "video/x-ms-asf"),
    (&["flv"], "video/x-flv"),
    (&["f4v"], "video/x-f4v"),
    (&["3gp"], "video/3gpp"),
    (&["3g2"], "video/3gpp2"),
    (&["mpg", "mpeg", "mpe", "m1v", "m2v", "vob"], "video/mpeg"),
    // `.ts` is both TypeScript source and an MPEG transport stream. HLS
    // segments are what actually gets served from object storage, and typing
    // them as text would break playback; TypeScript sources are compiled
    // before they are published, so they lose the tie.
    (&["ts", "m2ts", "mts"], "video/mp2t"),
    (&["rm", "rmvb"], "application/vnd.rn-realmedia"),
    (&["m3u8"], "application/vnd.apple.mpegurl"),
    (&["mpd"], "application/dash+xml"),
    (&["vtt"], "text/vtt; charset=utf-8"),
    (&["srt"], "application/x-subrip"),
    (&["ass", "ssa"], "text/x-ssa; charset=utf-8"),
];
