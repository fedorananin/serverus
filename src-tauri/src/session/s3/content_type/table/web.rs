//! Web assets, text and structured data, and source/config files.
//!
//! The web group is the reason this module exists: these are the types
//! a browser checks strictly and refuses to guess at.

/// See [`super::MEDIA_TYPES`] for the invariants every group obeys.
pub(super) const TYPES: &[(&[&str], &str)] = &[
    // ── Web assets — the reason this module exists ──────────────────────
    (&["css"], "text/css; charset=utf-8"),
    (&["js", "mjs", "cjs"], "text/javascript; charset=utf-8"),
    (&["json", "map"], "application/json"),
    (&["jsonl", "ndjson"], "application/x-ndjson"),
    (&["jsonld"], "application/ld+json"),
    (&["wasm"], "application/wasm"),
    (&["webmanifest"], "application/manifest+json"),
    (&["html", "htm"], "text/html; charset=utf-8"),
    (&["xhtml"], "application/xhtml+xml; charset=utf-8"),
    (&["xml"], "application/xml; charset=utf-8"),
    (&["xsl", "xslt"], "application/xslt+xml"),
    (&["rss"], "application/rss+xml"),
    (&["atom"], "application/atom+xml"),
    (&["svg"], "image/svg+xml"),
    // ── Text and structured data ────────────────────────────────────────
    (&["txt"], "text/plain; charset=utf-8"),
    (&["md", "markdown"], "text/markdown; charset=utf-8"),
    (&["csv"], "text/csv; charset=utf-8"),
    (&["tsv"], "text/tab-separated-values; charset=utf-8"),
    (&["yaml", "yml"], "application/yaml"),
    (&["ics"], "text/calendar; charset=utf-8"),
    (&["vcf"], "text/vcard; charset=utf-8"),
    (&["ttl"], "text/turtle; charset=utf-8"),
    (&["geojson"], "application/geo+json"),
    (&["gpx"], "application/gpx+xml"),
    (&["kml"], "application/vnd.google-earth.kml+xml"),
    (&["kmz"], "application/vnd.google-earth.kmz"),
    (&["sql"], "application/sql"),
    (&["parquet"], "application/vnd.apache.parquet"),
    (&["db", "sqlite", "sqlite3"], "application/vnd.sqlite3"),
    (&["torrent"], "application/x-bittorrent"),
    // ── Source and config: text, so a public URL is readable in a browser
    // rather than downloaded. Secrets are excluded on purpose — see the
    // bottom of the file.
    (
        &["sh", "bash", "zsh", "fish", "bat", "cmd", "ps1"],
        "text/plain; charset=utf-8",
    ),
    (
        &[
            "c", "h", "cc", "cpp", "cxx", "hpp", "hxx", "cs", "java", "kt", "kts", "go", "rs",
            "swift", "scala", "dart",
        ],
        "text/plain; charset=utf-8",
    ),
    (
        &[
            "py", "rb", "pl", "pm", "php", "lua", "jl", "ex", "exs", "erl", "hs", "clj",
        ],
        "text/plain; charset=utf-8",
    ),
    (
        &["jsx", "tsx", "vue", "svelte", "graphql", "gql", "proto"],
        "text/plain; charset=utf-8",
    ),
    (
        &["scss", "sass", "less", "styl"],
        "text/plain; charset=utf-8",
    ),
    (
        &["toml", "ini", "cfg", "conf", "properties", "service"],
        "text/plain; charset=utf-8",
    ),
    (
        &["tf", "tfvars", "hcl", "gradle", "cmake"],
        "text/plain; charset=utf-8",
    ),
    (
        &["log", "diff", "patch", "cue"],
        "text/plain; charset=utf-8",
    ),
];
