use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ThemePreference {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, Type)]
pub struct AppearanceSettings {
    pub theme: ThemePreference,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct SecuritySettings {
    /// 0 = never.
    pub auto_lock_minutes: u32,
    pub lock_on_sleep: bool,
    pub touch_id: bool,
}

impl Default for SecuritySettings {
    fn default() -> Self {
        SecuritySettings {
            auto_lock_minutes: 15,
            lock_on_sleep: true,
            touch_id: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ConflictPolicy {
    Ask,
    Overwrite,
    Skip,
    Rename,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct TransferSettings {
    pub max_parallel_per_server: u32,
    pub conflict_policy: ConflictPolicy,
    pub preserve_mtime: bool,
    pub tar_acceleration: bool,
}

impl Default for TransferSettings {
    fn default() -> Self {
        TransferSettings {
            max_parallel_per_server: 5,
            conflict_policy: ConflictPolicy::Ask,
            preserve_mtime: true,
            tar_acceleration: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct EditorSettings {
    /// When false, `custom_app` names the editor application to use.
    pub use_system_default: bool,
    pub custom_app: Option<String>,
}

impl Default for EditorSettings {
    fn default() -> Self {
        EditorSettings {
            use_system_default: true,
            custom_app: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct TerminalSettings {
    pub font_family: String,
    pub font_size: u16,
    pub scrollback: u32,
    /// Copy the selection to the clipboard automatically. Off by default —
    /// it silently clobbers whatever the user copied elsewhere.
    #[serde(default)]
    pub copy_on_select: bool,
}

impl Default for TerminalSettings {
    fn default() -> Self {
        // xterm.js takes a CSS font-family list, so fallbacks are free.
        let font_family = if cfg!(target_os = "macos") {
            "SF Mono"
        } else if cfg!(target_os = "windows") {
            "Cascadia Mono, Consolas, monospace"
        } else {
            "DejaVu Sans Mono, monospace"
        };
        TerminalSettings {
            font_family: font_family.into(),
            font_size: 13,
            scrollback: 10_000,
            copy_on_select: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum SizeFormat {
    /// Powers of 1000 (KB).
    Kb,
    /// Powers of 1024 (KiB).
    Kib,
}

/// Sidebar width bounds, in CSS pixels. The floor keeps connection names
/// readable; the ceiling keeps the file panes and terminal usable at the
/// minimum window width (940, see `tauri.conf.json`).
pub const SIDEBAR_WIDTH_MIN: u16 = 200;
pub const SIDEBAR_WIDTH_MAX: u16 = 380;
pub const SIDEBAR_WIDTH_DEFAULT: u16 = 230;

/// File-pane column width bounds, in CSS pixels. The floor keeps a header
/// label grabbable; the ceiling leaves the Name column room in a half-width
/// pane.
pub const COLUMN_WIDTH_MIN: u16 = 40;
pub const COLUMN_WIDTH_MAX: u16 = 320;

/// Widths of the fixed file-pane columns, in CSS pixels; Name takes the rest.
/// Shared by the local and remote panes of every tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct ColumnWidths {
    pub size: u16,
    /// Wide enough for `29.10.2026` and `29.10 14:30` in the platform's
    /// monospace font (Consolas runs wider than SF Mono).
    pub date: u16,
    /// Unix mode (`rwxr-xr-x`), or the S3 access column.
    pub mode: u16,
}

impl Default for ColumnWidths {
    fn default() -> Self {
        ColumnWidths {
            size: 74,
            date: 100,
            mode: 80,
        }
    }
}

impl ColumnWidths {
    fn clamp(&mut self) {
        for width in [&mut self.size, &mut self.date, &mut self.mode] {
            *width = (*width).clamp(COLUMN_WIDTH_MIN, COLUMN_WIDTH_MAX);
        }
    }
}

fn default_hide_local_junk() -> bool {
    true
}

fn default_sidebar_width() -> u16 {
    SIDEBAR_WIDTH_DEFAULT
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct PanelSettings {
    pub show_hidden: bool,
    /// Never show OS metadata junk (`.DS_Store`, `Thumbs.db`) in the local
    /// pane — regardless of `show_hidden` — keep it out of the local side of
    /// folder comparison, and leave it out of directory transfers in either
    /// direction. The remote pane and remote side of comparison still show
    /// such files so stray uploads surface as differences and can be deleted.
    /// On by default (also for vaults saved before the setting existed).
    #[serde(default = "default_hide_local_junk")]
    pub hide_local_junk: bool,
    pub size_format: SizeFormat,
    pub default_local_dir: Option<String>,
    /// Sidebar width in CSS pixels, always within
    /// [`SIDEBAR_WIDTH_MIN`]..=[`SIDEBAR_WIDTH_MAX`].
    #[serde(default = "default_sidebar_width")]
    pub sidebar_width: u16,
    /// Missing in vaults written before columns were resizable.
    #[serde(default)]
    pub columns: ColumnWidths,
}

impl Default for PanelSettings {
    fn default() -> Self {
        PanelSettings {
            show_hidden: false,
            hide_local_junk: true,
            size_format: SizeFormat::Kib,
            default_local_dir: None,
            sidebar_width: SIDEBAR_WIDTH_DEFAULT,
            columns: ColumnWidths::default(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, Type)]
pub struct Settings {
    /// Missing in vaults written before theme selection was introduced.
    #[serde(default)]
    pub appearance: AppearanceSettings,
    pub security: SecuritySettings,
    pub transfers: TransferSettings,
    pub editor: EditorSettings,
    pub terminal: TerminalSettings,
    pub panels: PanelSettings,
    /// Missing in vaults written before AI agent access existed.
    #[serde(default)]
    pub agent: super::AgentSettings,
}

impl Settings {
    /// Force values that would wedge the UI back into range. The frontend
    /// clamps while dragging, but a hand-edited vault must not be able to
    /// leave the sidebar or a pane column unusably wide or narrow.
    pub fn clamp(&mut self) {
        self.panels.sidebar_width = self
            .panels
            .sidebar_width
            .clamp(SIDEBAR_WIDTH_MIN, SIDEBAR_WIDTH_MAX);
        self.panels.columns.clamp();
    }
}
