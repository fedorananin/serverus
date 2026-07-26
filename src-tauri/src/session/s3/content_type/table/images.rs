//! Fonts and images, including the vendor raw camera formats.

/// See [`super::MEDIA_TYPES`] for the invariants every group obeys.
pub(super) const TYPES: &[(&[&str], &str)] = &[
    // ── Fonts. Octet-stream costs the correct cache heuristics and trips
    // some browsers' font sanitizers.
    (&["woff"], "font/woff"),
    (&["woff2"], "font/woff2"),
    (&["ttf"], "font/ttf"),
    (&["otf"], "font/otf"),
    (&["ttc"], "font/collection"),
    (&["eot"], "application/vnd.ms-fontobject"),
    // ── Images ──────────────────────────────────────────────────────────
    (&["png"], "image/png"),
    (&["apng"], "image/apng"),
    (&["jpg", "jpeg", "jpe", "jfif"], "image/jpeg"),
    (&["jxl"], "image/jxl"),
    (&["gif"], "image/gif"),
    (&["webp"], "image/webp"),
    (&["avif"], "image/avif"),
    (&["heic"], "image/heic"),
    (&["heif"], "image/heif"),
    (&["bmp"], "image/bmp"),
    (&["tif", "tiff"], "image/tiff"),
    (&["ico"], "image/x-icon"),
    (&["jp2"], "image/jp2"),
    (&["djvu", "djv"], "image/vnd.djvu"),
    (&["psd", "psb"], "image/vnd.adobe.photoshop"),
    (&["xcf"], "image/x-xcf"),
    (&["tga"], "image/x-tga"),
    (&["exr"], "image/x-exr"),
    (&["hdr"], "image/vnd.radiance"),
    (&["ai", "eps", "ps"], "application/postscript"),
    // Raw camera formats, by vendor.
    (&["dng"], "image/x-adobe-dng"),
    (&["cr2"], "image/x-canon-cr2"),
    (&["cr3"], "image/x-canon-cr3"),
    (&["nef"], "image/x-nikon-nef"),
    (&["arw"], "image/x-sony-arw"),
    (&["orf"], "image/x-olympus-orf"),
    (&["raf"], "image/x-fuji-raf"),
    (&["rw2"], "image/x-panasonic-rw2"),
];
